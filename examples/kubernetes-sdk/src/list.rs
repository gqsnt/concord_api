//! Chunked lists: the Kubernetes `limit`/`continue` contract as an adapter for
//! `concord-pagination`'s sequential traversal.
//!
//! # The provider contract (Kubernetes API concepts, "Retrieving large results sets in chunks")
//!
//! - A list request with `limit=n` returns at most `n` items (possibly **zero** while more exist)
//!   and a non-empty `metadata.continue` while more chunks exist. Only the presence of `continue`
//!   decides whether to go on; an empty page with a token continues.
//! - The token is opaque and valid for the identical query only. It is passed back unmodified;
//!   this adapter never builds, edits or predicts one. A continued chunk that returns the token
//!   it was requested with would repeat itself forever (only the caller's budget would end the
//!   chain): it is a [`ListError::ContinueDidNotAdvance`], and its items are not returned.
//! - Every chunk of one chain reports the **same** collection `metadata.resourceVersion` (the
//!   consistent snapshot). [`ListPager::fetch`] checks it: the snapshot travels inside the page key
//!   ([`Continuation`]), and a continued chunk that reports another one is a
//!   [`ListError::InconsistentSnapshot`] instead of data from a different snapshot.
//! - A continued request carries `continue` and `limit` but no `resourceVersion` /
//!   `resourceVersionMatch` (the token encodes the snapshot; a specific `resourceVersion` next to
//!   `continue` is `400 Bad Request` per the list semantics table). The adapter clears them.
//! - `remainingItemCount` is an optional **estimate** (absent with selectors and on the last
//!   chunk). It is reported in [`ListPageMeta`] and never used to plan, size or parallelize
//!   requests: a chain is strictly sequential because each token is known only after the previous
//!   chunk.
//! - An expired token (or an unavailable `resourceVersion`) is `410 Gone` with a `Status` of
//!   reason `Expired`: [`ListError::Expired`]. The consistent list must restart from the
//!   beginning. The `410` may carry a token for an **inconsistent** continuation; this adapter does
//!   not use it (see [`ApiError::offers_inconsistent_continue`]).

use std::fmt;
use std::future::Future;
use std::num::NonZeroU32;

use crate::error::ApiError;
use crate::ids::{ContinueToken, ResourceVersion};
use crate::model::{ObjectList, Resource};
use crate::selector::{FieldSelector, LabelSelector};
use crate::{
    KubeAppsV1Deployments, KubeAppsV1NamespacedDeployments, KubeBatchV1Jobs,
    KubeBatchV1NamespacedJobs, KubeCoreV1ConfigMaps, KubeCoreV1NamespacedConfigMaps,
    KubeCoreV1NamespacedPods, KubeCoreV1NamespacedServices, KubeCoreV1Namespaces, KubeCoreV1Pods,
    KubeCoreV1Services, KubeNetworkingV1Ingresses, KubeNetworkingV1NamespacedIngresses,
    ResourceVersionMatch,
};

pub use concord_pagination::Page;

/// A generated list operation (one of the thirteen `GET .../{plural}` endpoints), seen through the
/// parameters every Kubernetes list shares. Each method forwards to the generated `maybe_*`
/// setter of the same parameter, so the request stays an ordinary generated operation.
pub trait ListOperation: Clone + Send + Sync + 'static {
    /// The object type of the collection.
    type Item: Resource;

    /// `labelSelector` (`None`: absent).
    fn with_label_selector(self, selector: Option<LabelSelector>) -> Self;
    /// `fieldSelector` (`None`: absent).
    fn with_field_selector(self, selector: Option<FieldSelector>) -> Self;
    /// `limit` (`None`: absent, the whole collection in one response).
    fn with_limit(self, limit: Option<u32>) -> Self;
    /// `continue` (`None`: absent, the first chunk).
    fn with_continue(self, token: Option<ContinueToken>) -> Self;
    /// `resourceVersion` and `resourceVersionMatch`.
    fn with_resource_version(
        self,
        version: Option<ResourceVersion>,
        matching: Option<ResourceVersionMatch>,
    ) -> Self;
    /// `timeoutSeconds`.
    fn with_timeout_seconds(self, seconds: Option<u32>) -> Self;
    /// The generated `send()`: one request, the decoded list envelope.
    fn send_list(
        self,
    ) -> impl Future<Output = Result<ObjectList<Self::Item>, concord::Error>> + Send;
}

macro_rules! list_operation {
    ($($op:ty => $item:ty),* $(,)?) => {$(
        impl ListOperation for $op {
            type Item = $item;
            fn with_label_selector(self, selector: Option<LabelSelector>) -> Self {
                self.maybe_label_selector(selector)
            }
            fn with_field_selector(self, selector: Option<FieldSelector>) -> Self {
                self.maybe_field_selector(selector)
            }
            fn with_limit(self, limit: Option<u32>) -> Self {
                self.maybe_limit(limit)
            }
            fn with_continue(self, token: Option<ContinueToken>) -> Self {
                self.maybe_continue_token(token)
            }
            fn with_resource_version(
                self,
                version: Option<ResourceVersion>,
                matching: Option<ResourceVersionMatch>,
            ) -> Self {
                self.maybe_resource_version(version)
                    .maybe_resource_version_match(matching)
            }
            fn with_timeout_seconds(self, seconds: Option<u32>) -> Self {
                self.maybe_timeout_seconds(seconds)
            }
            fn send_list(
                self,
            ) -> impl Future<Output = Result<ObjectList<Self::Item>, concord::Error>> + Send {
                <$op>::send(self)
            }
        }
    )*};
}

list_operation! {
    KubeCoreV1Namespaces => crate::model::Namespace,
    KubeCoreV1Pods => crate::model::Pod,
    KubeCoreV1Services => crate::model::Service,
    KubeCoreV1ConfigMaps => crate::model::ConfigMap,
    KubeCoreV1NamespacedPods => crate::model::Pod,
    KubeCoreV1NamespacedServices => crate::model::Service,
    KubeCoreV1NamespacedConfigMaps => crate::model::ConfigMap,
    KubeAppsV1Deployments => crate::model::Deployment,
    KubeAppsV1NamespacedDeployments => crate::model::Deployment,
    KubeBatchV1Jobs => crate::model::Job,
    KubeBatchV1NamespacedJobs => crate::model::Job,
    KubeNetworkingV1Ingresses => crate::model::Ingress,
    KubeNetworkingV1NamespacedIngresses => crate::model::Ingress,
}

/// The page key of a chunked list: the server's `continue` token **and** the collection
/// snapshot (`resourceVersion`) of the chunk that returned it. The next chunk must report the same
/// snapshot. Debug shows the snapshot and withholds the token.
#[derive(Clone, PartialEq, Eq)]
pub struct Continuation {
    token: ContinueToken,
    snapshot: ResourceVersion,
}

impl Continuation {
    /// A key restored from a stored checkpoint: a token **received from the server** and the
    /// snapshot of the chunk that returned it. Never fabricate either.
    pub fn resume(token: ContinueToken, snapshot: ResourceVersion) -> Self {
        Self { token, snapshot }
    }

    /// The opaque token (read deliberately; do not log it).
    pub fn token(&self) -> &ContinueToken {
        &self.token
    }

    /// The collection snapshot the chain is bound to.
    pub fn snapshot(&self) -> &ResourceVersion {
        &self.snapshot
    }
}

impl fmt::Debug for Continuation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Continuation")
            .field("token", &format_args!("<redacted>"))
            .field("snapshot", &self.snapshot.as_str())
            .finish()
    }
}

/// What one chunk reported besides its items.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListPageMeta {
    /// The collection `resourceVersion` (the snapshot), if the server sent one.
    pub snapshot: Option<ResourceVersion>,
    /// `remainingItemCount` as reported (an estimate; often absent).
    pub remaining_item_count: Option<i64>,
}

/// One chunk: moved items, the next key, the chunk metadata.
pub type ListPage<T> = Page<T, Continuation, ListPageMeta>;

/// Why a chunk failed.
#[derive(Debug)]
#[non_exhaustive]
pub enum ListError {
    /// Concord's error (not built, transport, refused head, undecodable body), kept whole.
    Request(concord::Error),
    /// A non-2xx answer other than `410`.
    Api(ApiError),
    /// `410 Gone`: the continue token expired, or the requested `resourceVersion` is no longer
    /// available. A consistent list restarts from the first chunk.
    Expired(ApiError),
    /// A continued chunk reported a different collection snapshot than the chunk whose token it
    /// used (or none at all). Its items were not returned.
    InconsistentSnapshot,
    /// A chunk returned a `continue` token without a collection `resourceVersion`, so the next
    /// chunk's consistency could not be checked.
    MissingSnapshot,
    /// A continued chunk returned the `continue` token it was requested with: following it
    /// would request the same chunk again. Its items were not returned.
    ContinueDidNotAdvance,
}

impl ListError {
    /// Classifies a Concord error, reading the `Status` document of a non-2xx answer.
    pub async fn from_concord(error: concord::Error) -> Self {
        match ApiError::read(error).await {
            Ok(api) if api.is_expired() => Self::Expired(api),
            Ok(api) => Self::Api(api),
            Err(error) => Self::Request(error),
        }
    }

    /// Whether the consistent list must restart from the first chunk (`410`).
    pub fn is_expired(&self) -> bool {
        matches!(self, Self::Expired(_))
    }
}

impl fmt::Display for ListError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Request(error) => error.fmt(f),
            Self::Api(error) => error.fmt(f),
            Self::Expired(error) => write!(f, "list snapshot expired: {error}"),
            Self::InconsistentSnapshot => {
                f.write_str("a continued list chunk reported a different collection snapshot")
            }
            Self::MissingSnapshot => {
                f.write_str("a list chunk returned a continue token without a resourceVersion")
            }
            Self::ContinueDidNotAdvance => f.write_str(
                "a continued list chunk returned the continue token it was requested with",
            ),
        }
    }
}

impl std::error::Error for ListError {}

/// Fetches the chunks of one list operation: the request factory for
/// `concord_pagination::sequential`.
///
/// ```no_run
/// # async fn run(ns: concord_example_kubernetes_sdk::NamespaceContext)
/// #     -> Result<(), Box<dyn std::error::Error>> {
/// use std::num::NonZeroU32;
/// use concord_example_kubernetes_sdk::list::ListPager;
///
/// let pager = ListPager::new(ns.core().pods(), NonZeroU32::new(100).unwrap());
/// let first = pager.fetch(None).await?;          // GET .../pods?limit=100
/// if let Some(next) = first.next.clone() {
///     let second = pager.fetch(Some(next)).await?; // GET .../pods?limit=100&continue=...
/// #   let _ = second;
/// }
/// # Ok(()) }
/// ```
#[derive(Clone, Debug)]
pub struct ListPager<O> {
    template: O,
    limit: NonZeroU32,
}

impl<O: ListOperation> ListPager<O> {
    /// Chunks of `limit` items of the list `template` (with its selectors and, for the first
    /// chunk, its `resourceVersion` parameters).
    pub fn new(template: O, limit: NonZeroU32) -> Self {
        Self { template, limit }
    }

    /// The chunk size.
    pub fn limit(&self) -> NonZeroU32 {
        self.limit
    }

    /// One chunk: the first (`None`) or the one `key` continues. The returned future owns its
    /// request (`'static`), sends it once and checks the snapshot of a continued chunk.
    pub fn fetch(
        &self,
        key: Option<Continuation>,
    ) -> impl Future<Output = Result<ListPage<O::Item>, ListError>> + Send + 'static {
        let mut op = self.template.clone().with_limit(Some(self.limit.get()));
        if let Some(key) = &key {
            op = op
                .with_continue(Some(key.token.clone()))
                .with_resource_version(None, None);
        }
        async move {
            let list = match op.send_list().await {
                Ok(list) => list,
                Err(error) => return Err(ListError::from_concord(error).await),
            };
            into_page(list, key.as_ref())
        }
    }
}

/// Moves the items out of a list envelope and derives the next key, checking a continued chunk
/// against the key it was requested with (`requested`, `None` for the first chunk): it must
/// report the same snapshot and must not return the same token.
///
/// # Errors
///
/// [`ListError::InconsistentSnapshot`] when `requested` is given and the chunk reports another
/// (or no) snapshot; [`ListError::ContinueDidNotAdvance`] when the chunk's token is the requested
/// one; [`ListError::MissingSnapshot`] when a token comes without a snapshot.
pub fn into_page<T>(
    list: ObjectList<T>,
    requested: Option<&Continuation>,
) -> Result<ListPage<T>, ListError> {
    let (meta, items) = list.into_parts();
    let snapshot = meta.snapshot().cloned();
    if let Some(requested) = requested
        && snapshot.as_ref() != Some(&requested.snapshot)
    {
        return Err(ListError::InconsistentSnapshot);
    }
    let next = match meta.next_token() {
        None => None,
        Some(token) if requested.is_some_and(|r| r.token == *token) => {
            return Err(ListError::ContinueDidNotAdvance);
        }
        Some(token) => Some(Continuation {
            token: token.clone(),
            snapshot: snapshot.clone().ok_or(ListError::MissingSnapshot)?,
        }),
    };
    Ok(Page {
        items,
        next,
        meta: ListPageMeta {
            snapshot,
            remaining_item_count: meta.remaining_item_count,
        },
    })
}

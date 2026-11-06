//! Watches: one watch connection as a stream of typed events, read with Concord's
//! consecutive-JSON-values reader (`concord::jsonl::JsonValues`).
//!
//! # The provider contract (Kubernetes API concepts, "Efficient detection of changes")
//!
//! - `GET <collection>?watch=1&resourceVersion=<rv>&allowWatchBookmarks=true` answers `200` with a
//!   chunked `application/json` body: a series of JSON documents `{"type": .., "object": ..}`
//!   (the server writes one per line, but nothing in the contract makes it a strict JSON Lines
//!   media type, so the decoder frames by JSON value, not by line).
//! - `type` is `ADDED`, `MODIFIED`, `DELETED` (the object as it was before deletion), `BOOKMARK`
//!   (an object of the watched kind carrying only `metadata.resourceVersion`: every change up to
//!   it was sent) or `ERROR` (a `Status`, typically `410` / reason `Expired` when the requested
//!   resource version is older than the server's history).
//! - Starting at a specific `resourceVersion` sends only changes **after** it (no synthetic
//!   `ADDED` events): the caller must already hold that state, normally from a list.
//! - A watch ends normally when the server closes it (`timeoutSeconds`, server shutdown); the
//!   client may start a new watch from the last `resourceVersion` it saw. A `410` (as the
//!   response status or as an `ERROR` event) means that history is gone: clear the cache, list
//!   again and watch from the new list's `resourceVersion`.
//!
//! This module covers **one** connection. Reconnecting, relisting and the current-state map are
//! the caller's workflow (the decoder never reopens a watch, and neither does [`WatchStream`]).
//!
//! # Opening a watch
//!
//! The six watch endpoints are declared `-> stream WatchEvent<T>` with `response JsonStream;`, so
//! the generated `send()` sends the request once and returns the open [`WatchStream`] as soon as
//! the response head is accepted, **without reading the body**:
//!
//! ```no_run
//! # use concord_example_kubernetes_sdk::{KubeContext, ResourceVersion};
//! # use concord_example_kubernetes_sdk::watch::WatchEvent;
//! # type Failure = Box<dyn std::error::Error>;
//! # async fn run(kube: KubeContext, rv: ResourceVersion) -> Result<(), Failure> {
//! let mut events = kube.core().watch_pods().set_resource_version(rv).send().await?;
//! while let Some(event) = events.next().await {
//!     match event? {
//!         WatchEvent::Added(pod) | WatchEvent::Modified(pod) => { /* upsert */ let _ = pod; }
//!         WatchEvent::Deleted(pod) => { /* remove */ let _ = pod; }
//!         WatchEvent::Bookmark(_) => {}
//!         WatchEvent::Error(status) if status.is_expired() => { /* relist */ break; }
//!         WatchEvent::Error(_) => break,
//!     }
//! }
//! # Ok(()) }
//! ```
//!
//! `send()` fails with Concord's error: a non-2xx answer is `ErrorKind::Status` holding the
//! unread response, which [`KubeError::from_concord`](crate::KubeError::from_concord) reads into
//! [`ApiError`] (`410`: [`ApiError::is_expired`]); a 2xx head the reader refuses (not JSON) is
//! `ErrorKind::Metadata`, the response kept unread (`concord::Error::into_response`). Every read
//! error of the stream names the declared endpoint (`ReadError::operation`), and
//! `completion()` tells the true end of the body from a failure or an early stop.
//!
//! **Why `JsonStream`'s defaults.** Kubernetes serves a JSON watch as `application/json` (the
//! OpenAPI response map also lists `application/json;stream=watch`; the standard JSON media policy
//! compares the media type's essence, so both are accepted). A watch has no natural length, so no
//! total is set. An event carries one stored object (etcd's default request limit, which bounds a
//! stored object, is 1.5 MiB), so the default 16 MiB per-value bound leaves headroom for its JSON
//! form (an inference from etcd's default, not checked against a real server here). The SDK
//! therefore has no provider limit of its own to impose. An application that wants a tighter
//! bound per event (its own memory policy) passes its configured `JsonValuesReader` to [`open`],
//! which sends the **same declared operation** with `Prepared::receive` and opens it with that
//! reader; both routes return a [`WatchStream`] whose errors name the endpoint from the
//! declaration.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::fmt;

use concord::jsonl::{JsonValues, JsonValuesReader};
use concord::reqwest;
use concord::stream::{OpenError, OpenErrorKind};

use crate::error::ApiError;
use crate::ids::ResourceVersion;
use crate::model::{Resource, Status};
use crate::selector::{FieldSelector, LabelSelector};
use crate::{
    KubeAppsV1NamespacedWatchDeployments, KubeAppsV1WatchDeployments,
    KubeCoreV1NamespacedWatchPods, KubeCoreV1WatchPods, KubeNetworkingV1NamespacedWatchIngresses,
    KubeNetworkingV1WatchIngresses, WatchOptions,
};

/// One watch event, decoded by its `type` into the watched object, a bookmark or a `Status`.
///
/// The JSON shape is `{"type": "ADDED", "object": {...}}` (serde's adjacently tagged
/// representation; the members may come in either order). An unknown `type` is a decode error.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(
    tag = "type",
    content = "object",
    bound(deserialize = "T: serde::de::DeserializeOwned")
)]
pub enum WatchEvent<T> {
    /// `ADDED`: the new object.
    #[serde(rename = "ADDED")]
    Added(T),
    /// `MODIFIED`: the new state of the object.
    #[serde(rename = "MODIFIED")]
    Modified(T),
    /// `DELETED`: the object's state immediately before deletion.
    #[serde(rename = "DELETED")]
    Deleted(T),
    /// `BOOKMARK`: progress up to a resource version, without an object change.
    #[serde(rename = "BOOKMARK")]
    Bookmark(Bookmark),
    /// `ERROR`: a `Status` (the watch is over after it).
    #[serde(rename = "ERROR")]
    Error(Status),
}

impl<T: Resource> WatchEvent<T> {
    /// The resource version this event moves the watch to: the object's (added, modified,
    /// deleted) or the bookmark's. `None` for an error, or an object without one.
    pub fn resource_version(&self) -> Option<&ResourceVersion> {
        match self {
            Self::Added(o) | Self::Modified(o) | Self::Deleted(o) => {
                o.metadata().resource_version.as_ref()
            }
            Self::Bookmark(b) => Some(&b.metadata.resource_version),
            Self::Error(_) => None,
        }
    }

    /// The event's `type` as Kubernetes spells it.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Added(_) => "ADDED",
            Self::Modified(_) => "MODIFIED",
            Self::Deleted(_) => "DELETED",
            Self::Bookmark(_) => "BOOKMARK",
            Self::Error(_) => "ERROR",
        }
    }
}

/// The object of a `BOOKMARK` event: only `metadata.resourceVersion` (and, ending the initial
/// events of a streaming list, the `k8s.io/initial-events-end` annotation).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Bookmark {
    /// `metadata`.
    pub metadata: BookmarkMeta,
}

/// A bookmark's metadata.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BookmarkMeta {
    /// `resourceVersion`: every change up to it was sent.
    pub resource_version: ResourceVersion,
    /// `annotations`.
    #[serde(default)]
    pub annotations: BTreeMap<String, String>,
}

impl Bookmark {
    /// Whether this bookmark ends the initial events of a `sendInitialEvents=true` watch.
    pub fn is_initial_events_end(&self) -> bool {
        self.metadata
            .annotations
            .get("k8s.io/initial-events-end")
            .is_some_and(|v| v == "true")
    }
}

/// A generated watch operation (the six `GET .../{plural}?watch=1` endpoints), seen through the
/// parameters every watch shares: the declaration's parameter group [`WatchOptions`], whose
/// generated setters each watch endpoint implements; the `with_*` methods are written once here
/// over them. The operation's identity (its endpoint name in errors) comes from the declaration.
pub trait WatchOperation: WatchOptions + Clone + Send + Sync + 'static {
    /// The watched object type.
    type Item: Resource;

    /// `labelSelector`.
    fn with_label_selector(self, selector: Option<LabelSelector>) -> Self {
        self.maybe_label_selector(selector)
    }
    /// `fieldSelector`.
    fn with_field_selector(self, selector: Option<FieldSelector>) -> Self {
        self.maybe_field_selector(selector)
    }
    /// `resourceVersion`: the watch starts after it.
    fn with_resource_version(self, version: Option<ResourceVersion>) -> Self {
        self.maybe_resource_version(version)
    }
    /// `allowWatchBookmarks` (`None`: absent; the declaration's initial value is `true`).
    fn with_bookmarks(self, allow: Option<bool>) -> Self {
        self.maybe_allow_watch_bookmarks(allow)
    }
    /// `timeoutSeconds`: the server ends the watch after this long.
    fn with_timeout_seconds(self, seconds: Option<u32>) -> Self {
        self.maybe_timeout_seconds(seconds)
    }
    /// The generated `prepare()`: the request with its declared stream handling (`send()` on
    /// the result returns the [`WatchStream`]).
    fn prepare_watch(self) -> Result<concord::Prepared<WatchStream<Self::Item>>, concord::Error>;
}

macro_rules! watch_operation {
    ($($op:ty => $item:ty),* $(,)?) => {$(
        impl WatchOperation for $op {
            type Item = $item;
            fn prepare_watch(
                self,
            ) -> Result<concord::Prepared<WatchStream<Self::Item>>, concord::Error> {
                self.prepare()
            }
        }
    )*};
}

watch_operation! {
    KubeCoreV1WatchPods => crate::model::Pod,
    KubeCoreV1NamespacedWatchPods => crate::model::Pod,
    KubeAppsV1WatchDeployments => crate::model::Deployment,
    KubeAppsV1NamespacedWatchDeployments => crate::model::Deployment,
    KubeNetworkingV1WatchIngresses => crate::model::Ingress,
    KubeNetworkingV1NamespacedWatchIngresses => crate::model::Ingress,
}

/// Why [`open`] could not open a watch: the complete classification (no response, non-2xx,
/// refused head), so callers can match it exhaustively. (The generated `send()` reports the same
/// cases as Concord's `Transport` / `Status` / `Metadata` errors.)
#[derive(Debug)]
pub enum WatchOpenError {
    /// The request was not built, or no response arrived (Concord's error, redacted).
    Request(concord::Error),
    /// The server answered non-2xx; `410` ([`ApiError::is_expired`]) means the start resource
    /// version is older than the server's history: list again.
    Api(ApiError),
    /// The 2xx response's head was refused by the decoder's media policy (kept unread).
    Refused(OpenError),
}

impl fmt::Display for WatchOpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Request(error) => error.fmt(f),
            Self::Api(error) => error.fmt(f),
            Self::Refused(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for WatchOpenError {}

/// One open watch connection: typed events decoded incrementally from the response body. It is
/// what a watch endpoint's generated `send()` returns, and what [`open`] returns.
///
/// It **is** Concord's consecutive-JSON-values reader of [`WatchEvent<T>`] (the output type the
/// declaration `-> stream WatchEvent<T>` with `response JsonStream;` generates), so it has the
/// reader's whole contract: `next()` (cancel-safe; `None` at the true end of the body and after a
/// terminal error), `position()`, `peak_retained_bytes()`, `completion()` (only `EndOfInput`
/// proves the server ended the stream; stopping early or dropping it is not an end of input) and
/// `into_stream()` (an owning `futures_core::Stream`). A `ReadError` of kind `Transport` is a lost
/// connection, `Limit` an event larger than the reader's `max_value_bytes`, `Format`/`Decode` a
/// malformed event; each names the declared endpoint.
///
/// Nothing runs ahead of the caller; at most the current chunk and one unfinished event are
/// retained. Dropping it drops the response (the connection is closed, not returned to the pool).
/// The same reader over an in-memory source (`JsonValuesReader::read`) is a
/// `WatchStream<T, concord::stream::Chunks>`.
pub type WatchStream<T, S = reqwest::Response> = JsonValues<WatchEvent<T>, S>;

/// Opens one watch with a caller-configured `reader` (a per-event bound of the application's
/// choosing): prepares the generated operation, sends it once with `Prepared::receive`, and opens
/// the declared response with `reader` (a 2xx status and the reader's media policy are checked
/// before the body is polled). A non-2xx answer's `Status` document is read (bounded) into
/// [`WatchOpenError::Api`]. The endpoint's identity comes from the declaration: the stream's read
/// errors and the [`ApiError`] name it.
///
/// With the reader defaults, the generated `send()` of the operation is the shorter route.
///
/// # Errors
///
/// [`WatchOpenError`]: not built / no response, non-2xx, refused head (kept unread).
pub async fn open<W: WatchOperation>(
    operation: W,
    reader: &JsonValuesReader,
) -> Result<WatchStream<W::Item>, WatchOpenError> {
    let prepared = operation.prepare_watch().map_err(WatchOpenError::Request)?;
    let received = prepared.receive().await.map_err(WatchOpenError::Request)?;
    match reader.open::<WatchEvent<W::Item>>(received) {
        Ok(events) => Ok(events),
        Err(refused) if refused.kind() == OpenErrorKind::Status => {
            let endpoint = refused.operation();
            Err(WatchOpenError::Api(
                ApiError::from_response(endpoint, refused.into_response()).await,
            ))
        }
        Err(refused) => Err(WatchOpenError::Refused(refused)),
    }
}

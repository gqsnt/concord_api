//! The frozen initial consumer (stage 1): a downstream "release readiness" audit of one
//! application across namespaces, written against the Kubernetes SDK's public exports only.
//!
//! - [`audit`] reads, in each namespace, the Deployments and Services that carry the application's
//!   label ([`app_selector`]: `app.kubernetes.io/part-of=<app>`), each collection as one
//!   consistent chunked list ([`collect`]: the SDK's `list::ListPager` driven by
//!   `concord_pagination::sequential`), and reports Deployments with fewer available replicas than
//!   desired.
//! - [`follow`] opens one watch from an audited snapshot (`watch::open` over
//!   `concord::jsonl::JsonValuesReader`) and hands each typed event to a sink as it arrives;
//!   [`follow_deployments`] is the namespace-scoped Deployment watch the audit uses.
//!
//! This file is not edited after stage 1: the updated consumer lives in `after.rs`.

use std::num::{NonZeroU32, NonZeroUsize};
use std::ops::ControlFlow;

use concord::jsonl::JsonValuesReader;
use concord::stream::ReadError;
use concord_example_kubernetes_sdk::list::{ListError, ListOperation, ListPager};
use concord_example_kubernetes_sdk::model::{Deployment, Service};
use concord_example_kubernetes_sdk::watch::{self, WatchEvent, WatchOpenError, WatchOperation};
use concord_example_kubernetes_sdk::{
    KubeContext, LabelSelector, NamespaceContext, NamespaceName, ObjectName, ResourceVersion,
    SelectorError,
};
use concord_pagination::{Budget, Outcome, Resume, sequential};

/// The recommended label naming the application an object belongs to.
pub const PART_OF: &str = "app.kubernetes.io/part-of";

/// The audit's label selector: `app.kubernetes.io/part-of=<app>`.
///
/// # Errors
///
/// [`SelectorError`] when `app` is not a valid label value.
pub fn app_selector(app: &str) -> Result<LabelSelector, SelectorError> {
    LabelSelector::equals(PART_OF, app)
}

/// Chunking of every collection read.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// `limit` per chunk.
    pub chunk_size: NonZeroU32,
    /// The most chunks one collection may take.
    pub max_chunks: NonZeroUsize,
}

/// One collection, read completely at one snapshot.
#[derive(Debug)]
pub struct Listed<T> {
    /// The typed objects, in server order.
    pub items: Vec<T>,
    /// The collection `resourceVersion` every chunk reported (where a watch can start).
    pub snapshot: ResourceVersion,
    /// Chunks received.
    pub chunks: u64,
}

/// Why a collection could not be read completely.
#[derive(Debug)]
pub enum CollectError {
    /// A chunk failed; the SDK's typed error is kept (`ListError::Expired` for a `410`: re-run).
    List(ListError),
    /// The chunk budget ended while the server had more chunks.
    Bounded {
        /// Chunks received before the budget ended.
        chunks: u64,
    },
    /// The server sent no collection `resourceVersion`.
    NoSnapshot,
}

/// Reads the whole collection `template` lists (with its selectors) in consistent chunks.
///
/// # Errors
///
/// [`CollectError`]: a failed chunk, the chunk budget, or a list without a snapshot.
pub async fn collect<O: ListOperation>(
    template: O,
    limits: Limits,
) -> Result<Listed<O::Item>, CollectError> {
    let pager = ListPager::new(template, limits.chunk_size);
    let mut chain = sequential(Resume::Start, Budget::new(limits.max_chunks), |key| {
        pager.fetch(key)
    });
    let mut items = Vec::new();
    let mut snapshot = None;
    while let Some(page) = chain.next().await {
        let page = page.map_err(CollectError::List)?;
        snapshot = page.meta.snapshot.or(snapshot);
        items.extend(page.items);
    }
    let report = chain.finish();
    let chunks = report.progress.emitted;
    match report.outcome {
        Outcome::Exhausted => Ok(Listed {
            items,
            snapshot: snapshot.ok_or(CollectError::NoSnapshot)?,
            chunks,
        }),
        _ => Err(CollectError::Bounded { chunks }),
    }
}

/// What the audit reads.
#[derive(Clone, Debug)]
pub struct AuditRequest {
    /// The application's label value (`app.kubernetes.io/part-of`).
    pub app: String,
    /// The namespaces, in order.
    pub namespaces: Vec<NamespaceName>,
    /// Chunking of every collection.
    pub limits: Limits,
}

/// One namespace of the audit.
#[derive(Debug)]
pub struct NamespaceAudit {
    /// The namespace.
    pub namespace: NamespaceName,
    /// The application's Deployments.
    pub deployments: Listed<Deployment>,
    /// The application's Services.
    pub services: Listed<Service>,
}

impl NamespaceAudit {
    /// Deployments with fewer available replicas than desired (or a stale status), by name.
    pub fn unavailable(&self) -> Vec<&ObjectName> {
        self.deployments
            .items
            .iter()
            .filter(|d| !d.is_available())
            .filter_map(|d| d.metadata.name.as_ref())
            .collect()
    }
}

/// The audit's result.
#[derive(Debug)]
pub struct Audit {
    /// The selector every collection was read with.
    pub selector: LabelSelector,
    /// One entry per namespace, in request order.
    pub namespaces: Vec<NamespaceAudit>,
}

impl Audit {
    /// Whether every Deployment of the application is available.
    pub fn ready(&self) -> bool {
        self.namespaces.iter().all(|n| n.unavailable().is_empty())
    }
}

/// Why the audit stopped.
#[derive(Debug)]
pub enum AuditError {
    /// The application name is not a valid label value.
    Selector(SelectorError),
    /// A collection could not be read.
    Collect {
        /// Where.
        namespace: NamespaceName,
        /// Which family (`deployments`, `services`).
        family: &'static str,
        /// Why.
        error: CollectError,
    },
}

/// Runs the audit: Deployments and Services of `request.app` in each namespace.
///
/// # Errors
///
/// [`AuditError`] for an invalid application name or the first collection that fails.
pub async fn audit(kube: &KubeContext, request: &AuditRequest) -> Result<Audit, AuditError> {
    let selector = app_selector(&request.app).map_err(AuditError::Selector)?;
    let mut namespaces = Vec::new();
    for namespace in &request.namespaces {
        let scope = kube.namespace(namespace.clone());
        let failed = |family| {
            let namespace = namespace.clone();
            move |error| AuditError::Collect {
                namespace,
                family,
                error,
            }
        };
        let deployments = collect(
            scope
                .apps()
                .deployments()
                .set_label_selector(selector.clone()),
            request.limits,
        )
        .await
        .map_err(failed("deployments"))?;
        let services = collect(
            scope.core().services().set_label_selector(selector.clone()),
            request.limits,
        )
        .await
        .map_err(failed("services"))?;
        namespaces.push(NamespaceAudit {
            namespace: namespace.clone(),
            deployments,
            services,
        });
    }
    Ok(Audit {
        selector,
        namespaces,
    })
}

/// Bounds of one watch connection.
#[derive(Clone, Copy, Debug)]
pub struct FollowLimits {
    /// `timeoutSeconds`: the server ends the watch after this long.
    pub server_timeout_seconds: u32,
    /// The most object and bookmark events to deliver.
    pub max_events: u64,
    /// The largest event, in bytes.
    pub max_event_bytes: usize,
}

/// How the watch connection ended.
#[derive(Debug)]
pub enum FollowEnd {
    /// The server ended the stream (`timeoutSeconds`, shutdown): watch again from
    /// [`FollowReport::last`].
    ServerClosed,
    /// An `ERROR` event with a `410`/`Expired` status: re-run the audit, then watch again.
    Expired,
    /// Another `ERROR` event, with its `Status.code`.
    ErrorEvent(Option<u16>),
    /// Reading failed (lost connection, oversized or malformed event).
    Read(ReadError),
    /// The sink asked to stop.
    Stopped,
    /// [`FollowLimits::max_events`] events were delivered.
    Budget,
}

/// One watch connection's result.
#[derive(Debug)]
pub struct FollowReport {
    /// Object events (`ADDED`, `MODIFIED`, `DELETED`) delivered.
    pub changes: u64,
    /// `BOOKMARK` events delivered.
    pub bookmarks: u64,
    /// The last resource version seen (the start if no event carried one).
    pub last: ResourceVersion,
    /// How it ended.
    pub end: FollowEnd,
}

/// Watches the collection `operation` names (with `selector`) after `from`, delivering each
/// object or bookmark event to `sink` as it is decoded.
///
/// # Errors
///
/// [`WatchOpenError`] when the watch could not be opened (no response, non-2xx, refused head).
pub async fn follow<W, K>(
    operation: W,
    selector: &LabelSelector,
    from: ResourceVersion,
    limits: FollowLimits,
    mut sink: K,
) -> Result<FollowReport, WatchOpenError>
where
    W: WatchOperation,
    K: FnMut(&WatchEvent<W::Item>) -> ControlFlow<()>,
{
    let request = operation
        .with_label_selector(Some(selector.clone()))
        .with_resource_version(Some(from.clone()))
        .with_timeout_seconds(Some(limits.server_timeout_seconds));
    let reader = JsonValuesReader::new().max_value_bytes(limits.max_event_bytes);
    let mut stream = watch::open(request, &reader).await?;
    let mut report = FollowReport {
        changes: 0,
        bookmarks: 0,
        last: from,
        end: FollowEnd::ServerClosed,
    };
    loop {
        if report.changes + report.bookmarks >= limits.max_events {
            report.end = FollowEnd::Budget;
            break;
        }
        let event = match stream.next().await {
            None => break,
            Some(Err(error)) => {
                report.end = FollowEnd::Read(error);
                break;
            }
            Some(Ok(event)) => event,
        };
        if let WatchEvent::Error(status) = &event {
            report.end = if status.is_expired() {
                FollowEnd::Expired
            } else {
                FollowEnd::ErrorEvent(status.code)
            };
            break;
        }
        if let Some(rv) = event.resource_version() {
            report.last = rv.clone();
        }
        match event {
            WatchEvent::Bookmark(_) => report.bookmarks += 1,
            _ => report.changes += 1,
        }
        if sink(&event).is_break() {
            report.end = FollowEnd::Stopped;
            break;
        }
    }
    Ok(report)
}

/// Follows the application's Deployments in one namespace after an audited snapshot.
///
/// # Errors
///
/// [`WatchOpenError`], as [`follow`].
pub async fn follow_deployments<K>(
    scope: &NamespaceContext,
    selector: &LabelSelector,
    from: ResourceVersion,
    limits: FollowLimits,
    sink: K,
) -> Result<FollowReport, WatchOpenError>
where
    K: FnMut(&WatchEvent<Deployment>) -> ControlFlow<()>,
{
    follow(
        scope.apps().watch_deployments(),
        selector,
        from,
        limits,
        sink,
    )
    .await
}

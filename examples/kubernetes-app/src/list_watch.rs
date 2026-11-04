//! KU-B — Kubernetes list/watch/resync.
//!
//! **Input:** a configured [`KubeContext`], a [`ResourceSelector`] (namespace or cluster-wide,
//! label and field selectors), a [`WatchPolicy`] (bounds) and a stop signal (any
//! `Future<Output = ()>`: the caller's deadline or cancellation). **Output:** typed change events
//! delivered to the caller's sink as they are applied, and a [`WatchReport`]: the current-state
//! map, the last resource version, every reset boundary, every watch connection and how it ended,
//! and the outcome.
//!
//! The workflow (application policy, stated here; the SDK's [`watch`] module covers one
//! connection and never reconnects, and the byte decoder never reopens anything):
//!
//! 1. **List** the collection in consistent chunks ([`collect_list`]); its items become the state
//!    and its collection `resourceVersion` the watch start. This is a [`Change::Reset`] boundary
//!    ([`ResetCause::Initial`]).
//! 2. **Watch** from that resource version (`allowWatchBookmarks` per policy, `timeoutSeconds`
//!    per policy). `ADDED`/`MODIFIED` insert the object under its namespace/name, `DELETED`
//!    removes it, `BOOKMARK` only moves the resource version. Each applied event is passed to the
//!    sink.
//! 3. **Reconnect** from the last resource version seen when the server ends the stream normally,
//!    the transport fails mid-stream, the stream carries a malformed or oversized event, a
//!    non-`410` `ERROR` event arrives, or opening fails without a response. Each reconnect counts
//!    against [`WatchPolicy::max_reconnects`]. Events already applied stay applied; starting at a
//!    specific resource version sends only later changes, so nothing is applied twice.
//! 4. **Relist** when the history is gone: `410` answering the watch request, or an `ERROR` event
//!    whose `Status` is `410`/`Expired`. The state is replaced by a fresh list at a new snapshot;
//!    the [`ResetBoundary`] records how many objects the old state had, how many the new snapshot
//!    has and how many keys disappeared while the history was unavailable. Relists count against
//!    [`WatchPolicy::max_relists`].
//! 5. **Stop** when the stop signal completes (also while awaiting data on a stalled stream), the
//!    sink returns `ControlFlow::Break`, the event budget is used, a budget of reconnects/relists
//!    is exhausted, or an unrecoverable failure occurs (a non-`410` status, a list that cannot
//!    complete). Dropping the returned future instead drops the open response at once.

use std::collections::BTreeMap;
use std::future::Future;
use std::ops::ControlFlow;

use concord::jsonl::JsonValuesReader;
use concord::stream::{OpenError, ReadErrorKind};
use concord_example_kubernetes_sdk::list::ListOperation;
use concord_example_kubernetes_sdk::model::{Deployment, Pod};
use concord_example_kubernetes_sdk::watch::{self, WatchEvent, WatchOpenError, WatchOperation};
use concord_example_kubernetes_sdk::{
    ApiError, FieldSelector, KubeContext, LabelSelector, NamespaceName, ObjectKey, Resource,
    ResourceVersion,
};
use concord_pagination::Resume;

use crate::collect::{ListOutcome, ListPolicy, collect_list};
use crate::stop::Stop;

/// Which objects to list and watch.
#[derive(Clone, Debug, Default)]
pub struct ResourceSelector {
    /// The namespace; `None` watches every namespace.
    pub namespace: Option<NamespaceName>,
    /// `labelSelector` (list and watch).
    pub labels: Option<LabelSelector>,
    /// `fieldSelector` (list and watch).
    pub fields: Option<FieldSelector>,
}

/// The bounds of one list/watch run.
#[derive(Clone, Copy, Debug)]
pub struct WatchPolicy {
    /// Chunking and expiry policy of the initial list and of every relist.
    pub list: ListPolicy,
    /// `timeoutSeconds` of each watch request: the server ends the stream after it (a normal end,
    /// followed by a reconnect). `None`: the server's default.
    pub server_timeout_seconds: Option<u32>,
    /// `allowWatchBookmarks`.
    pub bookmarks: bool,
    /// The largest watch event, in bytes (the decoder's per-value limit); a larger one ends the
    /// connection as [`ConnectionEnd::Oversized`].
    pub max_event_bytes: usize,
    /// The most watch events (`ADDED`, `MODIFIED`, `DELETED`, `BOOKMARK`) to apply over the
    /// whole run.
    pub max_events: u64,
    /// The most reconnects over the whole run.
    pub max_reconnects: u32,
    /// The most relists after `410` over the whole run.
    pub max_relists: u32,
}

/// The type of an applied object change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeKind {
    /// `ADDED`.
    Added,
    /// `MODIFIED`.
    Modified,
    /// `DELETED`.
    Deleted,
}

/// Why the state was replaced by a list snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResetCause {
    /// The initial list.
    Initial,
    /// The watch request was answered `410 Gone`.
    ExpiredOnOpen,
    /// An `ERROR` event with a `410`/`Expired` status.
    ExpiredEvent,
}

/// An explicit reset boundary: before it, the state followed the previous snapshot and its
/// events; after it, the state is exactly the new list snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResetBoundary {
    /// Why.
    pub cause: ResetCause,
    /// The snapshot (`resourceVersion`) the state now reflects; the next watch starts there.
    pub snapshot: ResourceVersion,
    /// The resource version the abandoned watch had reached (`None` for the initial list).
    pub abandoned_at: Option<ResourceVersion>,
    /// Objects in the state before the reset.
    pub previous_objects: usize,
    /// Objects in the new snapshot.
    pub objects: usize,
    /// Keys of the previous state absent from the new snapshot (deleted while the history was
    /// unavailable; no `DELETED` event was seen for them).
    pub vanished: usize,
    /// Restarts of the list itself (expired continue tokens) before this snapshot was complete.
    pub list_restarts: usize,
}

/// One typed change, as delivered to the sink.
#[derive(Debug)]
pub enum Change<'a, T> {
    /// The state was replaced by a list snapshot.
    Reset(&'a ResetBoundary),
    /// An object change was applied to the state.
    Applied {
        /// `ADDED`, `MODIFIED` or `DELETED`.
        kind: ChangeKind,
        /// The object's namespace/name.
        key: &'a ObjectKey,
        /// The object (for `DELETED`, its state immediately before deletion).
        object: &'a T,
    },
    /// A `BOOKMARK` moved the resource version.
    Bookmark(&'a ResourceVersion),
    /// The connection ended and a new watch starts from `from`.
    Reconnecting {
        /// The resource version the new watch starts after.
        from: &'a ResourceVersion,
        /// How the previous connection ended.
        after: ConnectionEnd,
    },
}

/// How one watch connection ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionEnd {
    /// The server ended the stream (true end of the body).
    Normal,
    /// Reading the body failed mid-stream (connection lost).
    TransportLost,
    /// An event was not valid JSON, or did not decode as a typed watch event.
    Malformed,
    /// An event exceeded [`WatchPolicy::max_event_bytes`].
    Oversized,
    /// The history is gone (`410` on open or in an `ERROR` event): a relist follows.
    Expired,
    /// A non-`410` `ERROR` event.
    ErrorEvent {
        /// Its `Status.code`.
        code: Option<u16>,
    },
    /// The watch request failed without a response (connection refused, reset before headers).
    OpenFailed,
    /// The stop signal completed while the connection was open.
    Cancelled,
    /// The sink asked to stop.
    ConsumerStopped,
    /// The event budget was used.
    EventBudget,
}

/// One watch connection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionRecord {
    /// The resource version it started after.
    pub from: ResourceVersion,
    /// Events applied from it.
    pub events: u64,
    /// How it ended.
    pub end: ConnectionEnd,
}

/// Counts of applied events over the run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EventCounts {
    /// `ADDED`.
    pub added: u64,
    /// `MODIFIED`.
    pub modified: u64,
    /// `DELETED`.
    pub deleted: u64,
    /// `BOOKMARK`.
    pub bookmarks: u64,
}

impl EventCounts {
    /// All applied events.
    pub fn total(&self) -> u64 {
        self.added + self.modified + self.deleted + self.bookmarks
    }
}

/// An unrecoverable failure.
#[derive(Debug)]
#[non_exhaustive]
pub enum WatchFailure {
    /// The initial list or a relist did not complete (its outcome is kept).
    List(ListOutcome),
    /// The watch request was answered with a non-2xx status other than `410`.
    Api(ApiError),
    /// The watch request could not be built, or failed in a non-transport way.
    Request(concord::Error),
    /// The watch response's head was refused by the decoder (its media type).
    Refused(OpenError),
}

/// How the run ended.
#[derive(Debug)]
#[non_exhaustive]
pub enum WatchOutcome {
    /// The stop signal completed.
    Cancelled,
    /// The sink returned `ControlFlow::Break`.
    ConsumerStopped,
    /// [`WatchPolicy::max_events`] events were applied.
    EventBudget,
    /// A reconnect was needed beyond [`WatchPolicy::max_reconnects`].
    ReconnectBudget,
    /// A relist was needed beyond [`WatchPolicy::max_relists`].
    RelistBudget,
    /// An unrecoverable failure.
    Failed(WatchFailure),
}

/// The KU-B result: the current state and every boundary.
#[derive(Debug)]
pub struct WatchReport<T> {
    /// How it ended.
    pub outcome: WatchOutcome,
    /// The current state: the last snapshot plus the events applied after it.
    pub state: BTreeMap<ObjectKey, T>,
    /// The last resource version seen (snapshot, object or bookmark).
    pub resource_version: Option<ResourceVersion>,
    /// Every reset boundary, the initial list first.
    pub resets: Vec<ResetBoundary>,
    /// Every watch connection, in order.
    pub connections: Vec<ConnectionRecord>,
    /// Applied events.
    pub counts: EventCounts,
}

/// Runs KU-B for Pods.
pub async fn watch_pods<S, K>(
    kube: &KubeContext,
    selector: &ResourceSelector,
    policy: WatchPolicy,
    sink: K,
    stop: S,
) -> WatchReport<Pod>
where
    S: Future<Output = ()>,
    K: FnMut(Change<'_, Pod>) -> ControlFlow<()>,
{
    match &selector.namespace {
        Some(namespace) => {
            let ns = kube.namespace(namespace.clone());
            list_watch(
                ns.core().pods(),
                ns.core().watch_pods(),
                selector,
                policy,
                sink,
                stop,
            )
            .await
        }
        None => {
            let core = kube.core();
            list_watch(core.pods(), core.watch_pods(), selector, policy, sink, stop).await
        }
    }
}

/// Runs KU-B for Deployments.
pub async fn watch_deployments<S, K>(
    kube: &KubeContext,
    selector: &ResourceSelector,
    policy: WatchPolicy,
    sink: K,
    stop: S,
) -> WatchReport<Deployment>
where
    S: Future<Output = ()>,
    K: FnMut(Change<'_, Deployment>) -> ControlFlow<()>,
{
    match &selector.namespace {
        Some(namespace) => {
            let ns = kube.namespace(namespace.clone());
            let apps = ns.apps();
            list_watch(
                apps.deployments(),
                apps.watch_deployments(),
                selector,
                policy,
                sink,
                stop,
            )
            .await
        }
        None => {
            let apps = kube.apps();
            list_watch(
                apps.deployments(),
                apps.watch_deployments(),
                selector,
                policy,
                sink,
                stop,
            )
            .await
        }
    }
}

/// What one connection's read loop decided.
enum Next {
    Reconnect(ConnectionEnd),
    Relist(ResetCause),
    End(WatchOutcome, ConnectionEnd),
}

struct Run<T, K> {
    state: BTreeMap<ObjectKey, T>,
    resource_version: Option<ResourceVersion>,
    resets: Vec<ResetBoundary>,
    connections: Vec<ConnectionRecord>,
    counts: EventCounts,
    sink: K,
}

impl<T: Resource, K: FnMut(Change<'_, T>) -> ControlFlow<()>> Run<T, K> {
    fn finish(self, outcome: WatchOutcome) -> WatchReport<T> {
        WatchReport {
            outcome,
            state: self.state,
            resource_version: self.resource_version,
            resets: self.resets,
            connections: self.connections,
            counts: self.counts,
        }
    }

    /// Replaces the state with a complete list snapshot.
    fn reset(
        &mut self,
        cause: ResetCause,
        items: Vec<T>,
        snapshot: ResourceVersion,
        list_restarts: usize,
    ) -> ControlFlow<()> {
        let previous = std::mem::take(&mut self.state);
        for item in items {
            if let Some(key) = ObjectKey::of(&item) {
                self.state.insert(key, item);
            }
        }
        let vanished = previous
            .keys()
            .filter(|key| !self.state.contains_key(*key))
            .count();
        let boundary = ResetBoundary {
            cause,
            snapshot: snapshot.clone(),
            abandoned_at: match cause {
                ResetCause::Initial => None,
                _ => self.resource_version.clone(),
            },
            previous_objects: previous.len(),
            objects: self.state.len(),
            vanished,
            list_restarts,
        };
        self.resource_version = Some(snapshot);
        self.resets.push(boundary);
        let boundary = self.resets.last().expect("just pushed");
        (self.sink)(Change::Reset(boundary))
    }

    /// Applies one object change.
    fn apply(&mut self, kind: ChangeKind, object: T) -> ControlFlow<()> {
        if let Some(rv) = object.metadata().resource_version.clone() {
            self.resource_version = Some(rv);
        }
        match kind {
            ChangeKind::Added => self.counts.added += 1,
            ChangeKind::Modified => self.counts.modified += 1,
            ChangeKind::Deleted => self.counts.deleted += 1,
        }
        let Some(key) = ObjectKey::of(&object) else {
            // An object without a name cannot be keyed; count it, apply nothing.
            return ControlFlow::Continue(());
        };
        match kind {
            ChangeKind::Added | ChangeKind::Modified => {
                let entry = self.state.entry(key);
                let key = entry.key().clone();
                let stored = match entry {
                    std::collections::btree_map::Entry::Occupied(mut slot) => {
                        slot.insert(object);
                        slot.into_mut()
                    }
                    std::collections::btree_map::Entry::Vacant(slot) => slot.insert(object),
                };
                (self.sink)(Change::Applied {
                    kind,
                    key: &key,
                    object: stored,
                })
            }
            ChangeKind::Deleted => {
                let removed = self.state.remove(&key);
                let object = removed.as_ref().unwrap_or(&object);
                (self.sink)(Change::Applied {
                    kind,
                    key: &key,
                    object,
                })
            }
        }
    }
}

/// The generic list/watch/resync loop over one list operation and its watch operation.
pub async fn list_watch<L, W, S, K>(
    list: L,
    watch_op: W,
    selector: &ResourceSelector,
    policy: WatchPolicy,
    sink: K,
    stop: S,
) -> WatchReport<L::Item>
where
    L: ListOperation,
    W: WatchOperation<Item = L::Item>,
    S: Future<Output = ()>,
    K: FnMut(Change<'_, L::Item>) -> ControlFlow<()>,
{
    let list = list
        .with_label_selector(selector.labels.clone())
        .with_field_selector(selector.fields.clone());
    let watch_op = watch_op
        .with_label_selector(selector.labels.clone())
        .with_field_selector(selector.fields.clone())
        .with_bookmarks(Some(policy.bookmarks))
        .with_timeout_seconds(policy.server_timeout_seconds);
    let reader = JsonValuesReader::new().max_value_bytes(policy.max_event_bytes);
    let mut stop = Stop::new(stop);
    let mut run = Run {
        state: BTreeMap::new(),
        resource_version: None,
        resets: Vec::new(),
        connections: Vec::new(),
        counts: EventCounts::default(),
        sink,
    };
    let mut reconnects = 0u32;
    let mut relists = 0u32;
    let mut pending_reset = Some(ResetCause::Initial);

    loop {
        // 1/4. List (initially, or after the history expired).
        if let Some(cause) = pending_reset.take() {
            if cause != ResetCause::Initial {
                if relists >= policy.max_relists {
                    return run.finish(WatchOutcome::RelistBudget);
                }
                relists += 1;
            }
            let Some(collected) = stop
                .race(collect_list(list.clone(), policy.list, Resume::Start))
                .await
            else {
                return run.finish(WatchOutcome::Cancelled);
            };
            let restarts = collected.completion.restarts.len();
            match (collected.completion.outcome, collected.completion.snapshot) {
                (ListOutcome::Complete, Some(snapshot)) => {
                    if run
                        .reset(cause, collected.items, snapshot, restarts)
                        .is_break()
                    {
                        return run.finish(WatchOutcome::ConsumerStopped);
                    }
                }
                (outcome, _) => {
                    return run.finish(WatchOutcome::Failed(WatchFailure::List(outcome)));
                }
            }
        }

        // 2. Watch from the last resource version.
        let from = run
            .resource_version
            .clone()
            .expect("a snapshot or an event set the resource version");
        let request = watch_op.clone().with_resource_version(Some(from.clone()));
        let opened = match stop.race(watch::open(request, &reader)).await {
            None => return run.finish(WatchOutcome::Cancelled),
            Some(opened) => opened,
        };
        let mut events = 0u64;
        let next = match opened {
            Ok(mut stream) => loop {
                if run.counts.total() >= policy.max_events {
                    break Next::End(WatchOutcome::EventBudget, ConnectionEnd::EventBudget);
                }
                let Some(item) = stop.race(stream.next()).await else {
                    break Next::End(WatchOutcome::Cancelled, ConnectionEnd::Cancelled);
                };
                let flow = match item {
                    None => break Next::Reconnect(ConnectionEnd::Normal),
                    Some(Err(error)) => {
                        break Next::Reconnect(match error.kind() {
                            ReadErrorKind::Transport => ConnectionEnd::TransportLost,
                            ReadErrorKind::Limit => ConnectionEnd::Oversized,
                            _ => ConnectionEnd::Malformed,
                        });
                    }
                    Some(Ok(WatchEvent::Error(status))) => {
                        if status.is_expired() {
                            break Next::Relist(ResetCause::ExpiredEvent);
                        }
                        break Next::Reconnect(ConnectionEnd::ErrorEvent { code: status.code });
                    }
                    Some(Ok(WatchEvent::Bookmark(bookmark))) => {
                        events += 1;
                        run.counts.bookmarks += 1;
                        run.resource_version = Some(bookmark.metadata.resource_version);
                        let rv = run.resource_version.as_ref().expect("just set");
                        (run.sink)(Change::Bookmark(rv))
                    }
                    Some(Ok(WatchEvent::Added(object))) => {
                        events += 1;
                        run.apply(ChangeKind::Added, object)
                    }
                    Some(Ok(WatchEvent::Modified(object))) => {
                        events += 1;
                        run.apply(ChangeKind::Modified, object)
                    }
                    Some(Ok(WatchEvent::Deleted(object))) => {
                        events += 1;
                        run.apply(ChangeKind::Deleted, object)
                    }
                };
                if flow.is_break() {
                    break Next::End(
                        WatchOutcome::ConsumerStopped,
                        ConnectionEnd::ConsumerStopped,
                    );
                }
            },
            Err(WatchOpenError::Api(api)) if api.is_expired() => {
                Next::Relist(ResetCause::ExpiredOnOpen)
            }
            Err(WatchOpenError::Api(api)) => {
                return run.finish(WatchOutcome::Failed(WatchFailure::Api(api)));
            }
            Err(WatchOpenError::Refused(refused)) => {
                return run.finish(WatchOutcome::Failed(WatchFailure::Refused(refused)));
            }
            Err(WatchOpenError::Request(error)) => {
                if error.kind() == concord::ErrorKind::Transport {
                    Next::Reconnect(ConnectionEnd::OpenFailed)
                } else {
                    return run.finish(WatchOutcome::Failed(WatchFailure::Request(error)));
                }
            }
        };

        // 3/4/5. Decide what follows the connection.
        let end = match &next {
            Next::Reconnect(end) | Next::End(_, end) => *end,
            Next::Relist(_) => ConnectionEnd::Expired,
        };
        run.connections.push(ConnectionRecord {
            from: from.clone(),
            events,
            end,
        });
        match next {
            Next::End(outcome, _) => return run.finish(outcome),
            Next::Relist(cause) => pending_reset = Some(cause),
            Next::Reconnect(end) => {
                if reconnects >= policy.max_reconnects {
                    return run.finish(WatchOutcome::ReconnectBudget);
                }
                reconnects += 1;
                let rv = run.resource_version.as_ref().expect("set before watching");
                if (run.sink)(Change::Reconnecting {
                    from: rv,
                    after: end,
                })
                .is_break()
                {
                    return run.finish(WatchOutcome::ConsumerStopped);
                }
            }
        }
    }
}

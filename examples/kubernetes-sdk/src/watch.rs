//! Watches: one watch connection as a stream of typed events, read with Concord's
//! consecutive-JSON-values reader (`concord::jsonl::JsonValuesReader`).
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

use serde::Deserialize;
use std::collections::BTreeMap;

use crate::ids::ResourceVersion;
use crate::model::{Resource, Status};
use crate::selector::{FieldSelector, LabelSelector};
use crate::{
    KubeAppsV1NamespacedWatchDeployments, KubeAppsV1WatchDeployments,
    KubeCoreV1NamespacedWatchPods, KubeCoreV1WatchPods,
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

/// A generated watch operation (the four `GET .../{plural}?watch=1` endpoints), seen through the
/// parameters every watch shares; each method forwards to the generated `maybe_*` setter.
pub trait WatchOperation: Clone + Send + Sync + 'static {
    /// The watched object type.
    type Item: Resource;
    /// The endpoint name, used as the reader's operation label.
    const ENDPOINT: &'static str;

    /// `labelSelector`.
    fn with_label_selector(self, selector: Option<LabelSelector>) -> Self;
    /// `fieldSelector`.
    fn with_field_selector(self, selector: Option<FieldSelector>) -> Self;
    /// `resourceVersion`: the watch starts after it.
    fn with_resource_version(self, version: Option<ResourceVersion>) -> Self;
    /// `allowWatchBookmarks` (`None`: absent; the declaration's initial value is `true`).
    fn with_bookmarks(self, allow: Option<bool>) -> Self;
    /// `timeoutSeconds`: the server ends the watch after this long.
    fn with_timeout_seconds(self, seconds: Option<u32>) -> Self;
    /// The generated `prepare()`.
    fn prepare_watch(self) -> Result<concord::Prepared<()>, concord::Error>;
}

macro_rules! watch_operation {
    ($($op:ty => $item:ty, $endpoint:literal);* $(;)?) => {$(
        impl WatchOperation for $op {
            type Item = $item;
            const ENDPOINT: &'static str = $endpoint;
            fn with_label_selector(self, selector: Option<LabelSelector>) -> Self {
                self.maybe_label_selector(selector)
            }
            fn with_field_selector(self, selector: Option<FieldSelector>) -> Self {
                self.maybe_field_selector(selector)
            }
            fn with_resource_version(self, version: Option<ResourceVersion>) -> Self {
                self.maybe_resource_version(version)
            }
            fn with_bookmarks(self, allow: Option<bool>) -> Self {
                self.maybe_allow_watch_bookmarks(allow)
            }
            fn with_timeout_seconds(self, seconds: Option<u32>) -> Self {
                self.maybe_timeout_seconds(seconds)
            }
            fn prepare_watch(self) -> Result<concord::Prepared<()>, concord::Error> {
                self.prepare()
            }
        }
    )*};
}

watch_operation! {
    KubeCoreV1WatchPods => crate::model::Pod, "Kube.core_v1.watch_pods";
    KubeCoreV1NamespacedWatchPods => crate::model::Pod, "Kube.core_v1.namespaced.watch_pods";
    KubeAppsV1WatchDeployments => crate::model::Deployment, "Kube.apps_v1.watch_deployments";
    KubeAppsV1NamespacedWatchDeployments => crate::model::Deployment,
        "Kube.apps_v1.namespaced.watch_deployments";
}

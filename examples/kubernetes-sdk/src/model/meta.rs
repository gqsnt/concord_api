//! `meta/v1` shapes shared by every resource: object and list metadata, the list envelope, the
//! `Status` error document and server version information.
//!
//! Every struct tolerates unknown fields (serde's default) and treats absent optional fields as
//! absent, so a newer server's additional fields do not break decoding. Labels and annotations
//! stay maps: they are genuinely dynamic metadata.

use std::collections::BTreeMap;
use std::fmt;

use serde::Deserialize;

use crate::ids::{ContinueToken, NamespaceName, ObjectName, ResourceVersion, Uid};

/// An RFC 3339 timestamp as the server wrote it (`2026-11-04T10:00:00Z`). Kept as text: the
/// SDK does no date arithmetic and adds no date dependency.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(transparent)]
pub struct Time(pub String);

/// `ObjectMeta`, the fields this SDK reads (`metadata` of every object).
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ObjectMeta {
    /// `name`; absent only in objects that are not persisted (a bookmark's object).
    pub name: Option<ObjectName>,
    /// `namespace`; absent for cluster-scoped kinds (Namespace).
    pub namespace: Option<NamespaceName>,
    /// `uid`.
    pub uid: Option<Uid>,
    /// `resourceVersion` of this object.
    pub resource_version: Option<ResourceVersion>,
    /// `generation` (spec changes).
    pub generation: Option<i64>,
    /// `creationTimestamp`.
    pub creation_timestamp: Option<Time>,
    /// `deletionTimestamp`: set while the object is being deleted.
    pub deletion_timestamp: Option<Time>,
    /// `labels`.
    pub labels: BTreeMap<String, String>,
    /// `annotations`.
    pub annotations: BTreeMap<String, String>,
    /// `ownerReferences`.
    pub owner_references: Vec<OwnerReference>,
}

/// `OwnerReference`, the fields this SDK reads.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OwnerReference {
    /// `apiVersion` of the owner.
    pub api_version: String,
    /// `kind` of the owner.
    pub kind: String,
    /// `name` of the owner.
    pub name: String,
    /// `uid` of the owner.
    pub uid: String,
    /// `controller`: whether the owner is the managing controller.
    pub controller: Option<bool>,
}

/// `ListMeta`: the collection's metadata in a list response.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ListMeta {
    /// The collection's `resourceVersion`: the snapshot this list (and every chunk continued from
    /// it) was served at, and where a watch starts.
    pub resource_version: Option<ResourceVersion>,
    /// `continue`: present and non-empty when more chunks exist. Opaque.
    #[serde(rename = "continue")]
    pub continue_token: Option<ContinueToken>,
    /// `remainingItemCount`: an **estimate** of the items after this chunk. Absent when the
    /// request used selectors, on the last chunk, and from servers that do not compute it. Never
    /// a basis for computing or parallelizing continuation tokens.
    pub remaining_item_count: Option<i64>,
}

impl ListMeta {
    /// The continue token, if the server sent a non-empty one (`""` means "no more chunks").
    pub fn next_token(&self) -> Option<&ContinueToken> {
        self.continue_token
            .as_ref()
            .filter(|token| !token.expose().is_empty())
    }

    /// The collection resource version, if the server sent a non-empty one.
    pub fn snapshot(&self) -> Option<&ResourceVersion> {
        self.resource_version.as_ref().filter(|rv| !rv.is_empty())
    }
}

/// A Kubernetes list envelope (`PodList`, `DeploymentList`, ...): `kind`, `apiVersion`,
/// `metadata` and `items`. [`ObjectList::into_parts`] moves the items out of the envelope.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", bound(deserialize = "T: Deserialize<'de>"))]
pub struct ObjectList<T> {
    /// `kind`, such as `PodList`.
    #[serde(default)]
    pub kind: Option<String>,
    /// `apiVersion`, such as `v1`.
    #[serde(default)]
    pub api_version: Option<String>,
    /// The collection metadata.
    #[serde(default)]
    pub metadata: ListMeta,
    /// The objects of this response (one chunk of a chunked list). `null` is read as empty.
    #[serde(default, deserialize_with = "null_as_empty")]
    pub items: Vec<T>,
}

impl<T> ObjectList<T> {
    /// The collection metadata and the moved items.
    pub fn into_parts(self) -> (ListMeta, Vec<T>) {
        (self.metadata, self.items)
    }
}

fn null_as_empty<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Option::<Vec<T>>::deserialize(deserializer)?.unwrap_or_default())
}

/// The `Status` document: the body of an error response and the object of a watch `ERROR`
/// event.
///
/// Its `Debug` and the SDK's error `Display` never print [`message`](Self::message), which can
/// quote object names; read it deliberately.
#[derive(Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Status {
    /// `status`: `Success` or `Failure`.
    pub status: Option<String>,
    /// `reason`, machine readable: `Expired`, `Gone`, `NotFound`, `Forbidden`, ...
    pub reason: Option<String>,
    /// `code`: the suggested HTTP status code.
    pub code: Option<u16>,
    /// `message`, human readable (may name objects).
    pub message: Option<String>,
    /// `metadata`: for an expired `continue`, `metadata.continue` holds a token for an
    /// **inconsistent** continuation (deliberately not used by this SDK).
    pub metadata: ListMeta,
    /// `details`.
    pub details: Option<StatusDetails>,
}

impl Status {
    /// `410` with reason `Expired` or `Gone`: the requested resource version (a continue token's
    /// snapshot, a watch start) is no longer available. The client must relist.
    pub fn is_expired(&self) -> bool {
        self.code == Some(410) || matches!(self.reason.as_deref(), Some("Expired" | "Gone"))
    }
}

impl fmt::Debug for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Status")
            .field("status", &self.status)
            .field("reason", &self.reason)
            .field("code", &self.code)
            .field("message", &self.message.as_ref().map(|_| "<withheld>"))
            .field(
                "continue",
                &self.metadata.continue_token.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

/// `StatusDetails`, the fields this SDK reads.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StatusDetails {
    /// `kind` of the resource concerned.
    pub kind: Option<String>,
    /// `group` of the resource concerned.
    pub group: Option<String>,
    /// `retryAfterSeconds`.
    pub retry_after_seconds: Option<u32>,
}

/// `version.Info`, the answer of `GET /version/`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct VersionInfo {
    /// `major`, such as `1`.
    pub major: String,
    /// `minor`, such as `37`.
    pub minor: String,
    /// `gitVersion`, such as `v1.37.2`.
    pub git_version: String,
    /// `gitCommit`.
    pub git_commit: String,
    /// `platform`, such as `linux/amd64`.
    pub platform: String,
}

/// A label selector inside an object (`spec.selector` of a Deployment): `matchLabels` and
/// `matchExpressions`, as stored. Not to be confused with the query-side
/// [`LabelSelector`](crate::LabelSelector).
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ObjectLabelSelector {
    /// `matchLabels`.
    pub match_labels: BTreeMap<String, String>,
    /// `matchExpressions`.
    pub match_expressions: Vec<LabelSelectorRequirement>,
}

/// One `matchExpressions` entry.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LabelSelectorRequirement {
    /// `key`.
    pub key: String,
    /// `operator`: `In`, `NotIn`, `Exists`, `DoesNotExist`.
    pub operator: String,
    /// `values`.
    pub values: Vec<String>,
}

/// A status condition (`type`, `status`, `reason`) of a Deployment, Job or Namespace.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Condition {
    /// `type`, such as `Available` or `Complete`.
    #[serde(rename = "type")]
    pub kind: String,
    /// `status`: `True`, `False` or `Unknown`.
    pub status: String,
    /// `reason`.
    pub reason: Option<String>,
    /// `lastTransitionTime`.
    pub last_transition_time: Option<Time>,
}

impl Condition {
    /// `status == "True"`.
    pub fn is_true(&self) -> bool {
        self.status == "True"
    }
}

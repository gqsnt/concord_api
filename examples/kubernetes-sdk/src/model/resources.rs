//! The six resource kinds of this slice, with the spec/status subsets the SDK reads.
//!
//! Field names and types follow the pinned OpenAPI v3 schemas (Kubernetes v1.37.2, see
//! `PROVIDER.md`). Only the listed fields are modelled; every other field is ignored on decode.
//! Open string enumerations (phases, service types) are typed with a catch-all variant so a value
//! added by a newer server still decodes.

use std::collections::BTreeMap;

use serde::Deserialize;

use super::meta::{Condition, ObjectLabelSelector, ObjectMeta, Time};

/// A kind this SDK can list (and possibly watch): its object type, kind names and metadata
/// access.
pub trait Resource: serde::de::DeserializeOwned + Send + 'static {
    /// `kind`, such as `Pod`.
    const KIND: &'static str;
    /// `apiVersion`, such as `v1` or `apps/v1`.
    const API_VERSION: &'static str;
    /// Whether objects of this kind live in a namespace.
    const NAMESPACED: bool;
    /// The object's metadata.
    fn metadata(&self) -> &ObjectMeta;
}

macro_rules! resource {
    ($ty:ty, $kind:literal, $api:literal, $namespaced:literal) => {
        impl Resource for $ty {
            const KIND: &'static str = $kind;
            const API_VERSION: &'static str = $api;
            const NAMESPACED: bool = $namespaced;
            fn metadata(&self) -> &ObjectMeta {
                &self.metadata
            }
        }
    };
}

// ---------------------------------------------------------------------------------------------
// core/v1 Namespace

/// `core/v1` Namespace.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Namespace {
    /// `metadata`.
    pub metadata: ObjectMeta,
    /// `status`.
    pub status: NamespaceStatus,
}

/// `NamespaceStatus`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct NamespaceStatus {
    /// `phase`: `Active` or `Terminating`.
    pub phase: Option<NamespacePhase>,
}

/// `NamespaceStatus.phase`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum NamespacePhase {
    /// `Active`.
    Active,
    /// `Terminating`.
    Terminating,
    /// A phase this SDK does not know.
    #[serde(other)]
    Other,
}

resource!(Namespace, "Namespace", "v1", false);

// ---------------------------------------------------------------------------------------------
// core/v1 Pod

/// `core/v1` Pod.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Pod {
    /// `metadata`.
    pub metadata: ObjectMeta,
    /// `spec`.
    pub spec: PodSpec,
    /// `status`.
    pub status: PodStatus,
}

/// `PodSpec`, the fields this SDK reads.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PodSpec {
    /// `nodeName`: the node the pod is bound to.
    pub node_name: Option<String>,
    /// `serviceAccountName`.
    pub service_account_name: Option<String>,
    /// `containers` (name and image).
    pub containers: Vec<Container>,
}

/// `Container`, the fields this SDK reads.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Container {
    /// `name`.
    pub name: String,
    /// `image`.
    pub image: Option<String>,
}

/// `PodStatus`, the fields this SDK reads.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PodStatus {
    /// `phase`.
    pub phase: Option<PodPhase>,
    /// `podIP`.
    #[serde(rename = "podIP")]
    pub pod_ip: Option<String>,
    /// `startTime`.
    pub start_time: Option<Time>,
    /// `containerStatuses`.
    pub container_statuses: Vec<ContainerStatus>,
}

/// `PodStatus.phase`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum PodPhase {
    /// `Pending`.
    Pending,
    /// `Running`.
    Running,
    /// `Succeeded`.
    Succeeded,
    /// `Failed`.
    Failed,
    /// `Unknown` (the API's own value).
    Unknown,
    /// A phase this SDK does not know.
    #[serde(other)]
    Other,
}

/// `ContainerStatus`, the fields this SDK reads.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ContainerStatus {
    /// `name`.
    pub name: String,
    /// `ready`.
    pub ready: bool,
    /// `restartCount`.
    pub restart_count: i32,
}

impl Pod {
    /// Whether every container reports ready (and there is at least one status).
    pub fn all_containers_ready(&self) -> bool {
        !self.status.container_statuses.is_empty()
            && self.status.container_statuses.iter().all(|c| c.ready)
    }
}

resource!(Pod, "Pod", "v1", true);

// ---------------------------------------------------------------------------------------------
// core/v1 Service

/// `core/v1` Service.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Service {
    /// `metadata`.
    pub metadata: ObjectMeta,
    /// `spec`.
    pub spec: ServiceSpec,
}

/// `ServiceSpec`, the fields this SDK reads.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ServiceSpec {
    /// `type`.
    #[serde(rename = "type")]
    pub kind: Option<ServiceType>,
    /// `clusterIP` (`None` is the literal string `"None"` for headless services).
    #[serde(rename = "clusterIP")]
    pub cluster_ip: Option<String>,
    /// `selector`.
    pub selector: BTreeMap<String, String>,
    /// `ports`.
    pub ports: Vec<ServicePort>,
}

/// `ServiceSpec.type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum ServiceType {
    /// `ClusterIP`.
    ClusterIP,
    /// `NodePort`.
    NodePort,
    /// `LoadBalancer`.
    LoadBalancer,
    /// `ExternalName`.
    ExternalName,
    /// A type this SDK does not know.
    #[serde(other)]
    Other,
}

/// `ServicePort`, the fields this SDK reads.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ServicePort {
    /// `name`.
    pub name: Option<String>,
    /// `protocol`: `TCP` (default), `UDP`, `SCTP`.
    pub protocol: Option<String>,
    /// `port`.
    pub port: i32,
    /// `targetPort`: a number or a named port.
    pub target_port: Option<IntOrString>,
}

/// `IntOrString`: a JSON number or string.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum IntOrString {
    /// A number.
    Int(i32),
    /// A name (or a percentage).
    String(String),
}

resource!(Service, "Service", "v1", true);

// ---------------------------------------------------------------------------------------------
// core/v1 ConfigMap

/// `core/v1` ConfigMap. `data` values are configuration contents: the SDK's own diagnostics
/// never print them (this type's `Debug` lists keys only).
#[derive(Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ConfigMap {
    /// `metadata`.
    pub metadata: ObjectMeta,
    /// `data`: UTF-8 entries.
    pub data: BTreeMap<String, String>,
    /// `binaryData`: base64 entries, kept encoded.
    pub binary_data: BTreeMap<String, String>,
    /// `immutable`.
    pub immutable: Option<bool>,
}

impl std::fmt::Debug for ConfigMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConfigMap")
            .field("metadata", &self.metadata)
            .field("data_keys", &self.data.keys().collect::<Vec<_>>())
            .field(
                "binary_data_keys",
                &self.binary_data.keys().collect::<Vec<_>>(),
            )
            .field("immutable", &self.immutable)
            .finish()
    }
}

resource!(ConfigMap, "ConfigMap", "v1", true);

// ---------------------------------------------------------------------------------------------
// apps/v1 Deployment

/// `apps/v1` Deployment.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Deployment {
    /// `metadata`.
    pub metadata: ObjectMeta,
    /// `spec`.
    pub spec: DeploymentSpec,
    /// `status`.
    pub status: DeploymentStatus,
}

/// `DeploymentSpec`, the fields this SDK reads.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DeploymentSpec {
    /// `replicas` (absent means the server default, 1).
    pub replicas: Option<i32>,
    /// `selector`.
    pub selector: ObjectLabelSelector,
    /// `paused`.
    pub paused: Option<bool>,
}

/// `DeploymentStatus`, the fields this SDK reads.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DeploymentStatus {
    /// `observedGeneration`.
    pub observed_generation: Option<i64>,
    /// `replicas`.
    pub replicas: Option<i32>,
    /// `updatedReplicas`.
    pub updated_replicas: Option<i32>,
    /// `readyReplicas`.
    pub ready_replicas: Option<i32>,
    /// `availableReplicas`.
    pub available_replicas: Option<i32>,
    /// `unavailableReplicas`.
    pub unavailable_replicas: Option<i32>,
    /// `conditions`.
    pub conditions: Vec<Condition>,
}

impl Deployment {
    /// Desired replicas (`spec.replicas`, server default 1) equal available replicas and the
    /// status reflects the current generation.
    pub fn is_available(&self) -> bool {
        let desired = self.spec.replicas.unwrap_or(1);
        let observed = self.status.observed_generation >= self.metadata.generation;
        observed && self.status.available_replicas.unwrap_or(0) >= desired
    }
}

resource!(Deployment, "Deployment", "apps/v1", true);

// ---------------------------------------------------------------------------------------------
// batch/v1 Job

/// `batch/v1` Job.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Job {
    /// `metadata`.
    pub metadata: ObjectMeta,
    /// `spec`.
    pub spec: JobSpec,
    /// `status`.
    pub status: JobStatus,
}

/// `JobSpec`, the fields this SDK reads.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct JobSpec {
    /// `parallelism`.
    pub parallelism: Option<i32>,
    /// `completions`.
    pub completions: Option<i32>,
    /// `backoffLimit`.
    pub backoff_limit: Option<i32>,
    /// `suspend`.
    pub suspend: Option<bool>,
}

/// `JobStatus`, the fields this SDK reads.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct JobStatus {
    /// `active` pods.
    pub active: Option<i32>,
    /// `succeeded` pods.
    pub succeeded: Option<i32>,
    /// `failed` pods.
    pub failed: Option<i32>,
    /// `ready` pods.
    pub ready: Option<i32>,
    /// `startTime`.
    pub start_time: Option<Time>,
    /// `completionTime`.
    pub completion_time: Option<Time>,
    /// `conditions` (`Complete`, `Failed`, `Suspended`, ...).
    pub conditions: Vec<Condition>,
}

/// A Job's terminal state, derived from its conditions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobState {
    /// A `Complete` condition is `True`.
    Complete,
    /// A `Failed` condition is `True`.
    Failed,
    /// Neither: still running, suspended or pending.
    Active,
}

impl Job {
    /// The state its conditions report.
    pub fn state(&self) -> JobState {
        let has = |kind: &str| {
            self.status
                .conditions
                .iter()
                .any(|c| c.kind == kind && c.is_true())
        };
        if has("Complete") {
            JobState::Complete
        } else if has("Failed") {
            JobState::Failed
        } else {
            JobState::Active
        }
    }
}

resource!(Job, "Job", "batch/v1", true);

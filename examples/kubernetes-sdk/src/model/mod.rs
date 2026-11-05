//! Typed object models: `meta/v1` shapes, the seven resource kinds and their list envelopes.

mod meta;
mod resources;

pub use meta::{
    Condition, LabelSelectorRequirement, ListMeta, ObjectLabelSelector, ObjectList, ObjectMeta,
    OwnerReference, Status, StatusDetails, Time, VersionInfo,
};
pub use resources::{
    ConfigMap, Container, ContainerStatus, Deployment, DeploymentSpec, DeploymentStatus,
    HttpIngressPath, HttpIngressRuleValue, Ingress, IngressBackend, IngressLoadBalancerIngress,
    IngressLoadBalancerStatus, IngressRule, IngressServiceBackend, IngressSpec, IngressStatus,
    IngressTls, IntOrString, Job, JobSpec, JobState, JobStatus, Namespace, NamespacePhase,
    NamespaceStatus, PathType, Pod, PodPhase, PodSpec, PodStatus, Resource, Service,
    ServiceBackendPort, ServicePort, ServiceSpec, ServiceType, TypedLocalObjectReference,
};

/// `NamespaceList`.
pub type NamespaceList = ObjectList<Namespace>;
/// `PodList`.
pub type PodList = ObjectList<Pod>;
/// `ServiceList`.
pub type ServiceList = ObjectList<Service>;
/// `ConfigMapList`.
pub type ConfigMapList = ObjectList<ConfigMap>;
/// `DeploymentList`.
pub type DeploymentList = ObjectList<Deployment>;
/// `JobList`.
pub type JobList = ObjectList<Job>;
/// `IngressList` (`networking.k8s.io/v1`).
pub type IngressList = ObjectList<Ingress>;

/// Where an object lives: its namespace (absent for cluster-scoped kinds) and name. The key of a
/// current-state map.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectKey {
    /// The namespace, `None` for cluster-scoped objects.
    pub namespace: Option<crate::NamespaceName>,
    /// The name.
    pub name: crate::ObjectName,
}

impl ObjectKey {
    /// The key of `object`, `None` when its metadata has no name (not a persisted object).
    pub fn of<R: Resource>(object: &R) -> Option<Self> {
        let meta = object.metadata();
        Some(Self {
            namespace: meta.namespace.clone(),
            name: meta.name.clone()?,
        })
    }
}

impl std::fmt::Display for ObjectKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.namespace {
            Some(ns) => write!(f, "{ns}/{}", self.name),
            None => write!(f, "{}", self.name),
        }
    }
}

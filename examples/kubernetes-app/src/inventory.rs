//! KU-A — Kubernetes workload inventory.
//!
//! **Input:** a configured [`KubeContext`], the pinned server target (the SDK's
//! [`PINNED_SERVER_VERSION`]), a scope (all namespaces, or named namespaces), label/field
//! selectors and traversal limits ([`ListPolicy`]). **Output:** an [`Inventory`]: typed objects
//! per resource family and scope, each with its collection [`Completion`] (outcome, snapshot
//! `resourceVersion`, chunks, `remainingItemCount` estimates, restart boundaries).
//!
//! Policy:
//!
//! - Families are read one after the other, each collection with its own consistent chunked list
//!   ([`collect_list`]). A family that fails does not hide the others: its completion carries the
//!   failure and the rest of the inventory is still reported.
//! - The label selector applies to the five workload families (Pods, Deployments, Services,
//!   ConfigMaps, Jobs); the field selector to Pods only (field selectors are kind-specific:
//!   `status.phase`, `spec.nodeName` exist on Pods only). Namespaces are never filtered.
//! - With named namespaces, each namespace is read with `GET /api/v1/namespaces/{name}` (no
//!   cluster-wide list permission needed); with all namespaces, Namespaces are listed.
//! - The server's `/version/` is read and compared with the pinned target; a different server
//!   is recorded, not refused (the core/apps/batch v1 resources are stable across releases).

use concord_example_kubernetes_sdk::list::ListOperation;
use concord_example_kubernetes_sdk::model::{
    ConfigMap, Deployment, Job, JobState, Namespace, Pod, PodPhase, Service, VersionInfo,
};
use concord_example_kubernetes_sdk::{
    FieldSelector, KubeContext, KubeError, LabelSelector, NamespaceName, PINNED_SERVER_VERSION,
};
use concord_pagination::Resume;

use crate::collect::{Collected, Completion, ListPolicy, collect_list};

/// Which namespaces the inventory covers.
#[derive(Clone, Debug)]
pub enum InventoryScope {
    /// Cluster-wide collections (`/api/v1/pods`, ...), one per family.
    AllNamespaces,
    /// One collection per family and namespace, in this order.
    Namespaces(Vec<NamespaceName>),
}

/// What KU-A reads.
#[derive(Clone, Debug)]
pub struct InventoryRequest {
    /// The namespaces.
    pub scope: InventoryScope,
    /// `labelSelector` for the workload families.
    pub label_selector: Option<LabelSelector>,
    /// `fieldSelector` for Pods.
    pub pod_field_selector: Option<FieldSelector>,
    /// Chunk size, chunk budget and expiry policy of every collection.
    pub list: ListPolicy,
}

/// Where a collection was read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CollectionScope {
    /// Cluster-wide.
    AllNamespaces,
    /// One namespace.
    Namespace(NamespaceName),
}

/// One family's objects in one scope, with completion metadata.
#[derive(Debug)]
pub struct Family<T> {
    /// Where.
    pub scope: CollectionScope,
    /// The typed objects (see [`Completion::outcome`] for whether they are the whole collection).
    pub items: Vec<T>,
    /// Outcome, snapshot `resourceVersion`, chunks, restarts.
    pub completion: Completion,
}

/// The server check against the pinned target.
#[derive(Debug)]
pub struct ServerCheck {
    /// The version this SDK was checked against (`v1.37.2`).
    pub pinned: &'static str,
    /// What `GET /version/` answered.
    pub observed: Result<VersionInfo, KubeError>,
}

impl ServerCheck {
    /// Whether the server reports the pinned major.minor (`1.37`).
    pub fn same_minor(&self) -> Option<bool> {
        let info = self.observed.as_ref().ok()?;
        let pinned = self.pinned.trim_start_matches('v');
        let mut parts = pinned.split('.');
        let (major, minor) = (parts.next()?, parts.next()?);
        // `minor` may carry a `+` suffix on some distributions (`37+`).
        Some(info.major == major && info.minor.trim_end_matches('+') == minor)
    }
}

/// A requested namespace read with `GET`.
#[derive(Debug)]
pub struct NamespaceEntry {
    /// The requested name.
    pub name: NamespaceName,
    /// The object, or why it could not be read.
    pub result: Result<Namespace, KubeError>,
}

/// The KU-A result.
#[derive(Debug)]
pub struct Inventory {
    /// The server version check.
    pub server: ServerCheck,
    /// Named namespaces, each read with `GET` (empty for [`InventoryScope::AllNamespaces`]).
    pub namespaces: Vec<NamespaceEntry>,
    /// The namespace collection (only for [`InventoryScope::AllNamespaces`]).
    pub namespace_list: Option<Family<Namespace>>,
    /// Pods, one entry per scope.
    pub pods: Vec<Family<Pod>>,
    /// Deployments.
    pub deployments: Vec<Family<Deployment>>,
    /// Services.
    pub services: Vec<Family<Service>>,
    /// ConfigMaps.
    pub config_maps: Vec<Family<ConfigMap>>,
    /// Jobs.
    pub jobs: Vec<Family<Job>>,
}

impl Inventory {
    /// Whether every collection was read completely and every named namespace exists.
    pub fn is_complete(&self) -> bool {
        fn all<T>(families: &[Family<T>]) -> bool {
            families.iter().all(|f| f.completion.outcome.is_complete())
        }
        self.namespaces.iter().all(|n| n.result.is_ok())
            && self
                .namespace_list
                .as_ref()
                .is_none_or(|f| f.completion.outcome.is_complete())
            && all(&self.pods)
            && all(&self.deployments)
            && all(&self.services)
            && all(&self.config_maps)
            && all(&self.jobs)
    }

    /// Counts derived from the typed objects.
    pub fn summary(&self) -> InventorySummary {
        let pods = self.pods.iter().flat_map(|f| &f.items);
        let mut summary = InventorySummary::default();
        for pod in pods {
            summary.pods += 1;
            match pod.status.phase {
                Some(PodPhase::Running) => summary.pods_running += 1,
                Some(PodPhase::Pending) => summary.pods_pending += 1,
                _ => {}
            }
        }
        for d in self.deployments.iter().flat_map(|f| &f.items) {
            summary.deployments += 1;
            if !d.is_available() {
                summary.deployments_unavailable += 1;
            }
        }
        summary.services = self.services.iter().map(|f| f.items.len()).sum();
        summary.config_maps = self.config_maps.iter().map(|f| f.items.len()).sum();
        for j in self.jobs.iter().flat_map(|f| &f.items) {
            summary.jobs += 1;
            match j.state() {
                JobState::Complete => summary.jobs_complete += 1,
                JobState::Failed => summary.jobs_failed += 1,
                JobState::Active => {}
            }
        }
        summary
    }
}

/// Counts per family.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InventorySummary {
    /// Pods.
    pub pods: usize,
    /// Pods in phase `Running`.
    pub pods_running: usize,
    /// Pods in phase `Pending`.
    pub pods_pending: usize,
    /// Deployments.
    pub deployments: usize,
    /// Deployments with fewer available replicas than desired (or a stale status).
    pub deployments_unavailable: usize,
    /// Services.
    pub services: usize,
    /// ConfigMaps.
    pub config_maps: usize,
    /// Jobs.
    pub jobs: usize,
    /// Jobs with a true `Complete` condition.
    pub jobs_complete: usize,
    /// Jobs with a true `Failed` condition.
    pub jobs_failed: usize,
}

async fn family<O: ListOperation>(
    scope: CollectionScope,
    template: O,
    policy: ListPolicy,
) -> Family<O::Item> {
    let Collected { items, completion } = collect_list(template, policy, Resume::Start).await;
    Family {
        scope,
        items,
        completion,
    }
}

/// Runs KU-A.
pub async fn inventory(kube: &KubeContext, request: &InventoryRequest) -> Inventory {
    let labels = request.label_selector.clone();
    let pod_fields = request.pod_field_selector.clone();
    let policy = request.list;
    let server = ServerCheck {
        pinned: PINNED_SERVER_VERSION,
        observed: match kube.client().version().send().await {
            Ok(info) => Ok(info),
            Err(error) => Err(KubeError::from_concord(error).await),
        },
    };
    let mut inventory = Inventory {
        server,
        namespaces: Vec::new(),
        namespace_list: None,
        pods: Vec::new(),
        deployments: Vec::new(),
        services: Vec::new(),
        config_maps: Vec::new(),
        jobs: Vec::new(),
    };
    match &request.scope {
        InventoryScope::AllNamespaces => {
            let all = CollectionScope::AllNamespaces;
            let core = kube.core();
            inventory.namespace_list = Some(family(all.clone(), core.namespaces(), policy).await);
            let pods = core
                .pods()
                .with_label_selector(labels.clone())
                .with_field_selector(pod_fields.clone());
            inventory.pods.push(family(all.clone(), pods, policy).await);
            let deployments = kube
                .apps()
                .deployments()
                .with_label_selector(labels.clone());
            inventory
                .deployments
                .push(family(all.clone(), deployments, policy).await);
            let services = core.services().with_label_selector(labels.clone());
            inventory
                .services
                .push(family(all.clone(), services, policy).await);
            let config_maps = core.config_maps().with_label_selector(labels.clone());
            inventory
                .config_maps
                .push(family(all.clone(), config_maps, policy).await);
            let jobs = kube.batch().jobs().with_label_selector(labels.clone());
            inventory.jobs.push(family(all, jobs, policy).await);
        }
        InventoryScope::Namespaces(names) => {
            for name in names {
                let result = match kube.core().namespace(name.clone()).send().await {
                    Ok(namespace) => Ok(namespace),
                    Err(error) => Err(KubeError::from_concord(error).await),
                };
                inventory.namespaces.push(NamespaceEntry {
                    name: name.clone(),
                    result,
                });
            }
            for name in names {
                let ns = kube.namespace(name.clone());
                let scope = CollectionScope::Namespace(name.clone());
                let pods = ns
                    .core()
                    .pods()
                    .with_label_selector(labels.clone())
                    .with_field_selector(pod_fields.clone());
                inventory
                    .pods
                    .push(family(scope.clone(), pods, policy).await);
                let deployments = ns.apps().deployments().with_label_selector(labels.clone());
                inventory
                    .deployments
                    .push(family(scope.clone(), deployments, policy).await);
                let services = ns.core().services().with_label_selector(labels.clone());
                inventory
                    .services
                    .push(family(scope.clone(), services, policy).await);
                let config_maps = ns.core().config_maps().with_label_selector(labels.clone());
                inventory
                    .config_maps
                    .push(family(scope.clone(), config_maps, policy).await);
                let jobs = ns.batch().jobs().with_label_selector(labels.clone());
                inventory.jobs.push(family(scope, jobs, policy).await);
            }
        }
    }
    inventory
}

//! The updated consumer (stage 3): the release audit after the SDK gained the
//! `networking.k8s.io/v1` Ingress family, with a changed selector.
//!
//! Requirement change, as a downstream team would state it: the `checkout` components were split
//! out of `shop` into their own `app.kubernetes.io/part-of=checkout` application, a release now
//! covers both, and canary objects (label `example.com/canary`, any value) are not part of a
//! release. The audit also checks how the release is exposed: every Ingress backend must name an
//! audited Service and a port that Service declares, and every Ingress should have a published
//! address.
//!
//! - The selector changes from equality (`app.kubernetes.io/part-of=shop`, [`crate::before`]) to
//!   set-based requirements ([`release_selector`]:
//!   `app.kubernetes.io/part-of in (checkout,shop),!example.com/canary`), built with the SDK's
//!   [`LabelSelector`] builder and applied unchanged to every list and to the watch.
//! - Ingresses are read through the published context (`NamespaceContext::networking()`) with the
//!   frozen consumer's generic [`collect`] and watched with its generic [`follow`]: no new list or
//!   watch code here.

use std::ops::ControlFlow;

use concord_example_kubernetes_sdk::model::{Deployment, Ingress, Service};
use concord_example_kubernetes_sdk::watch::{WatchEvent, WatchOpenError};
use concord_example_kubernetes_sdk::{
    KubeContext, LabelSelector, NamespaceContext, NamespaceName, ObjectName, ResourceVersion,
    SelectorError,
};

use crate::before::{
    AuditError, FollowLimits, FollowReport, Limits, Listed, PART_OF, collect, follow,
};

/// The label that marks a canary object (excluded from a release whatever its value).
pub const CANARY: &str = "example.com/canary";

/// The release selector: `app.kubernetes.io/part-of in (<apps>),!example.com/canary`.
///
/// # Errors
///
/// [`SelectorError`] for an empty application list or an invalid application name.
pub fn release_selector<'a>(
    apps: impl IntoIterator<Item = &'a str>,
) -> Result<LabelSelector, SelectorError> {
    LabelSelector::builder()
        .in_set(PART_OF, apps)
        .not_exists(CANARY)
        .build()
}

/// What the release audit reads.
#[derive(Clone, Debug)]
pub struct ReleaseRequest {
    /// The applications of the release (`app.kubernetes.io/part-of` values).
    pub apps: Vec<String>,
    /// The namespaces, in order.
    pub namespaces: Vec<NamespaceName>,
    /// Chunking of every collection.
    pub limits: Limits,
}

/// An Ingress backend that does not reach an audited Service port.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DanglingBackend {
    /// The Ingress.
    pub ingress: ObjectName,
    /// The backend's Service name.
    pub service: String,
    /// Why it dangles.
    pub problem: BackendProblem,
}

/// Why an Ingress backend dangles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BackendProblem {
    /// No audited Service has that name (absent, or outside the release selector).
    NoSuchService,
    /// The Service declares no port with that number.
    NoSuchPortNumber(i32),
    /// The Service declares no port with that name.
    NoSuchPortName(String),
}

/// One namespace of the release audit.
#[derive(Debug)]
pub struct NamespaceRelease {
    /// The namespace.
    pub namespace: NamespaceName,
    /// The release's Deployments.
    pub deployments: Listed<Deployment>,
    /// The release's Services.
    pub services: Listed<Service>,
    /// The release's Ingresses.
    pub ingresses: Listed<Ingress>,
}

fn named(name: &Option<ObjectName>) -> Option<&str> {
    name.as_ref().map(ObjectName::as_str)
}

impl NamespaceRelease {
    /// Deployments with fewer available replicas than desired (or a stale status), by name.
    pub fn unavailable(&self) -> Vec<&ObjectName> {
        self.deployments
            .items
            .iter()
            .filter(|d| !d.is_available())
            .filter_map(|d| d.metadata.name.as_ref())
            .collect()
    }

    /// Ingress backends that do not reach a port of an audited Service, in Ingress order.
    pub fn dangling_backends(&self) -> Vec<DanglingBackend> {
        let mut dangling = Vec::new();
        for ingress in &self.ingresses.items {
            let Some(ingress_name) = ingress.metadata.name.clone() else {
                continue;
            };
            for backend in ingress.service_backends() {
                let service = self
                    .services
                    .items
                    .iter()
                    .find(|s| named(&s.metadata.name) == Some(backend.name.as_str()));
                let problem = match (service, &backend.port) {
                    (None, _) => Some(BackendProblem::NoSuchService),
                    (Some(_), None) => None,
                    (Some(service), Some(port)) => match (&port.name, port.number) {
                        (Some(name), _) => (!service
                            .spec
                            .ports
                            .iter()
                            .any(|p| p.name.as_ref() == Some(name)))
                        .then(|| BackendProblem::NoSuchPortName(name.clone())),
                        (None, Some(number)) => {
                            (!service.spec.ports.iter().any(|p| p.port == number))
                                .then_some(BackendProblem::NoSuchPortNumber(number))
                        }
                        (None, None) => None,
                    },
                };
                if let Some(problem) = problem {
                    dangling.push(DanglingBackend {
                        ingress: ingress_name.clone(),
                        service: backend.name.clone(),
                        problem,
                    });
                }
            }
        }
        dangling
    }

    /// Ingresses whose controller published no address yet, by name.
    pub fn unpublished(&self) -> Vec<&ObjectName> {
        self.ingresses
            .items
            .iter()
            .filter(|i| !i.has_address())
            .filter_map(|i| i.metadata.name.as_ref())
            .collect()
    }
}

/// The release audit's result.
#[derive(Debug)]
pub struct Release {
    /// The selector every collection was read with.
    pub selector: LabelSelector,
    /// One entry per namespace, in request order.
    pub namespaces: Vec<NamespaceRelease>,
}

impl Release {
    /// Every Deployment available, every Ingress backend reachable and every Ingress published.
    pub fn ready(&self) -> bool {
        self.namespaces.iter().all(|n| {
            n.unavailable().is_empty()
                && n.dangling_backends().is_empty()
                && n.unpublished().is_empty()
        })
    }
}

/// Runs the release audit: Deployments, Services and Ingresses of the release in each namespace.
///
/// # Errors
///
/// [`AuditError`] (the frozen consumer's error type) for an invalid selector or the first
/// collection that fails; the family is `deployments`, `services` or `ingresses`.
pub async fn release_audit(
    kube: &KubeContext,
    request: &ReleaseRequest,
) -> Result<Release, AuditError> {
    let selector =
        release_selector(request.apps.iter().map(String::as_str)).map_err(AuditError::Selector)?;
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
        let ingresses = collect(
            scope
                .networking()
                .ingresses()
                .set_label_selector(selector.clone()),
            request.limits,
        )
        .await
        .map_err(failed("ingresses"))?;
        namespaces.push(NamespaceRelease {
            namespace: namespace.clone(),
            deployments,
            services,
            ingresses,
        });
    }
    Ok(Release {
        selector,
        namespaces,
    })
}

/// Follows the release's Ingresses in one namespace after an audited snapshot.
///
/// # Errors
///
/// [`WatchOpenError`], as [`follow`].
pub async fn follow_ingresses<K>(
    scope: &NamespaceContext,
    selector: &LabelSelector,
    from: ResourceVersion,
    limits: FollowLimits,
    sink: K,
) -> Result<FollowReport, WatchOpenError>
where
    K: FnMut(&WatchEvent<Ingress>) -> ControlFlow<()>,
{
    follow(
        scope.networking().watch_ingresses(),
        selector,
        from,
        limits,
        sink,
    )
    .await
}

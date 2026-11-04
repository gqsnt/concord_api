//! One configured client and namespace contexts across API groups.
//!
//! Kubernetes repeats the namespace in every group's path (`/api/v1/namespaces/{ns}/pods`,
//! `/apis/apps/v1/namespaces/{ns}/deployments`, `/apis/batch/v1/namespaces/{ns}/jobs`,
//! `/apis/networking.k8s.io/v1/namespaces/{ns}/ingresses`). The declaration models that honestly —
//! each group/version scope has its own `namespaced(namespace)` scope — and [`NamespaceContext`]
//! binds one namespace across the four, so callers name it once. Everything here returns the
//! **generated** handles: callers keep the operations' setters, `prepare()` and `send()`. Like
//! Concord's handles, contexts are immutable snapshots and cheap to clone.

use std::fmt;

use concord::reqwest;

use crate::error::KubeError;
use crate::ids::{BearerToken, NamespaceName};
use crate::{
    Kube, KubeAppsV1, KubeAppsV1Namespaced, KubeBatchV1, KubeBatchV1Namespaced, KubeCoreV1,
    KubeCoreV1Namespaced, KubeNetworkingV1, KubeNetworkingV1Namespaced,
};

/// The configured Kubernetes client: base URL, credentials and the `reqwest::Client` (whose TLS
/// configuration — cluster CA, client certificates — is the caller's).
#[derive(Clone)]
pub struct KubeContext {
    client: Kube,
}

impl KubeContext {
    /// A context against the declared base (`https://kubernetes.default.svc`, the in-cluster
    /// name) without credentials.
    pub fn new(http: reqwest::Client) -> Self {
        Self {
            client: Kube::new(http),
        }
    }

    /// Replaces the API server URL (`https://127.0.0.1:6443`, a `kubectl proxy` address, a test
    /// server). Its path is kept as a prefix.
    ///
    /// # Errors
    ///
    /// Concord's `with_base_url` error for anything that is not an `http`/`https` URL with a
    /// host (and no user information, query or fragment).
    pub fn with_base_url(self, url: impl AsRef<str>) -> Result<Self, concord::Error> {
        Ok(Self {
            client: self.client.with_base_url(url)?,
        })
    }

    /// Sends `Authorization: Bearer <token>` on every request, flagged sensitive.
    ///
    /// # Errors
    ///
    /// [`KubeError::InvalidToken`] when the token cannot be a header value.
    pub fn with_bearer(self, token: &BearerToken) -> Result<Self, KubeError> {
        let value = token.header_value().map_err(|_| KubeError::InvalidToken)?;
        Ok(Self {
            client: self.client.set_authorization(value),
        })
    }

    /// The generated client.
    pub fn client(&self) -> &Kube {
        &self.client
    }

    /// `core/v1`, cluster-wide collections (`/api/v1/...`).
    pub fn core(&self) -> KubeCoreV1 {
        self.client.core_v1()
    }

    /// `apps/v1`, cluster-wide collections.
    pub fn apps(&self) -> KubeAppsV1 {
        self.client.apps_v1()
    }

    /// `batch/v1`, cluster-wide collections.
    pub fn batch(&self) -> KubeBatchV1 {
        self.client.batch_v1()
    }

    /// `networking.k8s.io/v1`, cluster-wide collections.
    pub fn networking(&self) -> KubeNetworkingV1 {
        self.client.networking_v1()
    }

    /// The namespace context of `namespace`: its scope in each group/version.
    pub fn namespace(&self, namespace: NamespaceName) -> NamespaceContext {
        NamespaceContext {
            core: self.client.core_v1().namespaced(namespace.clone()),
            apps: self.client.apps_v1().namespaced(namespace.clone()),
            batch: self.client.batch_v1().namespaced(namespace.clone()),
            networking: self.client.networking_v1().namespaced(namespace.clone()),
            namespace,
        }
    }
}

impl fmt::Debug for KubeContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The generated client's Debug withholds the sensitive `authorization` value.
        f.debug_struct("KubeContext")
            .field("client", &self.client)
            .finish()
    }
}

/// One namespace bound across `core/v1`, `apps/v1`, `batch/v1` and `networking.k8s.io/v1`.
#[derive(Clone)]
pub struct NamespaceContext {
    namespace: NamespaceName,
    core: KubeCoreV1Namespaced,
    apps: KubeAppsV1Namespaced,
    batch: KubeBatchV1Namespaced,
    networking: KubeNetworkingV1Namespaced,
}

impl NamespaceContext {
    /// The namespace.
    pub fn namespace(&self) -> &NamespaceName {
        &self.namespace
    }

    /// `core/v1` in this namespace (Pods, Services, ConfigMaps).
    pub fn core(&self) -> &KubeCoreV1Namespaced {
        &self.core
    }

    /// `apps/v1` in this namespace (Deployments).
    pub fn apps(&self) -> &KubeAppsV1Namespaced {
        &self.apps
    }

    /// `batch/v1` in this namespace (Jobs).
    pub fn batch(&self) -> &KubeBatchV1Namespaced {
        &self.batch
    }

    /// `networking.k8s.io/v1` in this namespace (Ingresses).
    pub fn networking(&self) -> &KubeNetworkingV1Namespaced {
        &self.networking
    }
}

impl fmt::Debug for NamespaceContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NamespaceContext")
            .field("namespace", &self.namespace)
            .finish_non_exhaustive()
    }
}

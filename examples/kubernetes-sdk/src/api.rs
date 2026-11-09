//! The `concord::api!` declaration: one client, one scope per API group/version, one nested
//! `namespaced(namespace)` scope per group/version.
//!
//! Paths follow the pinned OpenAPI v3 documents (Kubernetes v1.37.2): core/v1 under `api/v1`,
//! named groups under `apis/{group}/{version}`, namespaced collections under
//! `namespaces/{namespace}/{plural}`. Watches use the collection path with `watch=1` (the
//! deprecated `/watch/` path prefix is not declared) and are declared with their truthful output,
//! `-> stream WatchEvent<T>` read with `JsonStream` (see [`crate::watch`] for why its defaults
//! fit).

use concord::Json;
use concord::jsonl::JsonStream;
use concord::reqwest::header::{ACCEPT, AUTHORIZATION, HeaderValue};

use crate::ids::{ContinueToken, NamespaceName, ObjectName, ResourceVersion};
use crate::model::{
    ConfigMap, ConfigMapList, Deployment, DeploymentList, Ingress, IngressList, Job, JobList,
    Namespace, NamespaceList, Pod, PodList, Service, ServiceList, VersionInfo,
};
use crate::selector::{FieldSelector, LabelSelector};
use crate::watch::WatchEvent;

/// `resourceVersionMatch` of a list or watch request. Streaming lists require
/// [`NotOlderThan`](Self::NotOlderThan).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum ResourceVersionMatch {
    /// `NotOlderThan`: data at least as new as `resourceVersion`.
    NotOlderThan,
    /// `Exact`: data at exactly `resourceVersion` (`410 Gone` when unavailable).
    Exact,
}
serde_plain::derive_display_from_serialize!(ResourceVersionMatch);

concord::api! {
    /// A read-only slice of the Kubernetes API: get/list of Namespaces, Pods, Services,
    /// ConfigMaps (core/v1), Deployments (apps/v1), Jobs (batch/v1) and Ingresses
    /// (networking.k8s.io/v1); watch of Pods, Deployments and Ingresses; the server version.
    ///
    /// List endpoints share one parameter group, [`ListOptions`] (`labelSelector`,
    /// `fieldSelector`, `limit`, `continue`, `resourceVersion`, `resourceVersionMatch`,
    /// `timeoutSeconds`); the `continue` value is `#[sensitive]`. Watch endpoints share
    /// [`WatchOptions`]. Watch endpoints are `-> stream WatchEvent<T>`: their `send().await?`
    /// returns the open [`crate::watch::WatchStream`] without reading the body;
    /// [`crate::watch::open`] opens the same declared operation with a caller-configured reader.
    pub client Kube(
        /// `Authorization: Bearer <token>` (see [`crate::BearerToken::header_value`]). Absent:
        /// no `Authorization` header (an authenticating proxy such as `kubectl proxy`, or a
        /// transport that authenticates with client certificates).
        #[sensitive] header(AUTHORIZATION) authorization?: HeaderValue,
    ) {
        base "https://kubernetes.default.svc";
        format Json;

        /// The parameters every list endpoint (`GET .../{plural}`) shares.
        params ListOptions(
            query("labelSelector") label_selector?: LabelSelector,
            query("fieldSelector") field_selector?: FieldSelector,
            query limit?: u32,
            #[sensitive] query("continue") continue_token?: ContinueToken,
            query("resourceVersion") resource_version?: ResourceVersion,
            query("resourceVersionMatch") resource_version_match?: ResourceVersionMatch,
            query("timeoutSeconds") timeout_seconds?: u32,
        );

        /// The parameters every watch endpoint (`GET .../{plural}?watch=1`) shares.
        /// For [streaming lists](https://kubernetes.io/docs/reference/using-api/api-concepts/#streaming-lists),
        /// set `send_initial_events` to `true` and `resource_version_match` to
        /// [`ResourceVersionMatch::NotOlderThan`], keeping `allow_watch_bookmarks` enabled.
        params WatchOptions(
            query("labelSelector") label_selector?: LabelSelector,
            query("fieldSelector") field_selector?: FieldSelector,
            query("resourceVersion") resource_version?: ResourceVersion,
            /// `resourceVersionMatch`: streaming lists require `NotOlderThan`.
            query("resourceVersionMatch") resource_version_match?: ResourceVersionMatch,
            /// `sendInitialEvents`: opt into synthetic `ADDED` events for the current state.
            query("sendInitialEvents") send_initial_events?: bool,
            query("allowWatchBookmarks") allow_watch_bookmarks?: bool = true,
            query("timeoutSeconds") timeout_seconds?: u32,
        );

        /// `GET /version/`: the server's build information.
        GET version() -> VersionInfo { path "version/"; }

        /// The core group, `core/v1` (`/api/v1`).
        scope core_v1 {
            path "api/v1";

            /// `listCoreV1Namespace`: `GET /api/v1/namespaces`.
            GET namespaces(..ListOptions) -> NamespaceList { path "namespaces"; }

            /// `readCoreV1Namespace`: `GET /api/v1/namespaces/{name}`.
            GET namespace(name: NamespaceName) -> Namespace { path "namespaces/{name}"; }

            /// `listCoreV1PodForAllNamespaces`: `GET /api/v1/pods`.
            GET pods(..ListOptions) -> PodList { path "pods"; }

            /// `listCoreV1PodForAllNamespaces` with `watch=1`: `GET /api/v1/pods?watch=1`. A stream
            /// of consecutive JSON watch events, returned unread by `send()`.
            GET watch_pods(..WatchOptions) -> stream WatchEvent<Pod> {
                path "pods";
                query { "watch" = "1"; }
                headers { ACCEPT = "application/json"; }
                response JsonStream;
            }

            /// `listCoreV1ServiceForAllNamespaces`: `GET /api/v1/services`.
            GET services(..ListOptions) -> ServiceList { path "services"; }

            /// `listCoreV1ConfigMapForAllNamespaces`: `GET /api/v1/configmaps`.
            GET config_maps(..ListOptions) -> ConfigMapList { path "configmaps"; }

            /// One namespace of the core group (`/api/v1/namespaces/{namespace}`).
            scope namespaced(namespace: NamespaceName) {
                path "namespaces/{namespace}";

                /// `listCoreV1NamespacedPod`.
                GET pods(..ListOptions) -> PodList { path "pods"; }

                /// `readCoreV1NamespacedPod`.
                GET pod(name: ObjectName) -> Pod { path "pods/{name}"; }

                /// `listCoreV1NamespacedPod` with `watch=1`: `GET .../pods?watch=1`.
                GET watch_pods(..WatchOptions) -> stream WatchEvent<Pod> {
                    path "pods";
                    query { "watch" = "1"; }
                    headers { ACCEPT = "application/json"; }
                    response JsonStream;
                }

                /// `listCoreV1NamespacedService`.
                GET services(..ListOptions) -> ServiceList { path "services"; }

                /// `readCoreV1NamespacedService`.
                GET service(name: ObjectName) -> Service { path "services/{name}"; }

                /// `listCoreV1NamespacedConfigMap`.
                GET config_maps(..ListOptions) -> ConfigMapList { path "configmaps"; }

                /// `readCoreV1NamespacedConfigMap`.
                GET config_map(name: ObjectName) -> ConfigMap { path "configmaps/{name}"; }
            }
        }

        /// The `apps/v1` group (`/apis/apps/v1`).
        scope apps_v1 {
            path "apis/apps/v1";

            /// `listAppsV1DeploymentForAllNamespaces`: `GET /apis/apps/v1/deployments`.
            GET deployments(..ListOptions) -> DeploymentList { path "deployments"; }

            /// `listAppsV1DeploymentForAllNamespaces` with `watch=1`.
            GET watch_deployments(..WatchOptions) -> stream WatchEvent<Deployment> {
                path "deployments";
                query { "watch" = "1"; }
                headers { ACCEPT = "application/json"; }
                response JsonStream;
            }

            /// One namespace of `apps/v1`.
            scope namespaced(namespace: NamespaceName) {
                path "namespaces/{namespace}";

                /// `listAppsV1NamespacedDeployment`.
                GET deployments(..ListOptions) -> DeploymentList { path "deployments"; }

                /// `readAppsV1NamespacedDeployment`.
                GET deployment(name: ObjectName) -> Deployment { path "deployments/{name}"; }

                /// `listAppsV1NamespacedDeployment` with `watch=1`.
                GET watch_deployments(..WatchOptions) -> stream WatchEvent<Deployment> {
                    path "deployments";
                    query { "watch" = "1"; }
                    headers { ACCEPT = "application/json"; }
                    response JsonStream;
                }
            }
        }

        /// The `batch/v1` group (`/apis/batch/v1`).
        scope batch_v1 {
            path "apis/batch/v1";

            /// `listBatchV1JobForAllNamespaces`: `GET /apis/batch/v1/jobs`.
            GET jobs(..ListOptions) -> JobList { path "jobs"; }

            /// One namespace of `batch/v1`.
            scope namespaced(namespace: NamespaceName) {
                path "namespaces/{namespace}";

                /// `listBatchV1NamespacedJob`.
                GET jobs(..ListOptions) -> JobList { path "jobs"; }

                /// `readBatchV1NamespacedJob`.
                GET job(name: ObjectName) -> Job { path "jobs/{name}"; }
            }
        }

        /// The `networking.k8s.io/v1` group (`/apis/networking.k8s.io/v1`).
        scope networking_v1 {
            path "apis/networking.k8s.io/v1";

            /// `listNetworkingV1IngressForAllNamespaces`: `GET /apis/networking.k8s.io/v1/ingresses`.
            GET ingresses(..ListOptions) -> IngressList { path "ingresses"; }

            /// `listNetworkingV1IngressForAllNamespaces` with `watch=1`.
            GET watch_ingresses(..WatchOptions) -> stream WatchEvent<Ingress> {
                path "ingresses";
                query { "watch" = "1"; }
                headers { ACCEPT = "application/json"; }
                response JsonStream;
            }

            /// One namespace of `networking.k8s.io/v1`.
            scope namespaced(namespace: NamespaceName) {
                path "namespaces/{namespace}";

                /// `listNetworkingV1NamespacedIngress`.
                GET ingresses(..ListOptions) -> IngressList { path "ingresses"; }

                /// `readNetworkingV1NamespacedIngress`.
                GET ingress(name: ObjectName) -> Ingress { path "ingresses/{name}"; }

                /// `listNetworkingV1NamespacedIngress` with `watch=1`.
                GET watch_ingresses(..WatchOptions) -> stream WatchEvent<Ingress> {
                    path "ingresses";
                    query { "watch" = "1"; }
                    headers { ACCEPT = "application/json"; }
                    response JsonStream;
                }
            }
        }
    }
}

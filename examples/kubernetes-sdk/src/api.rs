//! The `concord::api!` declaration: one client, one scope per API group/version, one nested
//! `namespaced(namespace)` scope per group/version.
//!
//! Paths follow the pinned OpenAPI v3 documents (Kubernetes v1.37.2): core/v1 under `api/v1`,
//! named groups under `apis/{group}/{version}`, namespaced collections under
//! `namespaces/{namespace}/{plural}`. Watches use the collection path with `watch=1` (the
//! deprecated `/watch/` path prefix is not declared).

use std::fmt;

use concord::Json;
use concord::reqwest::header::{ACCEPT, AUTHORIZATION, HeaderValue};

use crate::ids::{ContinueToken, NamespaceName, ObjectName, ResourceVersion};
use crate::model::{
    ConfigMap, ConfigMapList, Deployment, DeploymentList, Job, JobList, Namespace, NamespaceList,
    Pod, PodList, Service, ServiceList, VersionInfo,
};
use crate::selector::{FieldSelector, LabelSelector};

/// `resourceVersionMatch` of a list request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceVersionMatch {
    /// `NotOlderThan`: data at least as new as `resourceVersion`.
    NotOlderThan,
    /// `Exact`: data at exactly `resourceVersion` (`410 Gone` when unavailable).
    Exact,
}

impl fmt::Display for ResourceVersionMatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotOlderThan => "NotOlderThan",
            Self::Exact => "Exact",
        })
    }
}

concord::api! {
    /// A read-only slice of the Kubernetes API: get/list of Namespaces, Pods, Services,
    /// ConfigMaps (core/v1), Deployments (apps/v1) and Jobs (batch/v1); watch of Pods and
    /// Deployments; the server version.
    ///
    /// List endpoints share one parameter set (`labelSelector`, `fieldSelector`, `limit`,
    /// `continue`, `resourceVersion`, `resourceVersionMatch`, `timeoutSeconds`); the `continue`
    /// value is `#[sensitive]`. Watch endpoints have no typed output: open them with
    /// [`crate::watch::open`] (or `prepare()?.send_raw()` and a
    /// `concord::jsonl::JsonValuesReader`).
    pub client Kube(
        /// `Authorization: Bearer <token>` (see [`crate::BearerToken::header_value`]). Absent:
        /// no `Authorization` header (an authenticating proxy such as `kubectl proxy`, or a
        /// transport that authenticates with client certificates).
        #[sensitive] header(AUTHORIZATION) authorization?: HeaderValue,
    ) {
        base "https://kubernetes.default.svc";
        format Json;

        /// `GET /version/`: the server's build information.
        GET version() -> VersionInfo { path "version/"; }

        /// The core group, `core/v1` (`/api/v1`).
        scope core_v1 {
            path "api/v1";

            /// `listCoreV1Namespace`: `GET /api/v1/namespaces`.
            GET namespaces(
                query("labelSelector") label_selector?: LabelSelector,
                query("fieldSelector") field_selector?: FieldSelector,
                query limit?: u32,
                #[sensitive] query("continue") continue_token?: ContinueToken,
                query("resourceVersion") resource_version?: ResourceVersion,
                query("resourceVersionMatch") resource_version_match?: ResourceVersionMatch,
                query("timeoutSeconds") timeout_seconds?: u32,
            ) -> NamespaceList { path "namespaces"; }

            /// `readCoreV1Namespace`: `GET /api/v1/namespaces/{name}`.
            GET namespace(name: NamespaceName) -> Namespace { path "namespaces/{name}"; }

            /// `listCoreV1PodForAllNamespaces`: `GET /api/v1/pods`.
            GET pods(
                query("labelSelector") label_selector?: LabelSelector,
                query("fieldSelector") field_selector?: FieldSelector,
                query limit?: u32,
                #[sensitive] query("continue") continue_token?: ContinueToken,
                query("resourceVersion") resource_version?: ResourceVersion,
                query("resourceVersionMatch") resource_version_match?: ResourceVersionMatch,
                query("timeoutSeconds") timeout_seconds?: u32,
            ) -> PodList { path "pods"; }

            /// `listCoreV1PodForAllNamespaces` with `watch=1`: `GET /api/v1/pods?watch=1`. A stream
            /// of consecutive JSON watch events; no typed output.
            GET watch_pods(
                query("labelSelector") label_selector?: LabelSelector,
                query("fieldSelector") field_selector?: FieldSelector,
                query("resourceVersion") resource_version?: ResourceVersion,
                query("allowWatchBookmarks") allow_watch_bookmarks?: bool = true,
                query("timeoutSeconds") timeout_seconds?: u32,
            ) {
                path "pods";
                query { "watch" = "1"; }
                headers { ACCEPT = "application/json"; }
            }

            /// `listCoreV1ServiceForAllNamespaces`: `GET /api/v1/services`.
            GET services(
                query("labelSelector") label_selector?: LabelSelector,
                query("fieldSelector") field_selector?: FieldSelector,
                query limit?: u32,
                #[sensitive] query("continue") continue_token?: ContinueToken,
                query("resourceVersion") resource_version?: ResourceVersion,
                query("resourceVersionMatch") resource_version_match?: ResourceVersionMatch,
                query("timeoutSeconds") timeout_seconds?: u32,
            ) -> ServiceList { path "services"; }

            /// `listCoreV1ConfigMapForAllNamespaces`: `GET /api/v1/configmaps`.
            GET config_maps(
                query("labelSelector") label_selector?: LabelSelector,
                query("fieldSelector") field_selector?: FieldSelector,
                query limit?: u32,
                #[sensitive] query("continue") continue_token?: ContinueToken,
                query("resourceVersion") resource_version?: ResourceVersion,
                query("resourceVersionMatch") resource_version_match?: ResourceVersionMatch,
                query("timeoutSeconds") timeout_seconds?: u32,
            ) -> ConfigMapList { path "configmaps"; }

            /// One namespace of the core group (`/api/v1/namespaces/{namespace}`).
            scope namespaced(namespace: NamespaceName) {
                path "namespaces/{namespace}";

                /// `listCoreV1NamespacedPod`.
                GET pods(
                    query("labelSelector") label_selector?: LabelSelector,
                    query("fieldSelector") field_selector?: FieldSelector,
                    query limit?: u32,
                    #[sensitive] query("continue") continue_token?: ContinueToken,
                    query("resourceVersion") resource_version?: ResourceVersion,
                    query("resourceVersionMatch") resource_version_match?: ResourceVersionMatch,
                    query("timeoutSeconds") timeout_seconds?: u32,
                ) -> PodList { path "pods"; }

                /// `readCoreV1NamespacedPod`.
                GET pod(name: ObjectName) -> Pod { path "pods/{name}"; }

                /// `listCoreV1NamespacedPod` with `watch=1`: `GET .../pods?watch=1`.
                GET watch_pods(
                    query("labelSelector") label_selector?: LabelSelector,
                    query("fieldSelector") field_selector?: FieldSelector,
                    query("resourceVersion") resource_version?: ResourceVersion,
                    query("allowWatchBookmarks") allow_watch_bookmarks?: bool = true,
                    query("timeoutSeconds") timeout_seconds?: u32,
                ) {
                    path "pods";
                    query { "watch" = "1"; }
                    headers { ACCEPT = "application/json"; }
                }

                /// `listCoreV1NamespacedService`.
                GET services(
                    query("labelSelector") label_selector?: LabelSelector,
                    query("fieldSelector") field_selector?: FieldSelector,
                    query limit?: u32,
                    #[sensitive] query("continue") continue_token?: ContinueToken,
                    query("resourceVersion") resource_version?: ResourceVersion,
                    query("resourceVersionMatch") resource_version_match?: ResourceVersionMatch,
                    query("timeoutSeconds") timeout_seconds?: u32,
                ) -> ServiceList { path "services"; }

                /// `readCoreV1NamespacedService`.
                GET service(name: ObjectName) -> Service { path "services/{name}"; }

                /// `listCoreV1NamespacedConfigMap`.
                GET config_maps(
                    query("labelSelector") label_selector?: LabelSelector,
                    query("fieldSelector") field_selector?: FieldSelector,
                    query limit?: u32,
                    #[sensitive] query("continue") continue_token?: ContinueToken,
                    query("resourceVersion") resource_version?: ResourceVersion,
                    query("resourceVersionMatch") resource_version_match?: ResourceVersionMatch,
                    query("timeoutSeconds") timeout_seconds?: u32,
                ) -> ConfigMapList { path "configmaps"; }

                /// `readCoreV1NamespacedConfigMap`.
                GET config_map(name: ObjectName) -> ConfigMap { path "configmaps/{name}"; }
            }
        }

        /// The `apps/v1` group (`/apis/apps/v1`).
        scope apps_v1 {
            path "apis/apps/v1";

            /// `listAppsV1DeploymentForAllNamespaces`: `GET /apis/apps/v1/deployments`.
            GET deployments(
                query("labelSelector") label_selector?: LabelSelector,
                query("fieldSelector") field_selector?: FieldSelector,
                query limit?: u32,
                #[sensitive] query("continue") continue_token?: ContinueToken,
                query("resourceVersion") resource_version?: ResourceVersion,
                query("resourceVersionMatch") resource_version_match?: ResourceVersionMatch,
                query("timeoutSeconds") timeout_seconds?: u32,
            ) -> DeploymentList { path "deployments"; }

            /// `listAppsV1DeploymentForAllNamespaces` with `watch=1`.
            GET watch_deployments(
                query("labelSelector") label_selector?: LabelSelector,
                query("fieldSelector") field_selector?: FieldSelector,
                query("resourceVersion") resource_version?: ResourceVersion,
                query("allowWatchBookmarks") allow_watch_bookmarks?: bool = true,
                query("timeoutSeconds") timeout_seconds?: u32,
            ) {
                path "deployments";
                query { "watch" = "1"; }
                headers { ACCEPT = "application/json"; }
            }

            /// One namespace of `apps/v1`.
            scope namespaced(namespace: NamespaceName) {
                path "namespaces/{namespace}";

                /// `listAppsV1NamespacedDeployment`.
                GET deployments(
                    query("labelSelector") label_selector?: LabelSelector,
                    query("fieldSelector") field_selector?: FieldSelector,
                    query limit?: u32,
                    #[sensitive] query("continue") continue_token?: ContinueToken,
                    query("resourceVersion") resource_version?: ResourceVersion,
                    query("resourceVersionMatch") resource_version_match?: ResourceVersionMatch,
                    query("timeoutSeconds") timeout_seconds?: u32,
                ) -> DeploymentList { path "deployments"; }

                /// `readAppsV1NamespacedDeployment`.
                GET deployment(name: ObjectName) -> Deployment { path "deployments/{name}"; }

                /// `listAppsV1NamespacedDeployment` with `watch=1`.
                GET watch_deployments(
                    query("labelSelector") label_selector?: LabelSelector,
                    query("fieldSelector") field_selector?: FieldSelector,
                    query("resourceVersion") resource_version?: ResourceVersion,
                    query("allowWatchBookmarks") allow_watch_bookmarks?: bool = true,
                    query("timeoutSeconds") timeout_seconds?: u32,
                ) {
                    path "deployments";
                    query { "watch" = "1"; }
                    headers { ACCEPT = "application/json"; }
                }
            }
        }

        /// The `batch/v1` group (`/apis/batch/v1`).
        scope batch_v1 {
            path "apis/batch/v1";

            /// `listBatchV1JobForAllNamespaces`: `GET /apis/batch/v1/jobs`.
            GET jobs(
                query("labelSelector") label_selector?: LabelSelector,
                query("fieldSelector") field_selector?: FieldSelector,
                query limit?: u32,
                #[sensitive] query("continue") continue_token?: ContinueToken,
                query("resourceVersion") resource_version?: ResourceVersion,
                query("resourceVersionMatch") resource_version_match?: ResourceVersionMatch,
                query("timeoutSeconds") timeout_seconds?: u32,
            ) -> JobList { path "jobs"; }

            /// One namespace of `batch/v1`.
            scope namespaced(namespace: NamespaceName) {
                path "namespaces/{namespace}";

                /// `listBatchV1NamespacedJob`.
                GET jobs(
                    query("labelSelector") label_selector?: LabelSelector,
                    query("fieldSelector") field_selector?: FieldSelector,
                    query limit?: u32,
                    #[sensitive] query("continue") continue_token?: ContinueToken,
                    query("resourceVersion") resource_version?: ResourceVersion,
                    query("resourceVersionMatch") resource_version_match?: ResourceVersionMatch,
                    query("timeoutSeconds") timeout_seconds?: u32,
                ) -> JobList { path "jobs"; }

                /// `readBatchV1NamespacedJob`.
                GET job(name: ObjectName) -> Job { path "jobs/{name}"; }
            }
        }
    }
}

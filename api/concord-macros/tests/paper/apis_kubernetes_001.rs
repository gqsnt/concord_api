concord::api! {
    pub client Kubernetes(cluster: Base, token: Secret) {
        base cluster;
        headers { "authorization" = "Bearer {token}"; "accept" = "application/json"; }
        read Json;
        problem Status;
        /// Reports the API server's Warning headers, including on each page and try.
        middleware KubeWarnings;

        part selectors(label_selector?: String, field_selector?: String) {
            query { "labelSelector" = label_selector; "fieldSelector" = field_selector; }
        }
        part page(limit?: u32, continuation?: String,
                  resource_version?: String, resource_version_match?: String) {
            query {
                limit; "continue" = continuation;
                "resourceVersion" = resource_version; "resourceVersionMatch" = resource_version_match;
            }
            paginate move |call, answer| answer.value().next_token().map(|next| {
                call.continuation(next).resource_version(None::<String>)
                    .resource_version_match(None::<String>)
            });
        }
        part watching(resource_version?: String, allow_watch_bookmarks?: bool, timeout_seconds?: u32) {
            query {
                "watch" = true; "resourceVersion" = resource_version;
                "allowWatchBookmarks" = allow_watch_bookmarks; "timeoutSeconds" = timeout_seconds;
            }
            read Stream;
        }
        part writing(dry_run?: String) {
            query { "dryRun" = dry_run; }
        }

        scope pods {
            path "api/v1/pods";
            GET list(..selectors, ..page) -> KubeList<Pod>;
            /// One connection; EOF and an ERROR record require an application decision.
            GET watch(..selectors, ..watching) -> ByteStream;
        }
        scope namespaces(namespace: String) {
            path "api/v1/namespaces/{namespace}";
            scope pods {
                path "pods";
                GET list(..selectors, ..page) -> KubeList<Pod>;
                GET get(name: String, resource_version?: String) -> Pod {
                    path "{name}";
                    query { "resourceVersion" = resource_version; }
                }
                POST create(pod: Pod, ..writing) -> Pod { body json(pod); }
                PUT replace(name: String, pod: Pod, ..writing) -> Pod {
                    path "{name}";
                    body json(pod);
                }
                /// Success acknowledges deletion; finalizers may keep the Pod alive.
                DELETE delete(name: String, options: DeleteOptions, ..writing) {
                    path "{name}";
                    body json(options);
                    idempotent false;
                }
                PATCH patch_json(name: String, patch: Vec<Value>, ..writing) -> Pod {
                    path "{name}";
                    headers { "content-type" = "application/json-patch+json"; }
                    body json(patch);
                }
                PATCH patch_merge(name: String, patch: Value, ..writing) -> Pod {
                    path "{name}";
                    headers { "content-type" = "application/merge-patch+json"; }
                    body json(patch);
                }
                PATCH patch_strategic(name: String, patch: Value, ..writing) -> Pod {
                    path "{name}";
                    headers { "content-type" = "application/strategic-merge-patch+json"; }
                    body json(patch);
                }
                GET watch(..selectors, ..watching) -> ByteStream;
                GET log(name: String, container?: String, follow?: bool,
                        previous?: bool, tail_lines?: i64, timestamps?: bool) -> ByteStream {
                    path "{name}/log";
                    query { container; follow; previous; "tailLines" = tail_lines; timestamps; }
                    headers { override "accept" = "text/plain"; }
                    read Stream;
                }
            }
        }
        scope deployments(namespace: String, name: String) {
            path "apis/apps/v1/namespaces/{namespace}/deployments/{name}";
            GET status() -> Deployment { path "status"; }
            GET scale() -> Scale { path "scale"; }
        }
        /// Requires the namespaced CronTab CRD from the Kubernetes documentation.
        GET get_crontab(namespace: String, name: String) -> CronTab {
            path "apis/stable.example.com/v1/namespaces/{namespace}/crontabs/{name}";
        }
    }
}

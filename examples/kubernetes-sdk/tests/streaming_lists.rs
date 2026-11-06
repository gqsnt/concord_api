//! Streaming-list queries captured on loopback, using synthetic initial events.
//! Contract: https://kubernetes.io/docs/reference/using-api/api-concepts/#streaming-lists

use std::time::Duration;

use concord::jsonl::JsonValuesReader;
use concord::reqwest;
use concord_example_kubernetes_sdk::watch::{self, WatchEvent, WatchOperation, WatchStream};
use concord_example_kubernetes_sdk::{
    KubeContext, NamespaceName, Resource, ResourceVersion, ResourceVersionMatch,
};
use concord_example_testserver::{Reply, TestServer};

async fn server() -> TestServer {
    TestServer::start(|request| {
        let (kind, api_version) = if request.path().ends_with("/pods") {
            ("Pod", "v1")
        } else if request.path().ends_with("/deployments") {
            ("Deployment", "apps/v1")
        } else {
            ("Ingress", "networking.k8s.io/v1")
        };
        let added = serde_json::json!({
            "type": "ADDED",
            "object": {"kind": kind, "apiVersion": api_version,
                "metadata": {"name": "existing", "resourceVersion": "40"}}
        });
        let bookmark = serde_json::json!({
            "type": "BOOKMARK",
            "object": {"kind": kind, "apiVersion": api_version,
                "metadata": {"resourceVersion": "42",
                    "annotations": {"k8s.io/initial-events-end": "true"}}}
        });
        Reply::json(200, format!("{added}\n{bookmark}\n"))
    })
    .await
}

fn kube(server: &TestServer) -> KubeContext {
    let http = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    KubeContext::new(http).with_base_url(server.url()).unwrap()
}

async fn check_initial_events<T: Resource>(mut events: WatchStream<T>) {
    let added = events.next().await.unwrap().unwrap();
    assert!(matches!(added, WatchEvent::Added(_)));
    assert_eq!(added.resource_version().unwrap().as_str(), "40");
    let bookmark = events.next().await.unwrap().unwrap();
    assert_eq!(bookmark.resource_version().unwrap().as_str(), "42");
    assert!(matches!(bookmark, WatchEvent::Bookmark(b) if b.is_initial_events_end()));
    assert!(events.next().await.is_none());
}

fn check_query(server: &TestServer, index: usize, path: &str, version: &str) {
    let requests = server.requests();
    let request = &requests[index];
    assert_eq!(request.method, "GET");
    assert_eq!(
        request.target,
        format!(
            "{path}?{version}resourceVersionMatch=NotOlderThan&sendInitialEvents=true\
             &allowWatchBookmarks=true&watch=1"
        )
    );
    assert_eq!(request.header("accept"), Some("application/json"));
}

#[tokio::test]
async fn pod_builder_sends_streaming_list_query() {
    let server = server().await;
    let events = kube(&server)
        .core()
        .watch_pods()
        .set_send_initial_events(true)
        .set_resource_version_match(ResourceVersionMatch::NotOlderThan)
        .send()
        .await
        .unwrap();
    check_initial_events(events).await;
    assert_eq!(server.requests().len(), 1);
    check_query(&server, 0, "/api/v1/pods", "");
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn namespaced_deployment_builder_sends_streaming_list_query() {
    let server = server().await;
    let events = kube(&server)
        .namespace(NamespaceName::new("team-a").unwrap())
        .apps()
        .watch_deployments()
        .set_resource_version(ResourceVersion::new("30"))
        .set_resource_version_match(ResourceVersionMatch::NotOlderThan)
        .set_send_initial_events(true)
        .set_allow_watch_bookmarks(true)
        .send()
        .await
        .unwrap();
    check_initial_events(events).await;
    assert_eq!(server.requests().len(), 1);
    check_query(
        &server,
        0,
        "/apis/apps/v1/namespaces/team-a/deployments",
        "resourceVersion=30&",
    );
    assert_eq!(server.shutdown().await.open_after, 0);
}

async fn generic_watch<W: WatchOperation>(operation: W) {
    let operation = operation
        .with_send_initial_events(Some(true))
        .with_resource_version_match(Some(ResourceVersionMatch::NotOlderThan))
        .with_bookmarks(Some(true));
    let events = watch::open(operation, &JsonValuesReader::default())
        .await
        .unwrap();
    check_initial_events(events).await;
}

#[tokio::test]
async fn generic_adapter_supports_streaming_lists_on_every_watch_endpoint() {
    let server = server().await;
    let kube = kube(&server);
    let team = kube.namespace(NamespaceName::new("team-a").unwrap());

    generic_watch(kube.core().watch_pods()).await;
    generic_watch(team.core().watch_pods()).await;
    generic_watch(kube.apps().watch_deployments()).await;
    generic_watch(team.apps().watch_deployments()).await;
    generic_watch(kube.networking().watch_ingresses()).await;
    generic_watch(team.networking().watch_ingresses()).await;

    let paths = [
        "/api/v1/pods",
        "/api/v1/namespaces/team-a/pods",
        "/apis/apps/v1/deployments",
        "/apis/apps/v1/namespaces/team-a/deployments",
        "/apis/networking.k8s.io/v1/ingresses",
        "/apis/networking.k8s.io/v1/namespaces/team-a/ingresses",
    ];
    assert_eq!(server.requests().len(), paths.len());
    for (index, path) in paths.into_iter().enumerate() {
        check_query(&server, index, path, "");
    }
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[test]
fn streaming_list_options_can_be_disabled_and_removed() {
    let operation = KubeContext::new(reqwest::Client::new())
        .core()
        .watch_pods()
        .set_send_initial_events(false)
        .set_resource_version_match(ResourceVersionMatch::NotOlderThan);
    assert_eq!(
        operation.clone().prepare().unwrap().request().url().query(),
        Some(
            "resourceVersionMatch=NotOlderThan&sendInitialEvents=false&allowWatchBookmarks=true&watch=1"
        )
    );
    let cleared = operation
        .with_send_initial_events(None)
        .with_resource_version_match(None)
        .prepare_watch()
        .unwrap();
    assert_eq!(
        cleared.request().url().query(),
        Some("allowWatchBookmarks=true&watch=1")
    );
}

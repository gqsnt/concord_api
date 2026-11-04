//! The frozen initial consumer (`src/before.rs`) against the loopback server: exact requests on
//! the wire and the typed results. Not edited after stage 1.
//!
//! Fixtures (`tests/fixtures/`, schema-derived synthetic: field names from the pinned Kubernetes
//! v1.37.2 OpenAPI v3 documents `apis__apps__v1_openapi.json` / `api__v1_openapi.json`; chunking
//! and watch semantics from <https://kubernetes.io/docs/reference/using-api/api-concepts/>):
//! `deployments_team_a_{1,2}.json` (two chunks of one snapshot, `5100`), `deployments_team_b.json`,
//! `services_team_{a,b}.json`, `watch_deployments_team_a.json-values` (`MODIFIED`, `BOOKMARK`,
//! `DELETED`, then the end of the body).

mod support;

use std::ops::ControlFlow;

use concord_check_maintenance_kubernetes::before::{
    AuditRequest, FollowEnd, FollowLimits, Limits, audit, follow_deployments,
};
use concord_example_kubernetes_sdk::watch::WatchEvent;
use concord_example_kubernetes_sdk::{ObjectName, ResourceVersion};
use concord_example_testserver::{Recorded, TestServer};
use support::*;

const SHOP: &str = "labelSelector=app.kubernetes.io%2Fpart-of%3Dshop";

fn respond(r: &Recorded) -> concord_example_testserver::Reply {
    let continued = query(r, "continue");
    match (r.path(), continued.as_deref()) {
        ("/apis/apps/v1/namespaces/team-a/deployments", None) if query(r, "watch").is_some() => {
            watch_events("watch_deployments_team_a.json-values")
        }
        ("/apis/apps/v1/namespaces/team-a/deployments", None) => json("deployments_team_a_1.json"),
        ("/apis/apps/v1/namespaces/team-a/deployments", Some("synthetic-continue-team-a-1")) => {
            json("deployments_team_a_2.json")
        }
        ("/apis/apps/v1/namespaces/team-b/deployments", None) => json("deployments_team_b.json"),
        ("/api/v1/namespaces/team-a/services", None) => json("services_team_a.json"),
        ("/api/v1/namespaces/team-b/services", None) => json("services_team_b.json"),
        _ => not_found(),
    }
}

fn names<T>(items: &[T], name: impl Fn(&T) -> Option<&ObjectName>) -> Vec<&str> {
    items
        .iter()
        .map(|i| name(i).map(ObjectName::as_str).unwrap_or("?"))
        .collect()
}

#[tokio::test]
async fn frozen_audit_reads_two_namespaces_in_consistent_chunks() {
    let server = TestServer::start(respond).await;
    let request = AuditRequest {
        app: "shop".into(),
        namespaces: vec![ns("team-a"), ns("team-b")],
        limits: Limits {
            chunk_size: nz32(2),
            max_chunks: nz(4),
        },
    };
    let audit = audit(&kube(&server), &request).await.unwrap();

    let requests = server.requests();
    assert_eq!(
        targets(&requests),
        [
            format!("/apis/apps/v1/namespaces/team-a/deployments?{SHOP}&limit=2"),
            format!(
                "/apis/apps/v1/namespaces/team-a/deployments?{SHOP}&limit=2\
                 &continue=synthetic-continue-team-a-1"
            ),
            format!("/api/v1/namespaces/team-a/services?{SHOP}&limit=2"),
            format!("/apis/apps/v1/namespaces/team-b/deployments?{SHOP}&limit=2"),
            format!("/api/v1/namespaces/team-b/services?{SHOP}&limit=2"),
        ]
    );
    assert_read_only_and_authenticated(&requests);

    assert_eq!(audit.selector.to_string(), "app.kubernetes.io/part-of=shop");
    let [a, b] = &audit.namespaces[..] else {
        panic!("two namespaces: {audit:?}")
    };
    assert_eq!(a.namespace.as_str(), "team-a");
    assert_eq!(
        names(&a.deployments.items, |d| d.metadata.name.as_ref()),
        ["storefront", "cart", "search"]
    );
    assert_eq!(a.deployments.snapshot, ResourceVersion::new("5100"));
    assert_eq!(a.deployments.chunks, 2);
    assert_eq!(
        names(&a.services.items, |s| s.metadata.name.as_ref()),
        ["storefront", "cart"]
    );
    assert_eq!(
        a.unavailable()
            .iter()
            .map(|n| n.as_str())
            .collect::<Vec<_>>(),
        ["cart"]
    );
    assert_eq!(b.namespace.as_str(), "team-b");
    assert_eq!(
        names(&b.deployments.items, |d| d.metadata.name.as_ref()),
        ["storefront"]
    );
    assert_eq!(b.deployments.snapshot, ResourceVersion::new("6100"));
    assert_eq!(
        b.deployments.items[0].metadata.namespace.as_ref().unwrap(),
        &ns("team-b")
    );
    assert!(!audit.ready());

    finish(server).await;
}

#[tokio::test]
async fn frozen_follow_delivers_deployment_events_from_the_audited_snapshot() {
    let server = TestServer::start(respond).await;
    let kube = kube(&server);
    let team_a = kube.namespace(ns("team-a"));
    let selector = concord_check_maintenance_kubernetes::before::app_selector("shop").unwrap();
    let mut seen = Vec::new();
    let report = follow_deployments(
        &team_a,
        &selector,
        ResourceVersion::new("5100"),
        FollowLimits {
            server_timeout_seconds: 60,
            max_events: 10,
            max_event_bytes: 64 * 1024,
        },
        |event| {
            let name = match event {
                WatchEvent::Added(d) | WatchEvent::Modified(d) | WatchEvent::Deleted(d) => d
                    .metadata
                    .name
                    .as_ref()
                    .map(|n| n.as_str().to_owned())
                    .unwrap_or_default(),
                _ => String::new(),
            };
            seen.push((event.type_name(), name));
            ControlFlow::Continue(())
        },
    )
    .await
    .unwrap();

    let requests = server.requests();
    assert_eq!(
        targets(&requests),
        [format!(
            "/apis/apps/v1/namespaces/team-a/deployments?{SHOP}&resourceVersion=5100\
             &allowWatchBookmarks=true&timeoutSeconds=60&watch=1"
        )]
    );
    assert_read_only_and_authenticated(&requests);
    assert_eq!(
        seen,
        [
            ("MODIFIED", "cart".to_owned()),
            ("BOOKMARK", String::new()),
            ("DELETED", "search".to_owned()),
        ]
    );
    assert_eq!(report.changes, 2);
    assert_eq!(report.bookmarks, 1);
    assert_eq!(report.last, ResourceVersion::new("5115"));
    assert!(matches!(report.end, FollowEnd::ServerClosed), "{report:?}");

    finish(server).await;
}

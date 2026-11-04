//! The updated consumer (`src/after.rs`) against the loopback server: the changed selector on
//! the wire for every list and the watch, the new Ingress family read through the published
//! context with the frozen consumer's generic helpers, and the new family's typed errors.
//!
//! Fixtures (`tests/fixtures/release_*`, `watch_ingresses_team_a.json-values`,
//! `status_403_ingresses_team_b.json`; schema-derived synthetic: field names from the pinned
//! v1.37.2 OpenAPI v3 documents, `apis__networking.k8s.io__v1_openapi.json` for Ingresses). They
//! stand for what a server returns for the new selector: objects of both applications, no
//! canaries (the test server does not evaluate selectors; the test checks every returned object
//! against the selector with the SDK's `LabelSelector::matches`). The `403` mirrors the shape of
//! an RBAC refusal (`Status` reason `Forbidden`, `details.group`).

mod support;

use std::collections::BTreeMap;
use std::ops::ControlFlow;

use concord_check_maintenance_kubernetes::after::{
    BackendProblem, DanglingBackend, ReleaseRequest, follow_ingresses, release_audit,
    release_selector,
};
use concord_check_maintenance_kubernetes::before::{
    AuditError, CollectError, FollowEnd, FollowLimits, Limits,
};
use concord_example_kubernetes_sdk::list::ListError;
use concord_example_kubernetes_sdk::watch::WatchEvent;
use concord_example_kubernetes_sdk::{ObjectName, ResourceVersion};
use concord_example_testserver::{Recorded, Reply, TestServer};
use support::*;

/// `app.kubernetes.io/part-of in (checkout,shop),!example.com/canary`, as Concord encodes it.
const RELEASE: &str =
    "labelSelector=app.kubernetes.io%2Fpart-of+in+%28checkout%2Cshop%29%2C%21example.com%2Fcanary";

fn respond(r: &Recorded) -> Reply {
    let continued = query(r, "continue");
    let watching = query(r, "watch").is_some();
    match (r.path(), continued.as_deref(), watching) {
        ("/apis/apps/v1/namespaces/team-a/deployments", None, false) => {
            json("release_deployments_team_a.json")
        }
        ("/apis/apps/v1/namespaces/team-b/deployments", None, false) => {
            json("release_deployments_team_b.json")
        }
        ("/api/v1/namespaces/team-a/services", None, false) => json("release_services_team_a.json"),
        ("/api/v1/namespaces/team-b/services", None, false) => json("release_services_team_b.json"),
        ("/apis/networking.k8s.io/v1/namespaces/team-a/ingresses", None, false) => {
            json("release_ingresses_team_a_1.json")
        }
        (
            "/apis/networking.k8s.io/v1/namespaces/team-a/ingresses",
            Some("synthetic-continue-ingresses-team-a-1"),
            false,
        ) => json("release_ingresses_team_a_2.json"),
        ("/apis/networking.k8s.io/v1/namespaces/team-b/ingresses", None, false) => {
            json("release_ingresses_team_b.json")
        }
        ("/apis/networking.k8s.io/v1/namespaces/team-a/ingresses", None, true) => {
            watch_events("watch_ingresses_team_a.json-values")
        }
        _ => not_found(),
    }
}

fn request() -> ReleaseRequest {
    ReleaseRequest {
        apps: vec!["shop".into(), "checkout".into()],
        namespaces: vec![ns("team-a"), ns("team-b")],
        limits: Limits {
            chunk_size: nz32(2),
            max_chunks: nz(4),
        },
    }
}

fn names<'a>(names: impl IntoIterator<Item = &'a ObjectName>) -> Vec<&'a str> {
    names.into_iter().map(ObjectName::as_str).collect()
}

fn labels(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

#[test]
fn the_selector_changed_from_equality_to_set_based() {
    let selector = release_selector(["shop", "checkout"]).unwrap();
    assert_eq!(
        selector.to_string(),
        "app.kubernetes.io/part-of in (checkout,shop),!example.com/canary"
    );
    let part_of = "app.kubernetes.io/part-of";
    assert!(selector.matches(&labels(&[(part_of, "checkout")])));
    assert!(!selector.matches(&labels(&[(part_of, "shop"), ("example.com/canary", "")])));
    assert!(!selector.matches(&labels(&[(part_of, "billing")])));
    assert!(release_selector([]).is_err());
}

#[tokio::test]
async fn updated_audit_reads_ingresses_through_the_context_with_the_new_selector() {
    let server = TestServer::start(respond).await;
    let release = release_audit(&kube(&server), &request()).await.unwrap();

    let requests = server.requests();
    assert_eq!(
        targets(&requests),
        [
            format!("/apis/apps/v1/namespaces/team-a/deployments?{RELEASE}&limit=2"),
            format!("/api/v1/namespaces/team-a/services?{RELEASE}&limit=2"),
            format!("/apis/networking.k8s.io/v1/namespaces/team-a/ingresses?{RELEASE}&limit=2"),
            format!(
                "/apis/networking.k8s.io/v1/namespaces/team-a/ingresses?{RELEASE}&limit=2\
                 &continue=synthetic-continue-ingresses-team-a-1"
            ),
            format!("/apis/apps/v1/namespaces/team-b/deployments?{RELEASE}&limit=2"),
            format!("/api/v1/namespaces/team-b/services?{RELEASE}&limit=2"),
            format!("/apis/networking.k8s.io/v1/namespaces/team-b/ingresses?{RELEASE}&limit=2"),
        ]
    );
    assert_read_only_and_authenticated(&requests);

    let [a, b] = &release.namespaces[..] else {
        panic!("two namespaces: {release:?}")
    };
    // Every object the fixtures return satisfies the selector that was sent.
    for n in [a, b] {
        let metas = n
            .deployments
            .items
            .iter()
            .map(|d| &d.metadata)
            .chain(n.services.items.iter().map(|s| &s.metadata))
            .chain(n.ingresses.items.iter().map(|i| &i.metadata));
        for meta in metas {
            assert!(release.selector.matches(&meta.labels), "{meta:?}");
            assert_eq!(meta.namespace.as_ref(), Some(&n.namespace));
        }
    }

    assert_eq!(
        names(
            a.deployments
                .items
                .iter()
                .filter_map(|d| d.metadata.name.as_ref())
        ),
        ["storefront", "cart", "checkout"]
    );
    assert_eq!(names(a.unavailable()), ["checkout"]);
    // The Ingress chain: two chunks of one snapshot, through the SDK's unchanged `ListPager`.
    assert_eq!(a.ingresses.chunks, 2);
    assert_eq!(a.ingresses.snapshot, ResourceVersion::new("5202"));
    assert_eq!(
        names(
            a.ingresses
                .items
                .iter()
                .filter_map(|i| i.metadata.name.as_ref())
        ),
        ["storefront", "checkout"]
    );
    let storefront = ObjectName::new("storefront").unwrap();
    assert_eq!(
        a.dangling_backends(),
        [
            DanglingBackend {
                ingress: storefront.clone(),
                service: "cart".into(),
                problem: BackendProblem::NoSuchPortNumber(8080),
            },
            DanglingBackend {
                ingress: storefront,
                service: "search".into(),
                problem: BackendProblem::NoSuchService,
            },
        ]
    );
    assert_eq!(names(a.unpublished()), ["checkout"]);

    assert_eq!(b.ingresses.snapshot, ResourceVersion::new("6202"));
    assert!(b.dangling_backends().is_empty());
    assert!(b.unpublished().is_empty());
    assert!(b.unavailable().is_empty());
    assert!(!release.ready());

    finish(server).await;
}

#[tokio::test]
async fn updated_follow_watches_ingresses_with_the_new_selector() {
    let server = TestServer::start(respond).await;
    let kube = kube(&server);
    let selector = release_selector(["shop", "checkout"]).unwrap();
    let mut seen = Vec::new();
    let report = follow_ingresses(
        &kube.namespace(ns("team-a")),
        &selector,
        ResourceVersion::new("5202"),
        FollowLimits {
            server_timeout_seconds: 60,
            max_events: 10,
            max_event_bytes: 64 * 1024,
        },
        |event| {
            let detail = match event {
                WatchEvent::Added(i) | WatchEvent::Modified(i) | WatchEvent::Deleted(i) => {
                    format!(
                        "{}:{}",
                        i.metadata.name.as_ref().map(|n| n.as_str()).unwrap_or("?"),
                        i.has_address()
                    )
                }
                _ => String::new(),
            };
            seen.push((event.type_name(), detail));
            ControlFlow::Continue(())
        },
    )
    .await
    .unwrap();

    let requests = server.requests();
    assert_eq!(
        targets(&requests),
        [format!(
            "/apis/networking.k8s.io/v1/namespaces/team-a/ingresses?{RELEASE}\
             &resourceVersion=5202&allowWatchBookmarks=true&timeoutSeconds=60&watch=1"
        )]
    );
    assert_read_only_and_authenticated(&requests);
    assert_eq!(
        seen,
        [
            ("ADDED", "promo:false".to_owned()),
            ("MODIFIED", "checkout:true".to_owned()),
            ("BOOKMARK", String::new()),
        ]
    );
    assert_eq!((report.changes, report.bookmarks), (2, 1));
    assert_eq!(report.last, ResourceVersion::new("5220"));
    assert!(matches!(report.end, FollowEnd::Expired), "{report:?}");

    finish(server).await;
}

#[tokio::test]
async fn a_forbidden_ingress_list_is_a_typed_error_naming_the_new_endpoint() {
    // RBAC that grants workloads but not `networking.k8s.io`: the first two families of team-b
    // succeed, its Ingress list is refused.
    let server = TestServer::start(|r: &Recorded| {
        if r.path() == "/apis/networking.k8s.io/v1/namespaces/team-b/ingresses" {
            Reply::json(403, fixture("status_403_ingresses_team_b.json"))
        } else {
            respond(r)
        }
    })
    .await;
    let error = release_audit(&kube(&server), &request()).await.unwrap_err();

    let AuditError::Collect {
        namespace,
        family,
        error: CollectError::List(ListError::Api(api)),
    } = &error
    else {
        panic!("{error:?}")
    };
    assert_eq!(namespace, &ns("team-b"));
    assert_eq!(*family, "ingresses");
    assert_eq!(api.http_status().as_u16(), 403);
    assert_eq!(api.reason(), Some("Forbidden"));
    assert_eq!(
        api.status()
            .and_then(|s| s.details.as_ref()?.group.as_deref()),
        Some("networking.k8s.io")
    );
    // Concord's generated endpoint name for the new scope (the SDK's hand-written watch labels
    // follow the same `Kube.networking_v1.namespaced.*` pattern).
    assert_eq!(
        api.endpoint(),
        Some("Kube.networking_v1.namespaced.ingresses")
    );
    assert_eq!(
        api.to_string(),
        "Kube.networking_v1.namespaced.ingresses: Kubernetes API answered 403 (reason Forbidden)"
    );
    let shown = format!("{error:?}");
    assert!(
        !shown.contains("system:serviceaccount") && !shown.contains(TOKEN),
        "{shown}"
    );
    assert_eq!(server.requests().len(), 7);

    finish(server).await;
}

//! KU-A — workload inventory, end to end against the loopback server.
//!
//! Fixtures: `tests/fixtures/ku_a/*` (schema-derived synthetic, see `tests/fixtures/MANIFEST.json`).
//! Expectations come from the Kubernetes API concepts page (chunked lists, `remainingItemCount`,
//! `410` for an expired token) and the pinned v1.37.2 OpenAPI parameter descriptions, not from this
//! client.

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use concord_example_kubernetes_app::collect::{
    ListOutcome, OnExpired, RestartBoundary, collect_list,
};
use concord_example_kubernetes_app::inventory::{
    CollectionScope, InventoryRequest, InventoryScope, InventorySummary, inventory,
};
use concord_example_kubernetes_sdk::list::{Continuation, ListError, ListOperation};
use concord_example_kubernetes_sdk::{
    ContinueToken, FieldSelector, LabelSelector, ResourceVersion,
};
use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_pagination::Resume;
use support::*;

const T1: &str = "eyJ2IjoibWV0YS5rOHMuaW8vdjEiLCJydiI6NTAwMSwic3RhcnQiOiJ3ZWItYS0yXHUwMDAwIn0";
const T2: &str = "eyJ2IjoibWV0YS5rOHMuaW8vdjEiLCJydiI6NTAwMSwic3RhcnQiOiJ3ZWItYS05XHUwMDAwIn0";
const ALL1: &str = "Y2x1c3Rlci1wb2RzLTE";
const ALL2: &str = "Y2x1c3Rlci1wb2RzLTI";
const INCONSISTENT: &str = "SU5DT05TSVNURU5ULWNvbnRpbnVlLXRva2Vu";
const RESTARTED: &str = "cmVzdGFydGVkLWNoYWluLTE";

/// The two-namespace cluster: everything except team-a's pods, which each test routes itself.
fn base_route(r: &Recorded) -> Option<Reply> {
    Some(match r.path() {
        "/version/" => json("ku_a/version.json"),
        "/api/v1/namespaces/team-a" => json("ku_a/namespace_team_a.json"),
        "/api/v1/namespaces/team-b" => json("ku_a/namespace_team_b.json"),
        "/api/v1/namespaces" => json("ku_a/namespaces_all.json"),
        "/api/v1/namespaces/team-b/pods" => json("ku_a/pods_team_b.json"),
        "/apis/apps/v1/namespaces/team-a/deployments" => json("ku_a/deployments_team_a.json"),
        "/apis/apps/v1/namespaces/team-b/deployments" => json("ku_a/deployments_team_b.json"),
        "/api/v1/namespaces/team-a/services" => json("ku_a/services_team_a.json"),
        "/api/v1/namespaces/team-b/services" => json("ku_a/services_team_b.json"),
        "/api/v1/namespaces/team-a/configmaps" => json("ku_a/configmaps_team_a.json"),
        "/api/v1/namespaces/team-b/configmaps" => json("ku_a/configmaps_team_b.json"),
        "/apis/batch/v1/namespaces/team-a/jobs" => json("ku_a/jobs_team_a.json"),
        "/apis/batch/v1/namespaces/team-b/jobs" => json("ku_a/jobs_team_b.json"),
        "/apis/apps/v1/deployments" => json("ku_a/deployments_all.json"),
        "/api/v1/services" => json("ku_a/services_all.json"),
        "/api/v1/configmaps" => json("ku_a/configmaps_all.json"),
        "/apis/batch/v1/jobs" => json("ku_a/jobs_all.json"),
        _ => return None,
    })
}

/// team-a's pods in three chunks: two items, an empty chunk with a token, the last item.
fn pods_team_a(r: &Recorded) -> Reply {
    match query(r, "continue").as_deref() {
        None => json("ku_a/pods_team_a_1.json"),
        Some(T1) => json("ku_a/pods_team_a_2_empty.json"),
        Some(T2) => json("ku_a/pods_team_a_3.json"),
        Some(_) => not_found(),
    }
}

fn names<T: concord_example_kubernetes_sdk::Resource>(items: &[T]) -> Vec<String> {
    items
        .iter()
        .map(|i| i.metadata().name.as_ref().unwrap().to_string())
        .collect()
}

fn two_namespaces(
    labels: Option<LabelSelector>,
    fields: Option<FieldSelector>,
) -> InventoryRequest {
    InventoryRequest {
        scope: InventoryScope::Namespaces(vec![ns("team-a"), ns("team-b")]),
        label_selector: labels,
        pod_field_selector: fields,
        list: policy(2, 10, OnExpired::Restart { max_restarts: 1 }),
    }
}

#[tokio::test]
async fn ku_a_two_namespaces_selectors_and_an_empty_chunk() {
    let server = TestServer::start(|r| {
        if r.path() == "/api/v1/namespaces/team-a/pods" {
            return pods_team_a(r);
        }
        base_route(r).unwrap_or_else(not_found)
    })
    .await;
    let labels = LabelSelector::builder()
        .equals("app", "web")
        .not_exists("canary")
        .build()
        .unwrap();
    let fields = FieldSelector::builder()
        .not_equals("status.phase", "Failed")
        .build()
        .unwrap();
    let inv = inventory(&kube(&server), &two_namespaces(Some(labels), Some(fields))).await;

    assert!(inv.is_complete(), "{inv:#?}");
    assert_eq!(inv.server.same_minor(), Some(true));
    assert_eq!(inv.server.observed.as_ref().unwrap().git_version, "v1.37.2");
    assert_eq!(inv.namespaces.len(), 2);
    assert!(inv.namespaces.iter().all(|n| n.result.is_ok()));

    // team-a pods: three chunks of one consistent snapshot; selectors -> no remainingItemCount.
    let pods_a = &inv.pods[0];
    assert_eq!(pods_a.scope, CollectionScope::Namespace(ns("team-a")));
    assert_eq!(names(&pods_a.items), ["web-a-1", "web-a-2", "web-a-3"]);
    assert_eq!(
        pods_a.completion.snapshot,
        Some(ResourceVersion::new("5001"))
    );
    assert_eq!(pods_a.completion.chunks, 3);
    assert_eq!(pods_a.completion.remaining_item_counts, [None, None, None]);
    assert!(pods_a.completion.restarts.is_empty());
    let pods_b = &inv.pods[1];
    assert_eq!(pods_b.scope, CollectionScope::Namespace(ns("team-b")));
    assert_eq!(names(&pods_b.items), ["web-b-1"]);
    assert_eq!(
        pods_b.completion.snapshot,
        Some(ResourceVersion::new("7001"))
    );
    assert_eq!(
        inv.jobs[1].items[0].state(),
        concord_example_kubernetes_sdk::model::JobState::Complete
    );

    assert_eq!(
        inv.summary(),
        InventorySummary {
            pods: 4,
            pods_running: 3,
            pods_pending: 1,
            deployments: 2,
            deployments_unavailable: 1,
            services: 1,
            config_maps: 2,
            jobs: 1,
            jobs_complete: 1,
            jobs_failed: 0,
        }
    );

    // Wire: selectors encoded, continue tokens passed back exactly as issued, no
    // resourceVersion next to a token, every family request scoped to its namespace.
    let requests = server.requests();
    assert_authenticated(&requests);
    let pods = to_path(&requests, "/api/v1/namespaces/team-a/pods");
    assert_eq!(pods.len(), 3);
    for r in &pods {
        assert!(
            r.target.contains("labelSelector=app%3Dweb%2C%21canary"),
            "{}",
            r.target
        );
        assert!(
            r.target.contains("fieldSelector=status.phase%21%3DFailed"),
            "{}",
            r.target
        );
        assert_eq!(query(r, "limit").as_deref(), Some("2"));
        assert_eq!(query(r, "resourceVersion"), None);
        assert_eq!(r.header("accept"), Some("application/json"));
    }
    assert_eq!(query(pods[0], "continue"), None);
    assert_eq!(query(pods[1], "continue").as_deref(), Some(T1));
    assert_eq!(query(pods[2], "continue").as_deref(), Some(T2));
    // The pod field selector is not sent to other families; the label selector is.
    let deployments = to_path(&requests, "/apis/apps/v1/namespaces/team-b/deployments");
    assert_eq!(deployments.len(), 1);
    assert_eq!(query(deployments[0], "fieldSelector"), None);
    assert_eq!(
        query(deployments[0], "labelSelector").as_deref(),
        Some("app=web,!canary")
    );
    // Namespaces are read with GET, never filtered.
    let get_ns = to_path(&requests, "/api/v1/namespaces/team-a");
    assert_eq!(get_ns.len(), 1);
    assert_eq!(get_ns[0].query(), None);
    // Scope: 2 GET namespace + version + (4 + 1 pods requests) + 4 families x 2 namespaces.
    assert_eq!(requests.len(), 1 + 2 + 3 + 1 + 8);
    for r in &requests {
        let team_b = r.path().contains("/namespaces/team-b");
        let team_a = r.path().contains("/namespaces/team-a");
        assert!(!(team_a && team_b), "{}", r.target);
    }
    finish(server).await;
}

#[tokio::test]
async fn ku_a_cluster_wide_reports_remaining_item_count_without_using_it() {
    let server = TestServer::start(|r| {
        if r.path() == "/api/v1/pods" {
            return match query(r, "continue").as_deref() {
                None => json("ku_a/pods_all_1.json"),
                Some(ALL1) => json("ku_a/pods_all_2.json"),
                Some(ALL2) => json("ku_a/pods_all_3.json"),
                Some(_) => not_found(),
            };
        }
        base_route(r).unwrap_or_else(not_found)
    })
    .await;
    let request = InventoryRequest {
        scope: InventoryScope::AllNamespaces,
        label_selector: None,
        pod_field_selector: None,
        list: policy(2, 10, OnExpired::Report),
    };
    let inv = inventory(&kube(&server), &request).await;
    assert!(inv.is_complete(), "{inv:#?}");
    let namespaces = inv.namespace_list.as_ref().unwrap();
    assert_eq!(names(&namespaces.items), ["team-a", "team-b"]);
    let pods = &inv.pods[0];
    assert_eq!(pods.scope, CollectionScope::AllNamespaces);
    assert_eq!(pods.items.len(), 5);
    assert_eq!(pods.completion.snapshot, Some(ResourceVersion::new("8100")));
    // Reported as received (3, then 1, absent on the last chunk) ...
    assert_eq!(
        pods.completion.remaining_item_counts,
        [Some(3), Some(1), None]
    );
    // ... and not used: exactly the chunks the tokens led to, one at a time, only server tokens.
    let requests = server.requests();
    let chain = to_path(&requests, "/api/v1/pods");
    assert_eq!(chain.len(), 3);
    let sent: Vec<_> = chain.iter().map(|r| query(r, "continue")).collect();
    assert_eq!(sent, [None, Some(ALL1.into()), Some(ALL2.into())]);
    assert_eq!(server.peak_active_bodies(), 1);
    assert_eq!(inv.services[0].items.len(), 2);
    assert_eq!(inv.jobs[0].items.len(), 2);
    finish(server).await;
}

/// A long generated chain: 60 chunks of 5 pods with a decreasing `remainingItemCount`. (Kept at
/// 60 because each loopback response costs ~40 ms with the test server's separate head/body writes
/// and no `TCP_NODELAY`; see FRICTION.md.)
#[tokio::test]
async fn ku_a_long_chain_is_strictly_sequential() {
    const CHUNKS: usize = 60;
    const PER: usize = 5;
    let server = TestServer::start(|r| {
        // Opaque server tokens: `g<chunk>`; chunk 0 has no token.
        let chunk = match query(r, "continue") {
            None => 0,
            Some(t) => t.trim_start_matches('g').parse::<usize>().unwrap(),
        };
        let items: Vec<String> = (0..PER)
            .map(|i| {
                let n = chunk * PER + i;
                format!(
                    r#"{{"metadata":{{"name":"pod-{n:05}","namespace":"team-a","resourceVersion":"{n}"}},"status":{{"phase":"Running"}}}}"#
                )
            })
            .collect();
        let remaining = CHUNKS * PER - (chunk + 1) * PER;
        let meta = if chunk + 1 < CHUNKS {
            format!(r#"{{"resourceVersion":"99000","continue":"g{}","remainingItemCount":{remaining}}}"#, chunk + 1)
        } else {
            r#"{"resourceVersion":"99000","continue":""}"#.to_owned()
        };
        Reply::json(
            200,
            format!(r#"{{"kind":"PodList","apiVersion":"v1","metadata":{meta},"items":[{}]}}"#, items.join(",")),
        )
    })
    .await;
    let template = kube(&server).namespace(ns("team-a")).core().pods();
    let collected = collect_list(
        template,
        policy(5, CHUNKS, OnExpired::Report),
        Resume::Start,
    )
    .await;
    assert!(collected.completion.outcome.is_complete());
    assert_eq!(collected.items.len(), CHUNKS * PER);
    let in_order = names(&collected.items)
        .iter()
        .enumerate()
        .all(|(i, n)| *n == format!("pod-{i:05}"));
    assert!(in_order);
    assert_eq!(collected.completion.chunks, CHUNKS as u64);
    assert_eq!(collected.completion.requests, CHUNKS as u64);
    assert_eq!(server.requests().len(), CHUNKS);
    assert_eq!(server.peak_active_bodies(), 1);

    // A budget smaller than the chain stops after exactly that many requests, whatever the
    // remaining count says, with the next server token as the resume point.
    let before = server.requests().len();
    let template = kube(&server).namespace(ns("team-a")).core().pods();
    let bounded = collect_list(template, policy(5, 3, OnExpired::Report), Resume::Start).await;
    assert_eq!(server.requests().len() - before, 3);
    match &bounded.completion.outcome {
        ListOutcome::Bounded {
            resume: Resume::At(key),
        } => {
            assert_eq!(key.token().expose(), "g3");
            assert_eq!(key.snapshot().as_str(), "99000");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        bounded.completion.remaining_item_counts,
        [Some(295), Some(290), Some(285)]
    );
    finish(server).await;
}

fn expired_then_restart(first_chunks: Arc<AtomicUsize>) -> impl Fn(&Recorded) -> Reply {
    move |r| {
        if r.path() != "/api/v1/namespaces/team-a/pods" {
            return base_route(r).unwrap_or_else(not_found);
        }
        match query(r, "continue").as_deref() {
            None => {
                if first_chunks.fetch_add(1, Ordering::SeqCst) == 0 {
                    json("ku_a/pods_team_a_1.json")
                } else {
                    json("ku_a/pods_team_a_1_restart.json")
                }
            }
            Some(T1) => status(410, "ku_a/status_410_expired_continue.json"),
            Some(RESTARTED) => json("ku_a/pods_team_a_2_restart.json"),
            Some(_) => not_found(),
        }
    }
}

#[tokio::test]
async fn ku_a_expired_token_restarts_with_an_explicit_boundary() {
    let firsts = Arc::new(AtomicUsize::new(0));
    let server = TestServer::start(expired_then_restart(Arc::clone(&firsts))).await;
    let inv = inventory(&kube(&server), &two_namespaces(None, None)).await;
    let pods = &inv.pods[0];
    assert!(
        pods.completion.outcome.is_complete(),
        "{:?}",
        pods.completion
    );
    // The expired chain's prefix (web-a-1, web-a-2 at 5001) was discarded; the new snapshot's
    // objects replace it (web-a-2 is gone, web-a-3 and web-a-4 are new).
    assert_eq!(names(&pods.items), ["web-a-1", "web-a-3", "web-a-4"]);
    assert_eq!(pods.completion.snapshot, Some(ResourceVersion::new("5100")));
    assert_eq!(pods.completion.chunks, 2);
    assert_eq!(
        pods.completion.restarts,
        [RestartBoundary {
            expired_snapshot: Some(ResourceVersion::new("5001")),
            chunks_before: 1,
            discarded_items: 2,
            during_continuation: true,
            inconsistent_token_offered: true,
        }]
    );
    assert_eq!(pods.completion.requests, 4);
    let requests = server.requests();
    let chain = to_path(&requests, "/api/v1/namespaces/team-a/pods");
    let sent: Vec<_> = chain.iter().map(|r| query(r, "continue")).collect();
    // The restart starts without a token; the inconsistent token is never sent.
    assert_eq!(sent, [None, Some(T1.into()), None, Some(RESTARTED.into())]);
    assert!(requests.iter().all(|r| !r.target.contains(INCONSISTENT)));
    assert!(inv.is_complete());
    finish(server).await;
}

#[tokio::test]
async fn ku_a_expired_token_reported_when_restarts_are_not_allowed() {
    let firsts = Arc::new(AtomicUsize::new(0));
    let server = TestServer::start(expired_then_restart(Arc::clone(&firsts))).await;
    let mut request = two_namespaces(None, None);
    request.list.on_expired = OnExpired::Report;
    let inv = inventory(&kube(&server), &request).await;
    let pods = &inv.pods[0];
    match &pods.completion.outcome {
        ListOutcome::Expired { boundary } => {
            assert_eq!(
                boundary.expired_snapshot,
                Some(ResourceVersion::new("5001"))
            );
            assert_eq!(boundary.discarded_items, 2);
            assert!(boundary.during_continuation);
        }
        other => panic!("{other:?}"),
    }
    // The incomplete prefix stays visible, labelled by the outcome; no restart was sent.
    assert_eq!(names(&pods.items), ["web-a-1", "web-a-2"]);
    assert!(pods.completion.restarts.is_empty());
    assert_eq!(firsts.load(Ordering::SeqCst), 1);
    assert!(!inv.is_complete());
    // The other families are still reported.
    assert!(inv.pods[1].completion.outcome.is_complete());
    assert!(
        inv.deployments
            .iter()
            .all(|f| f.completion.outcome.is_complete())
    );
    finish(server).await;
}

#[tokio::test]
async fn ku_a_a_different_snapshot_mid_chain_is_a_failure() {
    let server = TestServer::start(|r| {
        if r.path() == "/api/v1/namespaces/team-a/pods" {
            return match query(r, "continue").as_deref() {
                None => json("ku_a/pods_team_a_1.json"),
                Some(T1) => json("ku_a/pods_team_a_2_other_snapshot.json"),
                Some(_) => not_found(),
            };
        }
        base_route(r).unwrap_or_else(not_found)
    })
    .await;
    let inv = inventory(&kube(&server), &two_namespaces(None, None)).await;
    let pods = &inv.pods[0];
    match &pods.completion.outcome {
        ListOutcome::Failed {
            error: ListError::InconsistentSnapshot,
            resume: Resume::At(key),
        } => {
            assert_eq!(key.token().expose(), T1);
            assert_eq!(key.snapshot().as_str(), "5001");
        }
        other => panic!("{other:?}"),
    }
    // The chunk from snapshot 5077 (web-a-9) was not merged.
    assert_eq!(names(&pods.items), ["web-a-1", "web-a-2"]);
    assert!(!inv.is_complete());
    assert!(inv.pods[1].completion.outcome.is_complete());
    finish(server).await;
}

/// A server that answers a continued chunk with the token it was sent (OPUS-2): the chain fails
/// on the first repeat, its items are not merged, and the budget (50 chunks) is not what ends it.
#[tokio::test]
async fn ku_a_a_continue_token_that_does_not_advance_is_a_failure() {
    let server = TestServer::start(|r| {
        if r.path() == "/api/v1/namespaces/team-a/pods" {
            return match query(r, "continue").as_deref() {
                None => json("ku_a/pods_team_a_1.json"),
                Some(T1) => Reply::json(
                    200,
                    format!(
                        r#"{{"kind":"PodList","apiVersion":"v1","metadata":{{"resourceVersion":"5001","continue":"{T1}"}},"items":[{{"metadata":{{"name":"web-a-1","namespace":"team-a"}}}}]}}"#
                    ),
                ),
                Some(_) => not_found(),
            };
        }
        not_found()
    })
    .await;
    let template = kube(&server).namespace(ns("team-a")).core().pods();
    let collected = collect_list(template, policy(2, 50, OnExpired::Report), Resume::Start).await;
    match &collected.completion.outcome {
        ListOutcome::Failed {
            error: ListError::ContinueDidNotAdvance,
            resume: Resume::At(key),
        } => assert_eq!(key.token().expose(), T1),
        other => panic!("{other:?}"),
    }
    assert_eq!(names(&collected.items), ["web-a-1", "web-a-2"]);
    let requests = server.requests();
    assert_eq!(requests.len(), 2, "the first repeat ends the chain");
    assert_eq!(query(&requests[1], "continue").as_deref(), Some(T1));
    finish(server).await;
}

/// SOL-R2 (fix re-check): a `continue` token left on the template is not sent with the first
/// chunk. Before the fix the first request sent it while the progress check compared the reply
/// with "no token", so a reply repeating that token was accepted once.
#[tokio::test]
async fn ku_a_a_continue_token_on_the_template_is_not_sent() {
    let server = TestServer::start(|r| {
        if r.path() == "/api/v1/namespaces/team-a/pods" {
            return pods_team_a(r);
        }
        not_found()
    })
    .await;
    let template = kube(&server)
        .namespace(ns("team-a"))
        .core()
        .pods()
        .set_continue_token(ContinueToken::from_server(T1));
    let collected = collect_list(template, policy(2, 50, OnExpired::Report), Resume::Start).await;
    assert!(
        matches!(collected.completion.outcome, ListOutcome::Complete),
        "{:?}",
        collected.completion.outcome
    );
    let requests = server.requests();
    assert_eq!(
        query(&requests[0], "continue"),
        None,
        "the first chunk is the first"
    );
    let sent: Vec<_> = requests.iter().map(|r| query(r, "continue")).collect();
    assert_eq!(sent, [None, Some(T1.to_owned()), Some(T2.to_owned())]);
    finish(server).await;
}

#[tokio::test]
async fn ku_a_bounded_chain_resumes_from_its_checkpoint() {
    let server = TestServer::start(|r| {
        if r.path() == "/api/v1/namespaces/team-a/pods" {
            return pods_team_a(r);
        }
        not_found()
    })
    .await;
    let template = kube(&server)
        .namespace(ns("team-a"))
        .core()
        .pods()
        .with_resource_version(
            Some(ResourceVersion::new("0")),
            Some(concord_example_kubernetes_sdk::ResourceVersionMatch::NotOlderThan),
        );
    let first = collect_list(
        template.clone(),
        policy(2, 1, OnExpired::Report),
        Resume::Start,
    )
    .await;
    let resume = match first.completion.outcome {
        ListOutcome::Bounded { resume } => resume,
        other => panic!("{other:?}"),
    };
    assert_eq!(names(&first.items), ["web-a-1", "web-a-2"]);
    assert_eq!(format!("{resume:?}"), "At(..)");
    let key: &Continuation = resume.key().unwrap();
    assert!(!format!("{key:?}").contains(T1));

    let rest = collect_list(template, policy(2, 5, OnExpired::Report), resume).await;
    assert!(
        rest.completion.outcome.is_complete(),
        "{:?}",
        rest.completion
    );
    assert_eq!(names(&rest.items), ["web-a-3"]);
    assert_eq!(rest.completion.chunks, 2);
    assert_eq!(rest.completion.snapshot, Some(ResourceVersion::new("5001")));

    let requests = server.requests();
    // The first chunk carries the template's resourceVersion parameters; continued chunks
    // carry the token instead (continue with a specific resourceVersion is a 400).
    assert_eq!(query(&requests[0], "resourceVersion").as_deref(), Some("0"));
    assert_eq!(
        query(&requests[0], "resourceVersionMatch").as_deref(),
        Some("NotOlderThan")
    );
    for r in &requests[1..] {
        assert!(query(r, "continue").is_some());
        assert_eq!(query(r, "resourceVersion"), None);
        assert_eq!(query(r, "resourceVersionMatch"), None);
    }
    assert_eq!(query(&requests[1], "continue").as_deref(), Some(T1));
    finish(server).await;
}

#[tokio::test]
async fn ku_a_a_forbidden_family_does_not_hide_the_others() {
    let server = TestServer::start(|r| {
        match r.path() {
            "/api/v1/namespaces/team-a/pods" => return pods_team_a(r),
            "/apis/batch/v1/namespaces/team-b/jobs" => {
                return status(403, "ku_a/status_403_forbidden.json");
            }
            _ => {}
        }
        base_route(r).unwrap_or_else(not_found)
    })
    .await;
    let inv = inventory(&kube(&server), &two_namespaces(None, None)).await;
    let jobs_b = &inv.jobs[1];
    let ListOutcome::Failed {
        error: ListError::Api(api),
        resume: Resume::Start,
    } = &jobs_b.completion.outcome
    else {
        panic!("{:?}", jobs_b.completion.outcome)
    };
    assert_eq!(api.http_status().as_u16(), 403);
    assert_eq!(api.reason(), Some("Forbidden"));
    assert_eq!(api.endpoint(), Some("Kube.batch_v1.namespaced.jobs"));
    let shown = format!("{api} {api:?}");
    // The status message names a service account; diagnostics withhold it.
    assert!(!shown.contains("serviceaccount"), "{shown}");
    assert_eq!(
        api.to_string(),
        "Kube.batch_v1.namespaced.jobs: Kubernetes API answered 403 (reason Forbidden)"
    );
    assert!(
        api.status()
            .unwrap()
            .message
            .as_deref()
            .unwrap()
            .contains("serviceaccount")
    );
    assert!(!inv.is_complete());
    assert_eq!(inv.summary().pods, 4);
    assert!(inv.jobs[0].completion.outcome.is_complete());
    finish(server).await;
}

#[tokio::test]
async fn ku_a_missing_namespace_and_diagnostics_without_secrets() {
    let server = TestServer::start(|r| {
        if r.path() == "/api/v1/namespaces/team-a/pods" {
            return pods_team_a(r);
        }
        if r.path() == "/api/v1/namespaces/team-b" {
            return not_found();
        }
        base_route(r).unwrap_or_else(not_found)
    })
    .await;
    let inv = inventory(&kube(&server), &two_namespaces(None, None)).await;
    let missing = &inv.namespaces[1];
    assert_eq!(missing.name.as_str(), "team-b");
    let error = missing.result.as_ref().unwrap_err();
    assert_eq!(error.api().unwrap().reason(), Some("NotFound"));
    assert!(!inv.is_complete());
    // Nothing the inventory prints contains a token, a continue token or ConfigMap data.
    let shown = format!("{inv:?}");
    for secret in [TOKEN, T1, T2, "synthetic-flag-a"] {
        assert!(!shown.contains(secret), "{secret} leaked");
    }
    finish(server).await;
}

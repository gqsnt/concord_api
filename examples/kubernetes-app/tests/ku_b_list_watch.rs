//! KU-B — list/watch/resync, end to end against the loopback server.
//!
//! Fixtures: `tests/fixtures/ku_b/*` (schema-derived synthetic watch events and lists; fault
//! cases built from them in the tests). Expected behaviour comes from the Kubernetes API concepts
//! page: a watch from a specific `resourceVersion` sends only later changes; a disconnected watch
//! restarts from the last returned `resourceVersion`; `410 Gone` means clear the cache, list again
//! and watch from the new list's `resourceVersion`; bookmarks carry only `metadata.resourceVersion`.

mod support;

use std::ops::ControlFlow;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use concord::bytes::Bytes;
use concord_example_kubernetes_app::collect::OnExpired;
use concord_example_kubernetes_app::list_watch::{
    Change, ChangeKind, ConnectionEnd, ResetBoundary, ResetCause, ResourceSelector, WatchFailure,
    WatchOutcome, WatchPolicy, watch_deployments, watch_pods,
};
use concord_example_kubernetes_sdk::model::{Pod, PodPhase};
use concord_example_kubernetes_sdk::{LabelSelector, ObjectKey, ObjectName, ResourceVersion};
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use support::*;

const PODS: &str = "/api/v1/namespaces/team-a/pods";

fn rv(value: &str) -> ResourceVersion {
    ResourceVersion::new(value)
}

fn key(name: &str) -> ObjectKey {
    ObjectKey {
        namespace: Some(ns("team-a")),
        name: ObjectName::new(name).unwrap(),
    }
}

/// Each connection as (start resource version, events, end).
fn connections<T>(
    report: &concord_example_kubernetes_app::list_watch::WatchReport<T>,
) -> Vec<(ResourceVersion, u64, ConnectionEnd)> {
    report
        .connections
        .iter()
        .map(|c| (c.from.clone(), c.events, c.end))
        .collect()
}

fn watch_policy(max_events: u64) -> WatchPolicy {
    WatchPolicy {
        list: policy(100, 5, OnExpired::Restart { max_restarts: 1 }),
        server_timeout_seconds: Some(300),
        bookmarks: true,
        max_event_bytes: 64 * 1024,
        max_events,
        max_reconnects: 3,
        max_relists: 1,
    }
}

fn team_a(labels: Option<LabelSelector>) -> ResourceSelector {
    ResourceSelector {
        namespace: Some(ns("team-a")),
        labels,
        fields: None,
    }
}

fn is_watch(r: &Recorded) -> bool {
    query(r, "watch").as_deref() == Some("1")
}

/// A watch response with these chunks, then the normal end of the body.
fn events(chunks: Vec<Bytes>) -> Reply {
    Reply::new(200)
        .header("content-type", "application/json")
        .with_body(Body::Chunks(
            chunks.into_iter().map(|c| (Duration::ZERO, c)).collect(),
        ))
}

/// A watch response with these chunks, then whitespace every 10 ms forever (no further value:
/// the decoder keeps waiting; the server notices when the client goes away).
fn events_then_idle(chunks: Vec<Bytes>) -> Reply {
    let mut chunks = std::collections::VecDeque::from(chunks);
    let next = Box::new(move || Some(chunks.pop_front().unwrap_or(Bytes::from_static(b"\n"))));
    Reply::new(200)
        .header("content-type", "application/json")
        .with_body(Body::Generate(next, Some(Duration::from_millis(10))))
}

/// The fixture bytes split at the given offsets.
fn split(bytes: &[u8], at: &[usize]) -> Vec<Bytes> {
    let mut out = Vec::new();
    let mut start = 0;
    for &end in at {
        out.push(Bytes::copy_from_slice(&bytes[start..end]));
        start = end;
    }
    out.push(Bytes::copy_from_slice(&bytes[start..]));
    out
}

fn find(haystack: &[u8], needle: &str) -> usize {
    haystack
        .windows(needle.len())
        .position(|w| w == needle.as_bytes())
        .unwrap_or_else(|| panic!("{needle} not in fixture"))
}

/// The first events fixture split inside a JSON token and inside two multibyte UTF-8
/// characters.
fn watch_1_split() -> Vec<Bytes> {
    let bytes = fixture("ku_b/watch_pods_team_a_1.json-values");
    let token = find(&bytes, "\"resourceVersion\"") + 8; // inside `"resourceVersion"`
    let coffee = find(&bytes, "☕") + 1; // after the first of its three bytes
    let tokyo = find(&bytes, "東") + 2; // after two of its three bytes
    let number = find(&bytes, "\"restartCount\":") + 15; // between `:` and the digit
    let mut at = vec![token, number, coffee, tokyo];
    at.sort_unstable();
    split(&bytes, &at)
}

type Log = Arc<Mutex<Vec<String>>>;

fn logger(log: &Log) -> impl FnMut(Change<'_, Pod>) -> ControlFlow<()> + use<> {
    let log = Arc::clone(log);
    move |change| {
        let line = match change {
            Change::Reset(b) => {
                format!("reset {:?} {} {}", b.cause, b.snapshot.as_str(), b.objects)
            }
            Change::Applied { kind, key, .. } => format!("{kind:?} {key}"),
            Change::Bookmark(rv) => format!("bookmark {}", rv.as_str()),
            Change::Reconnecting { from, after } => {
                format!("reconnect {} {after:?}", from.as_str())
            }
        };
        log.lock().unwrap().push(line);
        ControlFlow::Continue(())
    }
}

#[tokio::test]
async fn ku_b_list_watch_applies_typed_changes_and_reconnects_after_normal_end() {
    let server = TestServer::start(|r| {
        if r.path() != PODS {
            return not_found();
        }
        if !is_watch(r) {
            return json("ku_b/pods_list_team_a.json");
        }
        match query(r, "resourceVersion").as_deref() {
            Some("8000") => events(watch_1_split()),
            Some("8004") => {
                events_then_idle(vec![fixture("ku_b/watch_pods_team_a_2.json-values").into()])
            }
            _ => not_found(),
        }
    })
    .await;
    let log = Log::default();
    let labels = LabelSelector::equals("app", "web").unwrap();
    let report = watch_pods(
        &kube(&server),
        &team_a(Some(labels)),
        watch_policy(5),
        logger(&log),
        std::future::pending(),
    )
    .await;

    assert!(
        matches!(report.outcome, WatchOutcome::EventBudget),
        "{:?}",
        report.outcome
    );
    assert_eq!(
        *log.lock().unwrap(),
        [
            "reset Initial 8000 2",
            "Modified team-a/web-a-1",
            "Added team-a/web-a-3",
            "bookmark 8003",
            "Deleted team-a/web-a-2",
            "reconnect 8004 Normal",
            "Modified team-a/web-a-3",
        ]
    );
    // Current state: web-a-2 deleted, web-a-1 modified (not ready), web-a-3 added then running.
    assert_eq!(
        report.state.keys().cloned().collect::<Vec<_>>(),
        [key("web-a-1"), key("web-a-3")]
    );
    assert!(!report.state[&key("web-a-1")].all_containers_ready());
    let web3 = &report.state[&key("web-a-3")];
    assert_eq!(web3.status.phase, Some(PodPhase::Running));
    assert_eq!(report.resource_version, Some(rv("8005")));
    assert_eq!(report.counts.added, 1);
    assert_eq!(report.counts.modified, 2);
    assert_eq!(report.counts.deleted, 1);
    assert_eq!(report.counts.bookmarks, 1);
    assert_eq!(
        connections(&report),
        [
            (rv("8000"), 4, ConnectionEnd::Normal),
            (rv("8004"), 1, ConnectionEnd::EventBudget),
        ]
    );
    assert_eq!(report.resets.len(), 1);

    // Wire: one list, then watches from the list's and the last event's resourceVersion.
    let requests = server.requests();
    assert_authenticated(&requests);
    assert_eq!(requests.len(), 3);
    assert_eq!(query(&requests[0], "watch"), None);
    assert_eq!(query(&requests[0], "limit").as_deref(), Some("100"));
    for (r, from) in requests[1..].iter().zip(["8000", "8004"]) {
        assert_eq!(query(r, "watch").as_deref(), Some("1"));
        assert_eq!(query(r, "resourceVersion").as_deref(), Some(from));
        assert_eq!(query(r, "allowWatchBookmarks").as_deref(), Some("true"));
        assert_eq!(query(r, "timeoutSeconds").as_deref(), Some("300"));
        assert_eq!(query(r, "labelSelector").as_deref(), Some("app=web"));
        assert_eq!(query(r, "continue"), None);
        assert_eq!(r.header("accept"), Some("application/json"));
    }
    finish(server).await;
}

#[tokio::test]
async fn ku_b_split_utf8_is_decoded_intact() {
    let server = TestServer::start(|r| {
        if !is_watch(r) {
            return json("ku_b/pods_list_team_a.json");
        }
        events(watch_1_split())
    })
    .await;
    let seen = Arc::new(Mutex::new(None));
    let sink = {
        let seen = Arc::clone(&seen);
        move |change: Change<'_, Pod>| {
            if let Change::Applied {
                kind: ChangeKind::Added,
                object,
                ..
            } = change
            {
                *seen.lock().unwrap() = object
                    .metadata
                    .annotations
                    .get("example.test/description")
                    .cloned();
            }
            ControlFlow::Continue(())
        }
    };
    let report = watch_pods(
        &kube(&server),
        &team_a(None),
        watch_policy(2),
        sink,
        std::future::pending(),
    )
    .await;
    assert!(matches!(report.outcome, WatchOutcome::EventBudget));
    assert_eq!(
        seen.lock().unwrap().as_deref(),
        Some("café ☕ — Größe 東京")
    );
    finish(server).await;
}

#[tokio::test]
async fn ku_b_transport_loss_reconnects_from_the_last_resource_version() {
    let bytes = fixture("ku_b/watch_pods_team_a_1.json-values");
    // Two complete events, then half of the bookmark, then the connection drops.
    let cut = find(&bytes, "\"BOOKMARK\"") + 4;
    let prefix = Bytes::copy_from_slice(&bytes[..cut]);
    let rest = Bytes::copy_from_slice(&bytes[find(&bytes, "{\"type\":\"BOOKMARK\"")..]);
    let server = TestServer::start(move |r| {
        if !is_watch(r) {
            return json("ku_b/pods_list_team_a.json");
        }
        match query(r, "resourceVersion").as_deref() {
            Some("8000") => Reply::new(200)
                .header("content-type", "application/json")
                .with_body(Body::FailAfter(vec![prefix.clone()])),
            Some("8002") => events_then_idle(vec![rest.clone()]),
            _ => not_found(),
        }
    })
    .await;
    let log = Log::default();
    let report = watch_pods(
        &kube(&server),
        &team_a(None),
        watch_policy(4),
        logger(&log),
        std::future::pending(),
    )
    .await;
    assert!(
        matches!(report.outcome, WatchOutcome::EventBudget),
        "{:?}",
        report.outcome
    );
    assert_eq!(
        connections(&report),
        [
            (rv("8000"), 2, ConnectionEnd::TransportLost),
            (rv("8002"), 2, ConnectionEnd::EventBudget),
        ]
    );
    // Nothing applied twice, nothing lost: each of the four events exactly once.
    assert_eq!(
        *log.lock().unwrap(),
        [
            "reset Initial 8000 2",
            "Modified team-a/web-a-1",
            "Added team-a/web-a-3",
            "reconnect 8002 TransportLost",
            "bookmark 8003",
            "Deleted team-a/web-a-2",
        ]
    );
    assert_eq!(report.resource_version, Some(rv("8004")));
    finish(server).await;
}

#[tokio::test]
async fn ku_b_expired_history_relists_with_an_explicit_reset_boundary() {
    let bytes = fixture("ku_b/watch_pods_team_a_1.json-values");
    let two_events = Bytes::copy_from_slice(&bytes[..find(&bytes, "{\"type\":\"BOOKMARK\"")]);
    let lists = Arc::new(Mutex::new(0));
    let server = TestServer::start({
        let lists = Arc::clone(&lists);
        move |r| {
            if !is_watch(r) {
                let mut n = lists.lock().unwrap();
                *n += 1;
                return if *n == 1 {
                    json("ku_b/pods_list_team_a.json")
                } else {
                    json("ku_b/pods_list_team_a_relist.json")
                };
            }
            match query(r, "resourceVersion").as_deref() {
                Some("8000") => events(vec![
                    two_events.clone(),
                    fixture("ku_b/watch_error_410.json-values").into(),
                ]),
                Some("9000") => {
                    events_then_idle(vec![fixture("ku_b/watch_pods_team_a_3.json-values").into()])
                }
                _ => not_found(),
            }
        }
    })
    .await;
    let log = Log::default();
    let report = watch_pods(
        &kube(&server),
        &team_a(None),
        watch_policy(3),
        logger(&log),
        std::future::pending(),
    )
    .await;
    assert!(
        matches!(report.outcome, WatchOutcome::EventBudget),
        "{:?}",
        report.outcome
    );
    assert_eq!(
        report.resets[1],
        ResetBoundary {
            cause: ResetCause::ExpiredEvent,
            snapshot: rv("9000"),
            abandoned_at: Some(rv("8002")),
            previous_objects: 3,
            objects: 2,
            vanished: 2,
            list_restarts: 0,
        }
    );
    assert_eq!(
        *log.lock().unwrap(),
        [
            "reset Initial 8000 2",
            "Modified team-a/web-a-1",
            "Added team-a/web-a-3",
            "reset ExpiredEvent 9000 2",
            "Added team-a/web-a-6",
        ]
    );
    assert_eq!(
        report.state.keys().cloned().collect::<Vec<_>>(),
        [key("web-a-3"), key("web-a-5"), key("web-a-6")]
    );
    assert_eq!(report.connections[0].end, ConnectionEnd::Expired);
    assert_eq!(report.connections[1].from, rv("9000"));
    assert_eq!(*lists.lock().unwrap(), 2);
    finish(server).await;
}

#[tokio::test]
async fn ku_b_410_on_open_relists_and_the_relist_budget_is_explicit() {
    let server = TestServer::start(|r| {
        if !is_watch(r) {
            return json("ku_b/pods_list_team_a.json");
        }
        status(410, "ku_b/status_410_watch_open.json")
    })
    .await;
    let mut policy = watch_policy(10);
    policy.max_relists = 1;
    let log = Log::default();
    let report = watch_pods(
        &kube(&server),
        &team_a(None),
        policy,
        logger(&log),
        std::future::pending(),
    )
    .await;
    // First 410: relist (allowed once). Second 410: the budget is used.
    assert!(
        matches!(report.outcome, WatchOutcome::RelistBudget),
        "{:?}",
        report.outcome
    );
    assert_eq!(report.resets.len(), 2);
    assert_eq!(report.resets[1].cause, ResetCause::ExpiredOnOpen);
    assert_eq!(report.resets[1].vanished, 0);
    assert_eq!(
        report.connections.iter().map(|c| c.end).collect::<Vec<_>>(),
        [ConnectionEnd::Expired, ConnectionEnd::Expired]
    );
    assert_eq!(report.state.len(), 2);
    finish(server).await;
}

#[tokio::test]
async fn ku_b_malformed_events_end_the_connection_and_reconnect() {
    let bytes = fixture("ku_b/watch_pods_team_a_1.json-values");
    let first = Bytes::copy_from_slice(&bytes[..find(&bytes, "{\"type\":\"ADDED\"")]);
    let server = TestServer::start(move |r| {
        if !is_watch(r) {
            return json("ku_b/pods_list_team_a.json");
        }
        match query(r, "resourceVersion").as_deref() {
            // One good event, then bytes that are not JSON.
            Some("8000") => events(vec![
                first.clone(),
                Bytes::from_static(b"{\"type\":\"MODIFIED\",]}\n"),
            ]),
            // Well-formed JSON with an unknown event type: a decode error.
            Some("8001") => events(vec![Bytes::from_static(
                b"{\"type\":\"SYNC\",\"object\":{}}\n",
            )]),
            _ => not_found(),
        }
    })
    .await;
    let mut policy = watch_policy(10);
    policy.max_reconnects = 1;
    let report = watch_pods(
        &kube(&server),
        &team_a(None),
        policy,
        |_| ControlFlow::Continue(()),
        std::future::pending(),
    )
    .await;
    assert!(
        matches!(report.outcome, WatchOutcome::ReconnectBudget),
        "{:?}",
        report.outcome
    );
    assert_eq!(
        connections(&report),
        [
            (rv("8000"), 1, ConnectionEnd::Malformed),
            (rv("8001"), 0, ConnectionEnd::Malformed),
        ]
    );
    // The good event stays applied.
    assert_eq!(report.resource_version, Some(rv("8001")));
    assert_eq!(report.counts.modified, 1);
    finish(server).await;
}

#[tokio::test]
async fn ku_b_an_oversized_event_is_a_limit_not_a_clean_end() {
    let big = format!(
        r#"{{"type":"MODIFIED","object":{{"kind":"Pod","apiVersion":"v1","metadata":{{"name":"web-a-1","namespace":"team-a","resourceVersion":"8001","annotations":{{"blob":"{}"}}}}}}}}"#,
        "x".repeat(4096)
    );
    let server = TestServer::start(move |r| {
        if !is_watch(r) {
            return json("ku_b/pods_list_team_a.json");
        }
        events(vec![Bytes::from(big.clone())])
    })
    .await;
    let mut policy = watch_policy(10);
    policy.max_event_bytes = 1024;
    policy.max_reconnects = 0;
    let report = watch_pods(
        &kube(&server),
        &team_a(None),
        policy,
        |_| ControlFlow::Continue(()),
        std::future::pending(),
    )
    .await;
    assert!(
        matches!(report.outcome, WatchOutcome::ReconnectBudget),
        "{:?}",
        report.outcome
    );
    assert_eq!(report.connections[0].end, ConnectionEnd::Oversized);
    assert_eq!(report.counts.total(), 0);
    assert_eq!(report.resource_version, Some(rv("8000")));
    finish(server).await;
}

#[tokio::test]
async fn ku_b_cancellation_while_awaiting_data() {
    let server = TestServer::start(|r| {
        if !is_watch(r) {
            return json("ku_b/pods_list_team_a.json");
        }
        // Headers, then nothing at all.
        Reply::new(200)
            .header("content-type", "application/json")
            .with_body(Body::Stall)
    })
    .await;
    let (cancel, cancelled) = tokio::sync::oneshot::channel::<()>();
    let kube = kube(&server);
    let selector = team_a(None);
    let run = watch_pods(
        &kube,
        &selector,
        watch_policy(10),
        |_| ControlFlow::Continue(()),
        async {
            let _ = cancelled.await;
        },
    );
    let watcher = async {
        assert!(
            server
                .wait_until(Duration::from_secs(5), |s| s.requests().len() == 2)
                .await
        );
        // The watch is open and its body is stalled: the workflow awaits data.
        tokio::time::sleep(Duration::from_millis(50)).await;
        let _ = cancel.send(());
    };
    let started = tokio::time::Instant::now();
    let (report, ()) =
        tokio::time::timeout(Duration::from_secs(5), async { tokio::join!(run, watcher) })
            .await
            .expect("cancellation took effect while awaiting data");
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(
        matches!(report.outcome, WatchOutcome::Cancelled),
        "{:?}",
        report.outcome
    );
    assert_eq!(report.connections[0].end, ConnectionEnd::Cancelled);
    assert_eq!(report.state.len(), 2);
    // The stalled server task holds its socket until shutdown, which closes it.
    finish(server).await;
}

#[tokio::test]
async fn ku_b_dropping_the_workflow_releases_the_watch_connection() {
    let server = TestServer::start(|r| {
        if !is_watch(r) {
            return json("ku_b/pods_list_team_a.json");
        }
        events_then_idle(vec![fixture("ku_b/watch_pods_team_a_2.json-values").into()])
    })
    .await;
    let (seen, first_event) = tokio::sync::oneshot::channel::<()>();
    let mut seen = Some(seen);
    let sink = move |change: Change<'_, Pod>| {
        if matches!(change, Change::Applied { .. })
            && let Some(seen) = seen.take()
        {
            let _ = seen.send(());
        }
        ControlFlow::Continue(())
    };
    let kube = kube(&server);
    let selector = team_a(None);
    let run = watch_pods(
        &kube,
        &selector,
        watch_policy(1000),
        sink,
        std::future::pending(),
    );
    tokio::select! {
        _ = run => panic!("the workflow ended on its own"),
        _ = first_event => {} // the workflow future is dropped here, mid-watch
    }
    assert_eq!(server.active_bodies(), 1);
    // The client closed the connection: the server's next heartbeat write fails.
    assert!(
        server
            .wait_until(Duration::from_secs(5), |s| s.active_bodies() == 0
                && s.live_connections() == 0)
            .await,
        "active bodies {}, live connections {}",
        server.active_bodies(),
        server.live_connections()
    );
    finish(server).await;
}

#[tokio::test]
async fn ku_b_a_consumer_can_stop_the_run() {
    let server = TestServer::start(|r| {
        if !is_watch(r) {
            return json("ku_b/pods_list_team_a.json");
        }
        events_then_idle(vec![fixture("ku_b/watch_pods_team_a_1.json-values").into()])
    })
    .await;
    let mut applied = 0;
    let sink = |change: Change<'_, Pod>| {
        if matches!(change, Change::Applied { .. }) {
            applied += 1;
            if applied == 2 {
                return ControlFlow::Break(());
            }
        }
        ControlFlow::Continue(())
    };
    let report = watch_pods(
        &kube(&server),
        &team_a(None),
        watch_policy(1000),
        sink,
        std::future::pending(),
    )
    .await;
    assert!(
        matches!(report.outcome, WatchOutcome::ConsumerStopped),
        "{:?}",
        report.outcome
    );
    assert_eq!(report.connections[0].end, ConnectionEnd::ConsumerStopped);
    // The event that stopped the run was applied before the sink saw it.
    assert!(report.state.contains_key(&key("web-a-3")));
    assert_eq!(report.resource_version, Some(rv("8002")));
    finish(server).await;
}

#[tokio::test]
async fn ku_b_deployments_in_a_second_namespace() {
    let server = TestServer::start(|r| {
        if r.path() != "/apis/apps/v1/namespaces/team-b/deployments" {
            return not_found();
        }
        if !is_watch(r) {
            return json("ku_b/deployments_list_team_b.json");
        }
        events(vec![
            fixture("ku_b/watch_deployments_team_b.json-values").into(),
        ])
    })
    .await;
    let selector = ResourceSelector {
        namespace: Some(ns("team-b")),
        labels: None,
        fields: None,
    };
    let mut policy = watch_policy(10);
    policy.max_reconnects = 0;
    let report = watch_deployments(
        &kube(&server),
        &selector,
        policy,
        |_| ControlFlow::Continue(()),
        std::future::pending(),
    )
    .await;
    assert!(
        matches!(report.outcome, WatchOutcome::ReconnectBudget),
        "{:?}",
        report.outcome
    );
    assert_eq!(report.connections[0].end, ConnectionEnd::Normal);
    let names: Vec<_> = report.state.keys().map(ToString::to_string).collect();
    assert_eq!(names, ["team-b/web", "team-b/worker"]);
    assert!(report.state.values().next().unwrap().is_available());
    assert_eq!(report.resource_version, Some(rv("7102")));
    for r in server.requests() {
        assert!(
            r.path().starts_with("/apis/apps/v1/namespaces/team-b/"),
            "{}",
            r.target
        );
    }
    finish(server).await;
}

#[tokio::test]
async fn ku_b_unrecoverable_open_failures() {
    // A non-410 status ends the run with the API's Status; a refused media type keeps the
    // response unread.
    let server = TestServer::start(|r| {
        if !is_watch(r) {
            return json("ku_b/pods_list_team_a.json");
        }
        if r.path() == PODS {
            status(403, "ku_a/status_403_forbidden.json")
        } else {
            Reply::text(200, "text/html", "<html>proxy login</html>")
        }
    })
    .await;
    let report = watch_pods(
        &kube(&server),
        &team_a(None),
        watch_policy(10),
        |_| ControlFlow::Continue(()),
        std::future::pending(),
    )
    .await;
    match &report.outcome {
        WatchOutcome::Failed(WatchFailure::Api(api)) => {
            assert_eq!(api.http_status().as_u16(), 403);
            assert_eq!(api.endpoint(), Some("Kube.core_v1.namespaced.watch_pods"));
        }
        other => panic!("{other:?}"),
    }
    let all = ResourceSelector::default();
    let report = watch_pods(
        &kube(&server),
        &all,
        watch_policy(10),
        |_| ControlFlow::Continue(()),
        std::future::pending(),
    )
    .await;
    match report.outcome {
        WatchOutcome::Failed(WatchFailure::Refused(refused)) => {
            assert_eq!(refused.operation(), Some("Kube.core_v1.watch_pods"));
            let body = refused.into_response().text().await.unwrap();
            assert_eq!(body, "<html>proxy login</html>");
        }
        other => panic!("{other:?}"),
    }
    finish(server).await;
}

/// A long generated watch: 5,000 `MODIFIED` events over 20 pods, produced lazily by the server.
/// The state stays at 20 objects; every event is applied once, in order.
#[tokio::test]
async fn ku_b_a_long_watch_keeps_bounded_state() {
    const EVENTS: u64 = 5_000;
    const PODS_N: u64 = 20;
    let server = TestServer::start(|r| {
        if !is_watch(r) {
            return json("ku_b/pods_list_team_a.json");
        }
        let generator = concord_example_testserver::generated_records(EVENTS, 16 * 1024, |i| {
            format!(
                "{{\"type\":\"MODIFIED\",\"object\":{{\"kind\":\"Pod\",\"apiVersion\":\"v1\",\"metadata\":{{\"name\":\"gen-{:02}\",\"namespace\":\"team-a\",\"resourceVersion\":\"{}\"}},\"status\":{{\"phase\":\"Running\"}}}}}}\n",
                i % PODS_N,
                10_000 + i
            )
            .into_bytes()
        });
        Reply::new(200)
            .header("content-type", "application/json")
            .with_body(Body::Generate(generator, None))
    })
    .await;
    let mut last = 0u64;
    let mut ordered = true;
    let sink = |change: Change<'_, Pod>| {
        if let Change::Applied { object, .. } = change {
            let v: u64 = object
                .metadata
                .resource_version
                .as_ref()
                .unwrap()
                .as_str()
                .parse()
                .unwrap();
            ordered &= v > last;
            last = v;
        }
        ControlFlow::Continue(())
    };
    let mut policy = watch_policy(EVENTS);
    policy.max_reconnects = 0;
    let started = std::time::Instant::now();
    let report = watch_pods(
        &kube(&server),
        &team_a(None),
        policy,
        sink,
        std::future::pending(),
    )
    .await;
    let elapsed = started.elapsed();
    assert!(
        matches!(report.outcome, WatchOutcome::EventBudget),
        "{:?}",
        report.outcome
    );
    assert!(ordered);
    assert_eq!(report.counts.modified, EVENTS);
    assert_eq!(report.state.len() as u64, 2 + PODS_N);
    assert_eq!(
        report.resource_version,
        Some(rv(&(10_000 + EVENTS - 1).to_string()))
    );
    // Incremental decoding: the decoder never held more than one event's bytes, although the
    // server sent hundreds of kilobytes on one connection.
    let peak = report.connections[0].peak_retained_bytes;
    let largest_event = 220;
    assert!(
        peak > 0 && peak <= largest_event,
        "peak retained {peak} bytes"
    );
    eprintln!(
        "long watch: {EVENTS} events, {} body bytes sent by the server, decoder peak retained \
         {peak} bytes, {elapsed:?}",
        server.body_bytes_sent()
    );
    finish(server).await;
}

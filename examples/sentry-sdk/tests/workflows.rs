//! Connected local workflows and scheduling regressions; all data is synthetic.
mod support;

use concord_sdk_sentry::{Error, HttpClass, PagesError, Resume, Settings};
use concord_testserver::{Body, Gate, Recorded, Reply, TestServer};
use std::{
    num::{NonZeroU32, NonZeroUsize},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const ISSUES_1: &str = include_str!("../fixtures/issues-1.json");
const ISSUES_2: &str = include_str!("../fixtures/issues-2.json");
const ISSUE: &str = include_str!("../fixtures/issue.json");
const EVENTS_1: &str = include_str!("../fixtures/events-1.json");
const EVENTS_2: &str = include_str!("../fixtures/events-2.json");
const EVENT: &str = include_str!("../fixtures/event.json");
const PROBLEM: &str = include_str!("../fixtures/problem.json");
const NEXT: &str = include_str!("../fixtures/next.link");
const LAST: &str = include_str!("../fixtures/last.link");

fn cap(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}
fn retry_settings() -> Settings {
    Settings {
        max_attempts: cap(3),
        ..support::settings()
    }
}

#[tokio::test]
async fn list_organization_issues_by_cursor_then_retrieve_chosen_issue() {
    let server = TestServer::start(|r: &Recorded| match r.path() {
        "/api/0/organizations/acme/issues/" => {
            assert_eq!(r.query_all("query"), [""]);
            assert_eq!(r.query_all("limit"), ["1"]);
            match r.query_all("cursor").as_slice() {
                [] => Reply::json(200, ISSUES_1).header("link", NEXT.trim()),
                [c] if c == "0:1:0" => Reply::json(200, ISSUES_2).header("link", LAST.trim()),
                _ => panic!("results=false cursor must never be requested"),
            }
        }
        "/api/0/organizations/acme/issues/202/" => Reply::json(200, ISSUE),
        path => panic!("unexpected path {path}"),
    })
    .await;
    let org = support::api(&server, support::settings()).organization("acme");
    let mut pages = org
        .issues()
        .set_query("")
        .set_limit(1)
        .pages(Resume::Start, cap(5));
    let mut ids = Vec::new();
    while let Some(page) = pages.next_page().await.unwrap() {
        assert_eq!(page.meta.status().as_u16(), 200);
        ids.extend(page.items.into_iter().map(|issue| issue.id));
    }
    assert_eq!(ids, ["101", "202"]);
    assert!(pages.next_page().await.unwrap().is_none());
    let response = org.issue(&ids[1]).get().send().await.unwrap();
    assert_eq!(response.value.id, "202");
    assert_eq!(response.value.status, "resolved");
    assert_eq!(response.value.short_id, "DEMO-2");
    assert_eq!(server.requests().len(), 3);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn page_issue_events_using_link_then_retrieve_event_id_not_storage_id() {
    let server = TestServer::start(|r: &Recorded| match r.path() {
        "/api/0/organizations/acme/issues/202/events/" => {
            assert_eq!(r.query_all("per_page"), ["1"]);
            match r.query_all("cursor").as_slice() {
                [] => Reply::json(200, EVENTS_1).header("link", NEXT.trim()),
                [c] if c == "0:1:0" => Reply::json(200, EVENTS_2).header("link", LAST.trim()),
                _ => panic!("terminal next link was followed"),
            }
        }
        "/api/0/organizations/acme/issues/202/events/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb/" => {
            Reply::json(200, EVENT)
        }
        path => panic!("unexpected event lookup: {path}"),
    })
    .await;
    let issue = support::api(&server, support::settings())
        .organization("acme")
        .issue("202");
    let events = issue
        .events()
        .set_per_page(1)
        .pages(Resume::Start, cap(5))
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[1].id, "storage-b");
    assert_eq!(events[1].event_id, "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
    let event = issue.event(&events[1].event_id).send().await.unwrap();
    assert_eq!(event.value.group_id, "202");
    assert_eq!(
        event.value.entries[0]["data"]["values"][0]["type"],
        "SyntheticError"
    );
    assert_eq!(server.requests().len(), 3);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn empty_nonterminal_page_continues_and_terminal_next_can_point_anywhere() {
    let server = TestServer::start(|r: &Recorded| {
        if r.query_all("cursor").is_empty() {
            Reply::json(200, include_str!("../fixtures/empty.json")).header("link", NEXT.trim())
        } else {
            Reply::json(200, ISSUES_2).header(
                "link",
                "<https://elsewhere.test/?cursor=unused>; rel=next; results=false",
            )
        }
    })
    .await;
    let items = support::api(&server, support::settings())
        .organization("acme")
        .issues()
        .pages(Resume::Start, cap(3))
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, "202");
    assert_eq!(server.requests().len(), 2);
}

#[tokio::test]
async fn page_cap_is_explicit_and_resumes_without_restarting() {
    let server = TestServer::start(|r: &Recorded| {
        if r.query_all("cursor").is_empty() {
            Reply::json(200, ISSUES_1).header("link", NEXT.trim())
        } else {
            Reply::json(200, ISSUES_2).header("link", LAST.trim())
        }
    })
    .await;
    let op = support::api(&server, support::settings())
        .organization("acme")
        .issues();
    let mut pages = op.clone().pages(Resume::Start, cap(1));
    assert_eq!(pages.next_page().await.unwrap().unwrap().items[0].id, "101");
    let error = pages.next_page().await.unwrap_err();
    let PagesError::Bounded { resume, .. } = error else {
        panic!("{error:?}")
    };
    assert_eq!(resume, Resume::At("0:1:0".to_owned()));
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Ended { .. })
    ));
    let rest = op
        .pages(resume, cap(1))
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(rest.len(), 1);
    assert_eq!(rest[0].id, "202");
    assert_eq!(server.requests().len(), 2);
}

#[tokio::test]
async fn provider_failure_preserves_prefix_and_failed_cursor() {
    let server = TestServer::start(|r: &Recorded| {
        if r.query_all("cursor").is_empty() {
            Reply::json(200, EVENTS_1).header("link", NEXT.trim())
        } else {
            Reply::json(404, PROBLEM)
        }
    })
    .await;
    let incomplete = support::api(&server, support::settings())
        .organization("acme")
        .issue("202")
        .events()
        .pages(Resume::Start, cap(5))
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 1);
    assert_eq!(incomplete.error.resume(), Some(&Resume::At("0:1:0".into())));
    let Some(Error::Provider(provider)) = incomplete.error.provider_error() else {
        panic!("{:?}", incomplete.error)
    };
    assert_eq!(provider.class, HttpClass::NotFound);
    assert_eq!(server.requests().len(), 2);
}

#[tokio::test]
async fn multiple_link_fields_and_url_only_cursor_work_without_native_follow() {
    let server = TestServer::start(|r: &Recorded| {
        assert_eq!(r.query_all("query"), ["release:fixed"]);
        if r.query_all("cursor").is_empty() {
            Reply::json(200, EVENTS_1)
                .header("link", "<?cursor=prev>; rel=previous; results=false")
                .header("link", "<?cursor=0%3A1%3A0>; rel=next; results=true")
        } else {
            assert_eq!(r.query_all("cursor"), ["0:1:0"]);
            Reply::json(200, EVENTS_2) // no next is also exhaustion
        }
    })
    .await;
    let items = support::api(&server, support::settings())
        .organization("acme")
        .issue("202")
        .events()
        .set_query("release:fixed")
        .pages(Resume::Start, cap(4))
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(server.requests().len(), 2);
}

fn concurrency_headers(mut reply: Reply) -> Reply {
    for line in include_str!("../fixtures/concurrency.headers").lines() {
        let (name, value) = line.split_once(':').unwrap();
        reply = reply.header(name, value.trim());
    }
    reply
}

#[tokio::test]
async fn retry_reacquires_concurrency_and_releases_lease_during_backoff() {
    let gate = Gate::new();
    let hold = gate.clone();
    let retry_calls = Arc::new(AtomicUsize::new(0));
    let calls = retry_calls.clone();
    let server = TestServer::start(move |r: &Recorded| match r.path() {
        "/api/0/organizations/acme/issues/retry/" => {
            if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                concurrency_headers(Reply::json(429, PROBLEM))
            } else {
                Reply::json(200, ISSUE)
            }
        }
        "/api/0/organizations/another/issues/blocker/" => {
            Reply::json(200, ISSUE).after(hold.clone())
        }
        path => panic!("unexpected path {path}"),
    })
    .await;
    let api = support::api(&server, retry_settings());
    let op = api.organization("acme").issue("retry").get();
    let retry = tokio::spawn(async move { op.send().await });
    assert!(
        server
            .wait_until(Duration::from_secs(1), |s| s.requests().len() == 1)
            .await
    );
    // This request obtains the released lease during the first request's backoff.
    let op = api.clone().organization("another").issue("blocker").get();
    let blocker = tokio::spawn(async move { op.send().await });
    assert!(
        server
            .wait_until(Duration::from_secs(1), |s| s.requests().len() >= 2)
            .await
    );
    assert_eq!(
        server.requests()[1].path(),
        "/api/0/organizations/another/issues/blocker/"
    );
    // Even after its retry delay expires, retry cannot dispatch while blocker owns the lease.
    assert!(
        !server
            .wait_until(Duration::from_millis(120), |s| s.requests().len() > 2)
            .await
    );
    assert_eq!(retry_calls.load(Ordering::SeqCst), 1);
    gate.open();
    blocker.await.unwrap().unwrap();
    let response = retry.await.unwrap().unwrap();
    assert_eq!(response.retry.retry.attempts, 2);
    assert_eq!(response.retry.retry.waits, [Duration::from_millis(50)]);
    assert_eq!(server.requests().len(), 3);
}

#[tokio::test]
async fn concurrency_is_held_through_body_and_cancellation_releases_it() {
    let server = TestServer::start(|r: &Recorded| {
        if r.path().contains("/stall/") {
            Reply::json(200, "").with_body(Body::Stall)
        } else {
            Reply::json(200, ISSUE)
        }
    })
    .await;
    let api = support::api(&server, retry_settings());
    let op = api.organization("acme").issue("stall").get();
    let first = tokio::spawn(async move { op.send().await });
    assert!(
        server
            .wait_until(Duration::from_secs(1), |s| s.active_bodies() == 1)
            .await
    );
    let op = api.organization("acme").issue("202").get();
    let second = tokio::spawn(async move { op.send().await });
    assert!(
        !server
            .wait_until(Duration::from_millis(100), |s| s.requests().len() > 1)
            .await
    );
    first.abort();
    assert!(first.await.unwrap_err().is_cancelled());
    second.await.unwrap().unwrap();
    assert_eq!(server.requests().len(), 2);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn provider_concurrency_limit_caps_a_larger_local_limit() {
    let gate = Gate::new();
    let hold = gate.clone();
    let server = TestServer::start(move |r: &Recorded| {
        if r.path().contains("/learn/") {
            Reply::json(200, ISSUE).header("x-sentry-rate-limit-concurrentlimit", "1")
        } else if r.path().contains("/hold/") {
            Reply::json(200, ISSUE).after(hold.clone())
        } else {
            Reply::json(200, ISSUE)
        }
    })
    .await;
    let settings = Settings {
        concurrency: cap(3),
        ..support::settings()
    };
    let api = support::api(&server, settings);
    let response = api
        .organization("acme")
        .issue("learn")
        .get()
        .send()
        .await
        .unwrap();
    assert_eq!(response.rates.concurrent_limit, Some(1));
    let op = api.organization("acme").issue("hold").get();
    let holding = tokio::spawn(async move { op.send().await });
    assert!(
        server
            .wait_until(Duration::from_secs(1), |s| s.requests().len() == 2)
            .await
    );
    let op = api.organization("acme").issue("202").get();
    let waiting = tokio::spawn(async move { op.send().await });
    assert!(
        !server
            .wait_until(Duration::from_millis(100), |s| s.requests().len() > 2)
            .await
    );
    gate.open();
    holding.await.unwrap().unwrap();
    waiting.await.unwrap().unwrap();
}

#[tokio::test]
async fn frequency_observation_is_checked_after_waiting_for_concurrency() {
    let reset = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 2;
    let gate = Gate::new();
    let hold = gate.clone();
    let dispatched = Arc::new(Mutex::new(Vec::new()));
    let times = dispatched.clone();
    let server = TestServer::start(move |r: &Recorded| {
        times.lock().unwrap().push(SystemTime::now());
        if r.path().contains("/hold/") {
            Reply::json(200, ISSUE)
                .header("x-sentry-rate-limit-limit", "10")
                .header("x-sentry-rate-limit-remaining", "0")
                .header("x-sentry-rate-limit-reset", reset.to_string())
                .after(hold.clone())
        } else {
            Reply::json(200, ISSUE)
        }
    })
    .await;
    let api = support::api(&server, support::settings());
    let op = api.organization("acme").issue("hold").get();
    let holding = tokio::spawn(async move { op.send().await });
    assert!(
        server
            .wait_until(Duration::from_secs(1), |s| s.requests().len() == 1)
            .await
    );
    let op = api.organization("acme").issue("202").get();
    let mut waiting = tokio::spawn(async move { op.send().await });
    assert!(
        tokio::time::timeout(Duration::from_millis(20), &mut waiting)
            .await
            .is_err()
    );
    gate.open();
    let response = holding.await.unwrap().unwrap();
    assert_eq!(response.rates.remaining, Some(0));
    waiting.await.unwrap().unwrap();
    let times = dispatched.lock().unwrap();
    assert_eq!(times.len(), 2);
    assert!(times[1] >= UNIX_EPOCH + Duration::from_secs(reset));
}

#[tokio::test]
async fn local_frequency_and_endpoint_concurrency_are_independent() {
    let gate = Gate::new();
    let hold = gate.clone();
    let server = TestServer::start(move |r: &Recorded| {
        if r.path().ends_with("/202/") {
            Reply::json(200, ISSUE).after(hold.clone())
        } else {
            Reply::json(200, EVENT)
        }
    })
    .await;
    let settings = Settings {
        requests: NonZeroU32::new(1).unwrap(),
        window: Duration::from_millis(120),
        ..support::settings()
    };
    let api = support::api(&server, settings);
    let op = api.organization("acme").issue("202").get();
    let holding = tokio::spawn(async move { op.send().await });
    assert!(
        server
            .wait_until(Duration::from_secs(1), |s| s.requests().len() == 1)
            .await
    );
    let issue = api.organization("acme").issue("202");
    let epoch = Instant::now();
    issue.event("latest").send().await.unwrap();
    issue.event("oldest").send().await.unwrap();
    assert!(epoch.elapsed() >= Duration::from_millis(120));
    assert!(!holding.is_finished()); // a different endpoint has its own concurrency bucket
    gate.open();
    holding.await.unwrap().unwrap();
}

#[tokio::test]
async fn retry_after_is_a_minimum_and_deadline_does_not_shorten_it() {
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let server = TestServer::start(move |_: &Recorded| {
        if count.fetch_add(1, Ordering::SeqCst) == 0 {
            Reply::json(503, PROBLEM).header("retry-after", "1")
        } else {
            Reply::json(200, ISSUE)
        }
    })
    .await;
    let settings = Settings {
        max_delay: Duration::from_millis(1),
        ..retry_settings()
    };
    let start = Instant::now();
    let response = support::api(&server, settings)
        .organization("acme")
        .issue("202")
        .get()
        .send()
        .await
        .unwrap();
    assert_eq!(response.retry.retry.waits, [Duration::from_secs(1)]);
    assert!(start.elapsed() >= Duration::from_secs(1));
    assert_eq!(server.requests().len(), 2);

    let server =
        TestServer::start(|_: &Recorded| Reply::json(429, PROBLEM).header("retry-after", "1"))
            .await;
    let settings = Settings {
        timeout: Duration::from_millis(100),
        ..retry_settings()
    };
    let error = support::api(&server, settings)
        .organization("acme")
        .issue("202")
        .get()
        .send()
        .await
        .unwrap_err();
    let Error::RetryStopped { report, meta } = error else {
        panic!("{error:?}")
    };
    assert_eq!(format!("{:?}", report.retry.stop), "Deadline");
    assert_eq!(report.retry.attempts, 1);
    assert!(report.retry.waits.is_empty());
    assert_eq!(meta.unwrap().status().as_u16(), 429);
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn deadline_covers_queued_admission_and_cancellation_does_not_consume_slot() {
    let server = TestServer::start(|r: &Recorded| {
        if r.path().ends_with("/202/") {
            Reply::json(200, "").with_body(Body::Stall)
        } else {
            Reply::json(200, ISSUE)
        }
    })
    .await;
    let settings = Settings {
        timeout: Duration::from_millis(120),
        ..retry_settings()
    };
    let api = support::api(&server, settings);
    let op = api.organization("acme").issue("202").get();
    let first = tokio::spawn(async move { op.send().await });
    assert!(
        server
            .wait_until(Duration::from_secs(1), |s| s.active_bodies() == 1)
            .await
    );
    let error = api
        .organization("acme")
        .issue("203")
        .get()
        .send()
        .await
        .unwrap_err();
    let Error::RetryStopped { report, meta } = error else {
        panic!("{error:?}")
    };
    assert_eq!(report.retry.attempts, 0);
    assert!(meta.is_none());
    assert_eq!(server.requests().len(), 1);
    first.abort();
    assert!(first.await.unwrap_err().is_cancelled());
    api.organization("acme")
        .issue("204")
        .get()
        .send()
        .await
        .unwrap();
}

#[tokio::test]
async fn repeated_rate_limit_exhausts_attempt_budget_with_final_http_error() {
    let server =
        TestServer::start(|_: &Recorded| concurrency_headers(Reply::json(429, PROBLEM))).await;
    let error = support::api(&server, retry_settings())
        .organization("acme")
        .issue("202")
        .get()
        .send()
        .await
        .unwrap_err();
    let Error::Provider(provider) = error else {
        panic!("{error:?}")
    };
    assert_eq!(provider.class, HttpClass::RateLimited);
    assert_eq!(provider.retry.retry.attempts, 3);
    assert_eq!(
        provider.retry.retry.waits,
        [Duration::from_millis(50), Duration::from_millis(100)]
    );
    assert_eq!(format!("{:?}", provider.retry.retry.stop), "Attempts");
    assert_eq!(server.requests().len(), 3);
}

#[tokio::test]
async fn transient_headless_transport_and_gateway_failures_retry_but_auth_does_not() {
    for first_status in [0, 502, 503, 504, 401, 403, 404, 400, 500] {
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let server = TestServer::start(move |_: &Recorded| {
            if count.fetch_add(1, Ordering::SeqCst) == 0 {
                if first_status == 0 {
                    Reply::hang_up()
                } else {
                    Reply::json(first_status, PROBLEM)
                }
            } else {
                Reply::json(200, ISSUE)
            }
        })
        .await;
        let result = support::api(&server, retry_settings())
            .organization("acme")
            .issue("202")
            .get()
            .send()
            .await;
        if matches!(first_status, 0 | 502 | 503 | 504) {
            let response = result.unwrap();
            assert_eq!(response.retry.retry.attempts, 2);
            assert_eq!(response.retry.no_response, first_status == 0);
            assert_eq!(server.requests().len(), 2);
        } else {
            assert!(matches!(result, Err(Error::Provider(_))));
            assert_eq!(server.requests().len(), 1);
        }
    }
}

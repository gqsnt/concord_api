//! Issue status update regressions (Update an Issue); all data is synthetic, see PROVENANCE.md.
mod support;

use concord_sdk_sentry::{
    Error, HttpClass, Ignore, MAX_IGNORE_WINDOW_MINUTES, SendPolicy, Sentry, Settings,
    StatusUpdate,
    concord::{Delivery, ErrorKind, reqwest},
};
use concord_testserver::{Gate, Recorded, Reply, TestServer};
use serde_json::{Value, json};
use std::{
    num::NonZeroUsize,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const ISSUE: &str = include_str!("../fixtures/issue.json");
const IGNORED: &str = include_str!("../fixtures/ignored.json");
const PROBLEM: &str = include_str!("../fixtures/problem.json");

fn retry_settings() -> Settings {
    Settings {
        max_attempts: NonZeroUsize::new(3).unwrap(),
        ..support::settings()
    }
}
fn body(r: &Recorded) -> Value {
    serde_json::from_slice(&r.body).unwrap()
}
fn concurrency_headers(mut reply: Reply) -> Reply {
    for line in include_str!("../fixtures/concurrency.headers").lines() {
        let (name, value) = line.split_once(':').unwrap();
        reply = reply.header(name, value.trim());
    }
    reply
}

#[tokio::test]
async fn each_status_change_sends_only_its_documented_fields_and_returns_the_issue() {
    let server = TestServer::start(|r: &Recorded| {
        assert_eq!(r.method, "PUT");
        assert_eq!(
            r.target,
            "/api/0/organizations/acme%2Fa%20b/issues/202%2Fx/"
        );
        assert_eq!(
            r.header("authorization"),
            Some("Bearer synthetic-bearer-secret")
        );
        assert_eq!(r.header("content-type"), Some("application/json"));
        assert_eq!(r.header("accept"), Some("application/json"));
        // Synthetic echo: the fixture issue carries the status the request asked for.
        let requested = body(r);
        let status = match requested["status"].as_str().unwrap() {
            "ignored" => return Reply::json(200, IGNORED),
            "resolvedInNextRelease" | "resolved" => "resolved",
            other => other,
        };
        Reply::json(
            200,
            ISSUE.replace(
                r#""status": "resolved""#,
                &format!(r#""status": "{status}""#),
            ),
        )
    })
    .await;
    let issue = support::api(&server, support::settings())
        .organization("acme/a b")
        .issue("202/x");
    let cases = [
        (StatusUpdate::Unresolved, json!({"status": "unresolved"})),
        (StatusUpdate::Resolved, json!({"status": "resolved"})),
        (
            StatusUpdate::ResolvedInNextRelease,
            json!({"status": "resolvedInNextRelease"}),
        ),
        (
            StatusUpdate::Ignored(Ignore::FOREVER),
            json!({"status": "ignored"}),
        ),
        (
            StatusUpdate::Ignored(Ignore::for_minutes(30).unwrap()),
            json!({"status": "ignored", "ignoreDuration": 30}),
        ),
        (
            StatusUpdate::Ignored(Ignore::until_seen(25, None).unwrap()),
            json!({"status": "ignored", "ignoreCount": 25}),
        ),
        (
            StatusUpdate::Ignored(Ignore::until_seen(25, Some(90)).unwrap()),
            json!({"status": "ignored", "ignoreCount": 25, "ignoreWindow": 90}),
        ),
        (
            StatusUpdate::Ignored(Ignore::until_users(3, None).unwrap()),
            json!({"status": "ignored", "ignoreUserCount": 3}),
        ),
        (
            StatusUpdate::Ignored(Ignore::until_users(3, Some(10_080)).unwrap()),
            json!({"status": "ignored", "ignoreUserCount": 3, "ignoreUserWindow": 10_080}),
        ),
    ];
    for (change, _) in &cases {
        let op = issue.update_status(*change);
        assert!(!format!("{op:?}").contains("synthetic-bearer-secret"));
        let response = op.send().await.unwrap();
        let expected = match change {
            StatusUpdate::Unresolved => "unresolved",
            StatusUpdate::Ignored(_) => "ignored",
            _ => "resolved",
        };
        assert_eq!(response.value.id, "202");
        assert_eq!(response.value.status, expected);
        assert_eq!(response.next_cursor, None);
        assert_eq!(response.retry.retry.attempts, 1);
        assert_eq!(
            response.meta.operation().unwrap().name(),
            "Sentry.organization.issue.update_status"
        );
    }
    let requests = server.requests();
    assert_eq!(requests.len(), cases.len());
    for (request, (_, expected)) in requests.iter().zip(&cases) {
        assert_eq!(&body(request), expected, "exactly the requested change");
    }
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn details_outside_the_documented_set_are_refused_before_any_request() {
    let server = TestServer::start(|_: &Recorded| panic!("a refused change was sent")).await;
    let issue = support::api(&server, support::settings())
        .organization("acme")
        .issue("202");
    assert_eq!(MAX_IGNORE_WINDOW_MINUTES, 7 * 24 * 60);
    let over = MAX_IGNORE_WINDOW_MINUTES + 1;
    for (ignore, reason) in [
        (Ignore::for_minutes(0), "zero ignore duration"),
        (Ignore::until_seen(0, None), "zero ignore count"),
        (Ignore::until_seen(0, Some(60)), "zero ignore count"),
        (Ignore::until_seen(5, Some(0)), "zero ignore window"),
        (
            Ignore::until_seen(5, Some(over)),
            "ignore window exceeds 7 days",
        ),
        (
            Ignore::until_seen(5, Some(u32::MAX)),
            "ignore window exceeds 7 days",
        ),
        (Ignore::until_users(0, None), "zero ignore user count"),
        (Ignore::until_users(5, Some(0)), "zero ignore window"),
        (
            Ignore::until_users(5, Some(over)),
            "ignore window exceeds 7 days",
        ),
    ] {
        // Caller shape: the checked detail is built in the same expression as the send.
        let sent = async {
            issue
                .update_status(StatusUpdate::Ignored(ignore?))
                .send()
                .await
        };
        let error = sent.await.unwrap_err();
        let Error::InvalidUpdate(refused) = error else {
            panic!("{error:?}")
        };
        assert_eq!(refused, reason);
        assert_eq!(error.to_string(), format!("Sentry issue update: {reason}"));
    }
    assert!(server.requests().is_empty());
    // The documented maximum itself is accepted.
    assert!(Ignore::until_seen(5, Some(MAX_IGNORE_WINDOW_MINUTES)).is_ok());
    assert!(Ignore::until_users(5, Some(MAX_IGNORE_WINDOW_MINUTES)).is_ok());
}

#[tokio::test]
async fn an_update_without_an_answer_is_uncertain_and_never_retried() {
    // The connection closes after the request was read: the change may have been applied.
    let server = TestServer::start(|_: &Recorded| Reply::hang_up()).await;
    let error = support::api(&server, retry_settings())
        .organization("acme")
        .issue("202")
        .update_status(StatusUpdate::Resolved)
        .send()
        .await
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "Sentry issue update outcome unknown: re-read the issue"
    );
    let Error::Uncertain { transport, report } = error else {
        panic!("{error:?}")
    };
    let transport = transport.expect("client failure");
    assert_eq!(transport.kind(), ErrorKind::Transport);
    assert_eq!(transport.delivery(), Delivery::Unknown);
    assert!(report.no_response);
    assert_eq!((report.retry.attempts, report.completed), (1, 1));
    assert_eq!(server.requests().len(), 1, "never resent");

    // The transport's own timeout.
    let gate = Gate::new();
    let hold = gate.clone();
    let server =
        TestServer::start(move |_: &Recorded| Reply::json(200, ISSUE).after(hold.clone())).await;
    let http = reqwest::Client::builder()
        .timeout(Duration::from_millis(150))
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .no_proxy()
        .build()
        .unwrap();
    let error = Sentry::at(
        http,
        concord_sdk_sentry::Deployment::parse(server.url()).unwrap(),
        "synthetic-bearer-secret",
        SendPolicy::new(retry_settings()).unwrap(),
    )
    .unwrap()
    .organization("acme")
    .issue("202")
    .update_status(StatusUpdate::Resolved)
    .send()
    .await
    .unwrap_err();
    assert!(!format!("{error:?} {error}").contains("synthetic-bearer-secret"));
    let Error::Uncertain { transport, report } = error else {
        panic!("{error:?}")
    };
    assert!(transport.unwrap().transport().unwrap().is_timeout());
    assert_eq!(report.retry.attempts, 1);
    gate.open();
    assert_eq!(server.requests().len(), 1, "never resent");

    // The SDK's scheduling deadline expires while the update is in flight.
    let gate = Gate::new();
    let hold = gate.clone();
    let server = TestServer::start(move |r: &Recorded| match r.method.as_str() {
        "PUT" => Reply::json(200, ISSUE).after(hold.clone()),
        _ => Reply::json(200, ISSUE), // the caller's re-read
    })
    .await;
    let settings = Settings {
        timeout: Duration::from_millis(150),
        ..retry_settings()
    };
    let issue = support::api(&server, settings)
        .organization("acme")
        .issue("202");
    let error = issue
        .update_status(StatusUpdate::Resolved)
        .send()
        .await
        .unwrap_err();
    let Error::Uncertain { transport, report } = error else {
        panic!("{error:?}")
    };
    assert!(transport.is_none());
    assert_eq!(format!("{:?}", report.retry.stop), "Deadline");
    assert_eq!((report.retry.attempts, report.completed), (1, 0));
    assert!(report.no_response);
    gate.open();
    // The caller learns the outcome by reading the issue, not by resending.
    assert_eq!(issue.get().send().await.unwrap().value.status, "resolved");
    let methods: Vec<_> = server.requests().into_iter().map(|r| r.method).collect();
    assert_eq!(methods, ["PUT", "GET"]);
}

#[tokio::test]
async fn deadline_during_rate_limit_backoff_is_not_uncertain() {
    // 429 is a documented rejection: the change was not applied, so this is RetryStopped.
    let server =
        TestServer::start(|_: &Recorded| Reply::json(429, PROBLEM).header("retry-after", "5"))
            .await;
    let settings = Settings {
        timeout: Duration::from_millis(150),
        ..retry_settings()
    };
    let error = support::api(&server, settings)
        .organization("acme")
        .issue("202")
        .update_status(StatusUpdate::Unresolved)
        .send()
        .await
        .unwrap_err();
    let Error::RetryStopped { report, meta } = error else {
        panic!("{error:?}")
    };
    assert_eq!(format!("{:?}", report.retry.stop), "Deadline");
    assert!(!report.no_response);
    assert_eq!(meta.unwrap().status().as_u16(), 429);
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn only_429_is_retried_for_an_update() {
    for status in [500, 502, 503, 504] {
        let server = TestServer::start(move |_: &Recorded| Reply::json(status, PROBLEM)).await;
        let error = support::api(&server, retry_settings())
            .organization("acme")
            .issue("202")
            .update_status(StatusUpdate::Resolved)
            .send()
            .await
            .unwrap_err();
        let Error::Provider(provider) = error else {
            panic!("{status}: {error:?}")
        };
        assert_eq!(provider.class, HttpClass::Server);
        assert_eq!(provider.meta.status().as_u16(), status);
        assert_eq!(
            provider.document.unwrap().detail,
            "synthetic-provider-secret"
        );
        assert_eq!(provider.retry.retry.attempts, 1);
        assert_eq!(server.requests().len(), 1, "{status} was resent");
    }

    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let server = TestServer::start(move |_: &Recorded| {
        if count.fetch_add(1, Ordering::SeqCst) == 0 {
            Reply::json(429, PROBLEM).header("retry-after", "1")
        } else {
            Reply::json(200, ISSUE)
        }
    })
    .await;
    let start = Instant::now();
    let response = support::api(&server, retry_settings())
        .organization("acme")
        .issue("202")
        .update_status(StatusUpdate::Resolved)
        .send()
        .await
        .unwrap();
    assert!(start.elapsed() >= Duration::from_secs(1));
    assert_eq!(response.retry.retry.attempts, 2);
    assert_eq!(response.retry.retry.waits, [Duration::from_secs(1)]);
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(body(&requests[0]), body(&requests[1]), "same change resent");

    // An applied (2xx) update with invalid quota fields is reported, never resent.
    let server = TestServer::start(|_: &Recorded| {
        Reply::json(200, ISSUE).header("x-sentry-rate-limit-limit", "+5")
    })
    .await;
    let error = support::api(&server, retry_settings())
        .organization("acme")
        .issue("202")
        .update_status(StatusUpdate::Resolved)
        .send()
        .await
        .unwrap_err();
    assert!(
        matches!(&error, Error::RateMetadata { meta, .. } if meta.status() == 200),
        "{error:?}"
    );
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn update_retry_reacquires_concurrency_and_releases_lease_during_backoff() {
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
    let op = api
        .organization("acme")
        .issue("retry")
        .update_status(StatusUpdate::Resolved);
    let retry = tokio::spawn(async move { op.send().await });
    assert!(
        server
            .wait_until(Duration::from_secs(1), |s| s.requests().len() == 1)
            .await
    );
    // This update takes the lease released during the first update's backoff.
    let op = api
        .organization("another")
        .issue("blocker")
        .update_status(StatusUpdate::Resolved);
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
    // Its retry delay has expired, but the retry cannot dispatch while the lease is held.
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
async fn updates_share_one_endpoint_bucket_and_reads_keep_theirs() {
    let gate = Gate::new();
    let hold = gate.clone();
    let server = TestServer::start(move |r: &Recorded| {
        if r.method == "PUT" && r.path().contains("/hold/") {
            Reply::json(200, ISSUE).after(hold.clone())
        } else {
            Reply::json(200, ISSUE)
        }
    })
    .await;
    let api = support::api(&server, support::settings());
    let op = api
        .organization("acme")
        .issue("hold")
        .update_status(StatusUpdate::Resolved);
    let holding = tokio::spawn(async move { op.send().await });
    assert!(
        server
            .wait_until(Duration::from_secs(1), |s| s.requests().len() == 1)
            .await
    );
    // Retrieving the same issue uses its own endpoint bucket while the update holds a lease...
    api.organization("acme")
        .issue("hold")
        .get()
        .send()
        .await
        .unwrap();
    assert_eq!(server.requests()[1].method, "GET");
    // ...and another organization's update waits for the update endpoint's single lease.
    // (Queued last: the limiter's lease queue is FIFO across endpoints, see README.)
    let op = api
        .organization("other")
        .issue("202")
        .update_status(StatusUpdate::Unresolved);
    let waiting = tokio::spawn(async move { op.send().await });
    assert!(
        !server
            .wait_until(Duration::from_millis(100), |s| s.requests().len() > 2)
            .await
    );
    gate.open();
    holding.await.unwrap().unwrap();
    waiting.await.unwrap().unwrap();
    let methods: Vec<_> = server.requests().into_iter().map(|r| r.method).collect();
    assert_eq!(methods, ["PUT", "GET", "PUT"]);
}

#[tokio::test]
async fn update_frequency_observation_delays_the_next_update_until_reset() {
    let reset = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 2;
    let dispatched = Arc::new(Mutex::new(Vec::new()));
    let times = dispatched.clone();
    let server = TestServer::start(move |_: &Recorded| {
        times.lock().unwrap().push(SystemTime::now());
        Reply::json(200, ISSUE)
            .header("x-sentry-rate-limit-limit", "10")
            .header("x-sentry-rate-limit-remaining", "0")
            .header("x-sentry-rate-limit-reset", reset.to_string())
    })
    .await;
    let issue = support::api(&server, support::settings())
        .organization("acme")
        .issue("202");
    let first = issue
        .update_status(StatusUpdate::Resolved)
        .send()
        .await
        .unwrap();
    assert_eq!(
        (first.rates.remaining, first.rates.reset),
        (Some(0), Some(reset))
    );
    issue
        .update_status(StatusUpdate::Unresolved)
        .send()
        .await
        .unwrap();
    let times = dispatched.lock().unwrap();
    assert_eq!(times.len(), 2);
    assert!(times[1] >= UNIX_EPOCH + Duration::from_secs(reset));
}

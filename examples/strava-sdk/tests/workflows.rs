//! Connected workflows and failure recovery, exclusively against local fixtures.
mod support;
use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_strava::{
    concord,
    pagination::{PagesError, Resume},
    *,
};
use std::{
    num::NonZeroUsize,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use support::*;
fn nz(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).unwrap()
}
fn polls(count: usize) -> PollPolicy {
    PollPolicy {
        max_polls: nz(count),
        interval: Duration::from_secs(1),
    }
}

#[tokio::test]
async fn expired_oauth_refresh_then_all_pages_then_activity_detail() {
    let server = TestServer::start(|r: &Recorded| match r.path() {
        "/oauth/token" => {
            assert_eq!(r.method, "POST");
            assert_eq!(
                r.form_pairs(),
                [
                    ("client_id".into(), "123".into()),
                    ("client_secret".into(), "CLIENT_SECRET".into()),
                    ("grant_type".into(), "refresh_token".into()),
                    ("refresh_token".into(), "REFRESH_SECRET_OLD".into())
                ]
            );
            assert!(r.header("authorization").is_none());
            Reply::json(200, TOKENS)
        }
        "/api/v3/athlete/activities" => {
            assert_eq!(r.header("authorization"), Some("Bearer ACCESS_SECRET_A"));
            assert_eq!(r.query_all("per_page"), ["2"]);
            Reply::json(
                200,
                match r.query_all("page")[0].as_str() {
                    "1" => FIRST,
                    "2" => SECOND,
                    "3" => EMPTY,
                    _ => panic!("unexpected page"),
                },
            )
        }
        "/api/v3/activities/9007199254740999" => Reply::json(200, ACTIVITY),
        _ => panic!("unexpected workflow route"),
    })
    .await;
    let session = session(&server.url(), EXPIRED, Duration::from_secs(3));
    let api = session.client();
    let mut pages = api.athlete().pages(2, Resume::Start, nz(5)).unwrap();
    let mut activities = Vec::new();
    while let Some(page) = pages.next_page().await.unwrap() {
        activities.extend(page.items);
    }
    assert_eq!(
        activities.iter().map(|a| a.id).collect::<Vec<_>>(),
        [9007199254740999, 52, 53]
    );
    let detail = api.activities().get(activities[0].id).send().await.unwrap();
    assert_eq!(detail.value.name, "Synthetic canal ride");
    assert_eq!(
        session.latest_tokens().await.refresh_token,
        "REFRESH_SECRET_A"
    );
    assert_eq!(
        server
            .requests()
            .iter()
            .filter(|r| r.path() == "/oauth/token")
            .count(),
        1
    );
    assert_eq!(server.requests().len(), 5);
    safe(&session);
    safe(session.latest_tokens().await);
    server.shutdown().await;
}

#[tokio::test]
async fn pagination_cap_and_provider_failure_keep_resume_and_prefix() {
    let server = TestServer::start(|r: &Recorded| {
        Reply::json(
            200,
            match r.query_all("page")[0].as_str() {
                "1" => FIRST,
                "2" => SECOND,
                "3" => EMPTY,
                _ => panic!("page"),
            },
        )
    })
    .await;
    let api = api(&server.url());
    let mut walk = api.athlete().pages(2, Resume::Start, nz(1)).unwrap();
    assert_eq!(walk.next_page().await.unwrap().unwrap().items.len(), 2);
    let Err(PagesError::Bounded { resume, .. }) = walk.next_page().await else {
        panic!("expected cap");
    };
    assert_eq!(resume, Resume::At(2));
    assert!(matches!(
        walk.next_page().await,
        Err(PagesError::Ended { .. })
    ));
    let rest = api
        .athlete()
        .pages(2, resume, nz(4))
        .unwrap()
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(rest.len(), 1);
    assert_eq!(rest[0].id, 53);
    server.shutdown().await;
    let server = TestServer::start(|r: &Recorded| {
        if r.query_all("page")[0] == "1" {
            Reply::json(200, FIRST)
        } else {
            Reply::json(403, PROBLEM)
        }
    })
    .await;
    let incomplete = api_for(&server)
        .athlete()
        .pages(2, Resume::Start, nz(4))
        .unwrap()
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    // The shared facade retains the prefix and the SDK's typed error without declaring completion.
    assert_eq!(incomplete.collected.len(), 2);
    let error = incomplete.error.provider_error().unwrap();
    assert_eq!(error.status(), Some(403));
    safe_error(error);
    server.shutdown().await;
}
fn api_for(server: &TestServer) -> Strava {
    support::api(&server.url())
}

#[tokio::test]
async fn upload_file_then_poll_its_id_until_activity_exists() {
    let steps = Arc::new(AtomicUsize::new(0));
    let seen = steps.clone();
    let last_poll = Arc::new(std::sync::Mutex::new(None::<Instant>));
    let times = last_poll.clone();
    let server = TestServer::start(move |r: &Recorded| {
        assert_eq!(r.header("authorization"), Some("Bearer ACCESS_SECRET_A"));
        match seen.fetch_add(1, Ordering::SeqCst) {
            0 => {
                assert_eq!(r.method, "POST");
                assert_eq!(r.target, "/api/v3/uploads");
                Reply::json(201, RECEIPT)
            }
            step @ (1 | 2) => {
                assert_eq!(r.method, "GET");
                assert_eq!(r.target, "/api/v3/uploads/9007199254740997");
                let now = Instant::now();
                if let Some(last) = times.lock().unwrap().replace(now) {
                    assert!(now.duration_since(last) >= Duration::from_secs(1));
                }
                Reply::json(200, if step == 1 { PENDING } else { READY })
            }
            3 => {
                assert_eq!(r.path(), "/api/v3/activities/9007199254740999");
                Reply::json(200, ACTIVITY)
            }
            _ => panic!("unexpected replay"),
        }
    })
    .await;
    let api = api(&server.url());
    let complete = api
        .uploads()
        .upload_and_poll(file(), polls(4))
        .await
        .unwrap();
    assert_eq!(complete.id, 9007199254740997);
    let detail = api
        .activities()
        .get(complete.activity_id.unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(detail.value.id, 9007199254740999);
    assert_eq!(
        server
            .requests()
            .iter()
            .filter(|r| r.method == "POST")
            .count(),
        1
    );
    assert_eq!(server.requests().len(), 4);
    server.shutdown().await;
}

#[tokio::test]
async fn upload_timeout_is_ambiguous_and_never_replays_one_shot_source() {
    let count = Arc::new(AtomicUsize::new(0));
    let polled = count.clone();
    let body = concord::reqwest::Body::wrap_stream(futures_util::stream::once(async move {
        polled.fetch_add(1, Ordering::SeqCst);
        Ok::<_, std::io::Error>(TRACK.to_vec())
    }));
    let server = TestServer::start(|r: &Recorded| {
        assert_eq!(r.target, "/api/v3/uploads");
        assert!(r.body.windows(TRACK.len()).any(|w| w == TRACK));
        // The entire file has reached the provider. Withhold the receipt past the client deadline.
        Reply::json(201, RECEIPT).delay(Duration::from_secs(1))
    })
    .await;
    let api = session(&server.url(), TOKENS, Duration::from_millis(100)).client();
    let file = UploadFile::new("one-shot.gpx", DataType::Gpx, body).unwrap();
    let error = api
        .uploads()
        .upload_and_poll(file, polls(2))
        .await
        .unwrap_err();
    assert!(error.is_ambiguous());
    safe_error(&error);
    let Error::Ambiguous(inner) = error else {
        panic!("unknown upload delivery");
    };
    assert_eq!(inner.delivery(), concord::Delivery::Unknown);
    assert!(inner.transport().unwrap().is_timeout());
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert_eq!(server.requests().len(), 1); // No poll and no automatic file replay.
    assert_eq!(server.connections(), 1);
    server.shutdown().await;
}

#[tokio::test]
async fn processing_failure_and_poll_bound_can_be_inspected_without_resubmission() {
    let server = TestServer::start(|r: &Recorded| {
        if r.method == "POST" {
            Reply::json(201, RECEIPT)
        } else {
            Reply::json(200, FAILED)
        }
    })
    .await;
    let error = api(&server.url())
        .uploads()
        .upload_and_poll(file(), polls(2))
        .await
        .unwrap_err();
    safe_error(&error);
    let Error::Processing(failure) = error else {
        panic!("processing failure");
    };
    assert!(failure.error.unwrap().contains("synthetic duplicate"));
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;

    let server = TestServer::start(|_: &Recorded| Reply::json(200, PENDING)).await;
    let api = api(&server.url());
    let error = api
        .uploads()
        .poll(serde_json::from_str(RECEIPT).unwrap(), polls(1))
        .await
        .unwrap_err();
    safe_error(&error);
    let Error::PollBounded(receipt) = error else {
        panic!("poll bound");
    };
    assert_eq!(receipt.id, 9007199254740997);
    assert_eq!(server.requests().len(), 1);
    assert_eq!(server.single().method, "GET");
    // The retained receipt is usable for another bounded poll pass, without another upload.
    assert!(matches!(
        api.uploads().poll(*receipt, polls(1)).await,
        Err(Error::PollBounded(_))
    ));
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

#[tokio::test]
async fn polling_validates_interval_receipt_identity_and_terminal_receipts() {
    let server = TestServer::start(|_: &Recorded| {
        Reply::json(200, "{\"id\":7,\"status\":\"Synthetic mismatch\"}")
    })
    .await;
    let api = api(&server.url());
    let error = api
        .uploads()
        .upload_and_poll(
            file(),
            PollPolicy {
                interval: Duration::ZERO,
                ..polls(1)
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Invalid(_)));
    assert!(server.requests().is_empty());
    let completed = api
        .uploads()
        .poll(serde_json::from_str(READY).unwrap(), polls(1))
        .await
        .unwrap();
    assert!(completed.activity_id.is_some());
    assert!(server.requests().is_empty());
    let error = api
        .uploads()
        .poll(serde_json::from_str(RECEIPT).unwrap(), polls(1))
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        Error::Invalid("upload identifier changed while polling")
    ));
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

#[tokio::test]
async fn concurrent_calls_share_one_refresh_and_next_refresh_uses_rotated_token() {
    let grants = Arc::new(AtomicUsize::new(0));
    let count = grants.clone();
    let server = TestServer::start(move |r: &Recorded| {
        if r.path() == "/oauth/token" {
            let grant = count.fetch_add(1, Ordering::SeqCst);
            let pairs = r.form_pairs();
            assert_eq!(
                pairs.last().unwrap().1,
                if grant == 0 {
                    "REFRESH_SECRET_OLD"
                } else {
                    "REFRESH_SECRET_A"
                }
            );
            Reply::json(200, if grant == 0 { TOKENS } else { ROTATED })
                .delay(Duration::from_millis(30))
        } else if r.path() == "/api/v3/uploads" {
            assert_eq!(r.header("authorization"), Some("Bearer ACCESS_SECRET_A"));
            Reply::json(401, PROBLEM)
        } else {
            Reply::json(200, ACTIVITY)
        }
    })
    .await;
    let session = session(&server.url(), EXPIRED, Duration::from_secs(3));
    let api = session.client();
    let (first, second) = tokio::join!(
        api.activities().get(1).send(),
        api.activities().get(2).send()
    );
    first.unwrap();
    second.unwrap();
    assert_eq!(grants.load(Ordering::SeqCst), 1);
    let error = api.uploads().create(file()).send().await.unwrap_err();
    assert_eq!(error.status(), Some(401));
    assert!(!error.is_ambiguous());
    assert_eq!(
        server
            .requests()
            .iter()
            .filter(|r| r.path() == "/api/v3/uploads")
            .count(),
        1
    );
    api.activities().get(3).send().await.unwrap();
    assert_eq!(grants.load(Ordering::SeqCst), 2);
    assert_eq!(
        session.latest_tokens().await.refresh_token,
        "REFRESH_SECRET_B"
    );
    assert!(
        server
            .requests()
            .iter()
            .any(|r| r.header("authorization") == Some("Bearer ACCESS_SECRET_B"))
    );
    server.shutdown().await;
}

#[tokio::test]
async fn failed_refresh_never_dispatches_application_or_retries_grant() {
    for case in 0..3 {
        let server = TestServer::start(move |r: &Recorded| {
            assert_eq!(r.path(), "/oauth/token");
            match case {
                0 => Reply::json(400, PROBLEM),
                1 => Reply::json(200, INVALID),
                _ => Reply::json(200, TOKENS.replace("21600", "0")),
            }
        })
        .await;
        let error = session(&server.url(), EXPIRED, Duration::from_secs(2))
            .client()
            .activities()
            .get(7)
            .send()
            .await
            .unwrap_err();
        safe_error(&error);
        assert!(matches!(error, Error::Auth(_)));
        assert_eq!(
            error.status(),
            if case == 2 {
                None
            } else {
                Some(if case == 0 { 400 } else { 200 })
            }
        );
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn read_quota_exhaustion_still_allows_upload_but_daily_overall_exhaustion_does_not() {
    let server = TestServer::start(|r: &Recorded| {
        if r.method == "POST" {
            Reply::json(201, RECEIPT)
        } else {
            Reply::json(429, PROBLEM)
                .header("x-ratelimit-limit", "200,2000")
                .header("x-ratelimit-usage", "20,100")
                .header("x-readratelimit-limit", "100,1000")
                .header("x-readratelimit-usage", "100,900")
        }
    })
    .await;
    let api = api(&server.url());
    assert_eq!(
        api.activities().get(7).send().await.unwrap_err().status(),
        Some(429)
    );
    api.uploads().create(file()).send().await.unwrap();
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
    let server = TestServer::start(|_: &Recorded| {
        Reply::json(429, PROBLEM)
            .header("x-ratelimit-limit", "200,2000")
            .header("x-ratelimit-usage", "1,2000")
    })
    .await;
    let api = api_for(&server);
    api.activities().get(7).send().await.unwrap_err();
    assert!(matches!(
        api.uploads().create(file()).send().await,
        Err(Error::Admission(_))
    ));
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

#[tokio::test]
async fn status_retry_preserves_the_one_second_poll_interval() {
    let seen = Arc::new(std::sync::Mutex::new(None::<Instant>));
    let recorded = seen.clone();
    let server = TestServer::start(move |r: &Recorded| {
        assert_eq!(r.method, "GET");
        let mut previous = recorded.lock().unwrap();
        let now = Instant::now();
        if let Some(last) = previous.replace(now) {
            assert!(now.duration_since(last) >= Duration::from_secs(1));
            Reply::json(200, READY)
        } else {
            Reply::json(503, PROBLEM)
        }
    })
    .await;
    let status = api(&server.url())
        .uploads()
        .poll(serde_json::from_str(RECEIPT).unwrap(), polls(1))
        .await
        .unwrap();
    assert!(status.activity_id.is_some());
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

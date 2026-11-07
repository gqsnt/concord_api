//! Fresh-consumer application for SDK37 (Strava): two caller tasks against the loopback server.
//!
//! Workflow 1: authorize (code exchange), page the athlete's activities, retrieve one activity.
//! Workflow 2: upload a one-shot activity file, poll the returned upload id, read the activity.
//! Live provider calls: NOT_RUN. Every request goes to `examples/testserver` on 127.0.0.1.

use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_strava::concord::{ErrorKind, FieldError, reqwest};
use concord_sdk_strava::pagination::{PagesError, Resume};
use concord_sdk_strava::{
    Credentials, DataType, Error, ErrorDocument, OAuth, PollPolicy, Quota, Session, TokenRequest,
    Tokens, UploadFile, Window, transport,
};
use futures_util::stream;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

type Res = Result<(), Box<dyn std::error::Error + Send + Sync>>;

const CLIENT_ID: u64 = 104_233;
const CLIENT_SECRET: &str = "5f2c9e0d1b7a4c3e8f6a2d9b0c1e7f4a3b5d8c2e";
const ACCESS: &str = "a4b9456877c1e2d3f4a5b6c7d8e9f0a1b2c3d4e5";
const REFRESH: &str = "e5n567567f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c";

const GPX: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" creator="FirstUse Bike Computer" xmlns="http://www.topografix.com/GPX/1/1">
 <trk><name>Morning Ride</name><trkseg>
  <trkpt lat="45.4642" lon="9.1900"><ele>122.0</ele><time>2026-11-07T06:00:00Z</time></trkpt>
  <trkpt lat="45.4650" lon="9.1912"><ele>122.4</ele><time>2026-11-07T06:00:05Z</time></trkpt>
 </trkseg></trk>
</gpx>
"#;

// ---------------------------------------------------------------- helpers (application side)

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn stored_tokens(access: &str, refresh: &str, expires_at: u64, expires_in: u64) -> Tokens {
    Tokens {
        token_type: "Bearer".into(),
        access_token: access.into(),
        refresh_token: refresh.into(),
        expires_at,
        expires_in,
        scope: Some("read activity:read_all activity:write".into()),
    }
}

fn credentials() -> Credentials {
    Credentials::new(CLIENT_ID, CLIENT_SECRET.into()).unwrap()
}

fn connect(server: &TestServer, tokens: Tokens, timeout: Duration) -> Session {
    Session::connect(&server.url(), &server.url(), credentials(), tokens, timeout).unwrap()
}

fn fresh_session(server: &TestServer) -> Session {
    connect(
        server,
        stored_tokens(ACCESS, REFRESH, now() + 21_600, 21_600),
        Duration::from_secs(5),
    )
}

fn token_json(access: &str, refresh: &str) -> String {
    format!(
        r#"{{"token_type":"Bearer","expires_at":{},"expires_in":21600,"refresh_token":"{refresh}","access_token":"{access}","athlete":{{"id":134815,"resource_state":2}},"scope":"read activity:read_all activity:write"}}"#,
        now() + 21_600
    )
}

fn activity_json(id: u64, name: &str, distance: f64, moving_time: u64, sport: &str) -> String {
    format!(
        r#"{{"resource_state":2,"athlete":{{"id":134815,"resource_state":1}},"id":{id},"name":"{name}","distance":{distance},"moving_time":{moving_time},"elapsed_time":{},"total_elevation_gain":12.5,"type":"{sport}","sport_type":"{sport}","start_date":"2026-11-01T06:00:00Z","private":false}}"#,
        moving_time + 120
    )
}

/// Strava's four documented usage headers: "15-minute,daily".
fn rated(reply: Reply, limit: &str, usage: &str, read_limit: &str, read_usage: &str) -> Reply {
    reply
        .header("x-ratelimit-limit", limit)
        .header("x-ratelimit-usage", usage)
        .header("x-readratelimit-limit", read_limit)
        .header("x-readratelimit-usage", read_usage)
}

fn ok_rates(reply: Reply) -> Reply {
    rated(reply, "200,2000", "12,340", "100,1000", "7,210")
}

fn quota(short_limit: u32, short_used: u32, daily_limit: u32, daily_used: u32) -> Quota {
    Quota {
        short: Window {
            limit: short_limit,
            used: short_used,
        },
        daily: Window {
            limit: daily_limit,
            used: daily_used,
        },
    }
}

fn problem(message: &str, resource: &str, field: &str, code: &str) -> String {
    format!(
        r#"{{"message":"{message}","errors":[{{"resource":"{resource}","field":"{field}","code":"{code}"}}]}}"#
    )
}

fn upload_status(id: u64, error: Option<&str>, status: &str, activity_id: Option<u64>) -> String {
    let error = error.map_or("null".to_owned(), |e| format!("\"{e}\""));
    let activity = activity_id.map_or("null".to_owned(), |a| a.to_string());
    format!(
        r#"{{"id":{id},"id_str":"{id}","external_id":"morning-ride.gpx","error":{error},"status":"{status}","activity_id":{activity}}}"#
    )
}

/// A one-shot source as an application would hand over an already opened file.
fn gpx_body() -> reqwest::Body {
    let chunks: Vec<Result<Vec<u8>, std::io::Error>> =
        GPX.as_bytes().chunks(64).map(|c| Ok(c.to_vec())).collect();
    reqwest::Body::wrap_stream(stream::iter(chunks))
}

fn morning_ride() -> UploadFile {
    UploadFile::new("morning-ride.gpx", DataType::Gpx, gpx_body())
        .unwrap()
        .external_id("bikecomputer-2026-11-07-0600".into())
        .name("Morning Ride".into())
}

fn policy(max_polls: usize) -> PollPolicy {
    PollPolicy {
        max_polls: NonZeroUsize::new(max_polls).unwrap(),
        interval: Duration::from_secs(1),
    }
}

fn assert_api_headers(r: &Recorded, bearer: &str) {
    assert_eq!(
        r.header("authorization"),
        Some(format!("Bearer {bearer}").as_str())
    );
    assert_eq!(r.header("accept"), Some("application/json"));
    assert!(r.header_all("referer").is_empty());
}

fn http(err: &Error) -> &concord_sdk_strava::HttpFailure {
    match err {
        Error::Http(failure) => failure,
        other => panic!("expected Error::Http, got {other:?}"),
    }
}

// ---------------------------------------------------------------- first working call

#[tokio::test]
async fn first_working_call() -> Res {
    let server = TestServer::start(|_r: &Recorded| {
        Reply::json(
            200,
            activity_json(16_123_456_789, "Morning Ride", 28_099.6, 4_207, "Ride"),
        )
    })
    .await;
    let session = fresh_session(&server);
    let got = session
        .client()
        .activities()
        .get(16_123_456_789u64)
        .send()
        .await?;
    assert_eq!(got.value.id, 16_123_456_789);
    let r = server.single();
    assert_eq!(r.method, "GET");
    assert_eq!(
        r.target,
        "/api/v3/activities/16123456789?include_all_efforts=false"
    );
    assert_api_headers(&r, ACCESS);
    Ok(())
}

// ---------------------------------------------------------------- workflow 1

/// The ordinary path: validated callback code → tokens → session → pages → one activity.
#[tokio::test]
async fn w1_authorize_page_activities_then_get_one() -> Res {
    let server = TestServer::start(|r: &Recorded| match (r.method.as_str(), r.path()) {
        ("POST", "/oauth/token") => Reply::json(200, token_json(ACCESS, REFRESH)),
        ("GET", "/api/v3/athlete/activities") => {
            let body = match r.query_all("page").first().map(String::as_str) {
                Some("1") => format!(
                    "[{},{}]",
                    activity_json(12_000_000_003, "Evening Run", 10_012.3, 2_890, "Run"),
                    activity_json(
                        12_000_000_002,
                        "Lunch Gravel",
                        42_250.0,
                        6_101,
                        "GravelRide"
                    ),
                ),
                Some("2") => format!(
                    "[{}]",
                    activity_json(12_000_000_001, "Recovery Spin", 15_400.8, 2_400, "Ride")
                ),
                _ => "[]".to_owned(),
            };
            ok_rates(Reply::json(200, body))
        }
        ("GET", "/api/v3/activities/12000000002") => ok_rates(Reply::json(
            200,
            activity_json(
                12_000_000_002,
                "Lunch Gravel",
                42_250.0,
                6_101,
                "GravelRide",
            ),
        )),
        _ => Reply::json(
            404,
            problem("Record Not Found", "Activity", "id", "not found"),
        ),
    })
    .await;

    // 1. The application validated `state` and the accepted `scope` on the callback; exchange the code.
    let credentials = credentials();
    let oauth = OAuth::new(transport(Duration::from_secs(5))?).with_base_url(server.url())?;
    let grant =
        TokenRequest::authorization_code(&credentials, "c0de-from-validated-callback".into())?;
    let exchanged = oauth.oauth().token(grant).send().await?;
    assert_eq!(exchanged.attempts, 1);
    let tokens = exchanged.value;
    let scope = tokens.scope.clone().unwrap_or_default();
    assert!(
        scope
            .split([' ', ','])
            .any(|s| s == "activity:read_all" || s == "activity:read")
    );

    // 2. Session for this athlete; page their activities with a small page size and a cap.
    let session = Session::connect(
        &server.url(),
        &server.url(),
        credentials,
        tokens,
        Duration::from_secs(5),
    )?;
    let api = session.client();
    let mut pages = api
        .athlete()
        .pages(2, Resume::Start, NonZeroUsize::new(10).unwrap())?;
    let mut sizes = Vec::new();
    let mut activities = Vec::new();
    while let Some(page) = pages.next_page().await? {
        sizes.push(page.items.len());
        assert_eq!(page.meta.attempts, 1);
        assert_eq!(page.meta.meta.status().as_u16(), 200);
        assert_eq!(page.meta.rates.overall, Ok(Some(quota(200, 12, 2000, 340))));
        assert_eq!(page.meta.rates.read, Ok(Some(quota(100, 7, 1000, 210))));
        activities.extend(page.items);
    }
    // Full page, short page, then the empty page that ends the listing.
    assert_eq!(sizes, [2, 1, 0]);
    assert!(pages.next_page().await?.is_none(), "end is sticky");
    let ids: Vec<u64> = activities.iter().map(|a| a.id).collect();
    assert_eq!(ids, [12_000_000_003, 12_000_000_002, 12_000_000_001]);
    assert_eq!(activities[1].sport_type, "GravelRide");

    // 3. Retrieve one of them.
    let detail = api.activities().get(activities[1].id).send().await?;
    assert_eq!(detail.value, activities[1]);
    assert_eq!(detail.attempts, 1);
    assert_eq!(detail.rates.read, Ok(Some(quota(100, 7, 1000, 210))));

    // Wire: token exchange in a form body, never in the URL, without a bearer.
    let reqs = server.requests();
    assert_eq!(reqs.len(), 5);
    let token = &reqs[0];
    assert_eq!(
        (token.method.as_str(), token.target.as_str()),
        ("POST", "/oauth/token")
    );
    assert_eq!(
        token.header("content-type"),
        Some("application/x-www-form-urlencoded")
    );
    assert!(token.header_all("authorization").is_empty());
    let mut form = token.form_pairs();
    form.sort();
    assert_eq!(
        form,
        [
            ("client_id".to_owned(), CLIENT_ID.to_string()),
            ("client_secret".to_owned(), CLIENT_SECRET.to_owned()),
            ("code".to_owned(), "c0de-from-validated-callback".to_owned()),
            ("grant_type".to_owned(), "authorization_code".to_owned()),
        ]
    );
    for (n, r) in reqs[1..4].iter().enumerate() {
        assert_eq!(r.method, "GET");
        assert_eq!(
            r.target,
            format!("/api/v3/athlete/activities?page={}&per_page=2", n + 1)
        );
        assert_api_headers(r, ACCESS);
    }
    assert_eq!(
        reqs[4].target,
        "/api/v3/activities/12000000002?include_all_efforts=false"
    );
    assert_api_headers(&reqs[4], ACCESS);

    // Secrets stay out of Debug output.
    let shown = format!(
        "{:?} {:?} {:?} {:?}",
        session.latest_tokens().await,
        session,
        credentials_debug(),
        detail
    );
    for secret in [ACCESS, REFRESH, CLIENT_SECRET, "104233"] {
        assert!(!shown.contains(secret), "{secret} leaked in {shown}");
    }
    Ok(())
}

fn credentials_debug() -> String {
    format!("{:?}", credentials())
}

/// A token row reloaded from storage hours later: `expires_at` is past, `expires_in` is stale.
#[tokio::test]
async fn w1_reloaded_expired_tokens_refresh_before_the_first_page() -> Res {
    let server = TestServer::start(|r: &Recorded| match (r.method.as_str(), r.path()) {
        ("POST", "/oauth/token") => {
            Reply::json(200, token_json("access-rotated-2", "refresh-rotated-2"))
        }
        _ => ok_rates(Reply::json(200, "[]")),
    })
    .await;
    let reloaded = stored_tokens(ACCESS, REFRESH, now() - 3_600, 21_600);
    let session = connect(&server, reloaded, Duration::from_secs(5));
    let mut pages =
        session
            .client()
            .athlete()
            .pages(30, Resume::Start, NonZeroUsize::new(3).unwrap())?;
    let first = pages
        .next_page()
        .await?
        .expect("the empty first page is still a page");
    assert!(first.items.is_empty());
    assert!(pages.next_page().await?.is_none());

    let reqs = server.requests();
    assert_eq!(reqs.len(), 2, "one refresh, one page");
    let mut form = reqs[0].form_pairs();
    form.sort();
    assert_eq!(
        form,
        [
            ("client_id".to_owned(), CLIENT_ID.to_string()),
            ("client_secret".to_owned(), CLIENT_SECRET.to_owned()),
            ("grant_type".to_owned(), "refresh_token".to_owned()),
            ("refresh_token".to_owned(), REFRESH.to_owned()),
        ]
    );
    assert_eq!(
        reqs[1].target,
        "/api/v3/athlete/activities?page=1&per_page=30"
    );
    assert_api_headers(&reqs[1], "access-rotated-2");
    // The application must persist the rotated refresh token.
    let latest = session.latest_tokens().await;
    assert_eq!(latest.refresh_token, "refresh-rotated-2");
    assert_eq!(latest.access_token, "access-rotated-2");
    Ok(())
}

/// A cap on a full last page ends with a resume position; resuming continues the listing.
#[tokio::test]
async fn w1_page_cap_reports_resume_and_resumes() -> Res {
    let server = TestServer::start(|r: &Recorded| {
        let page: u64 = r.query_all("page")[0].parse().unwrap();
        let body = if page <= 2 {
            format!(
                "[{}]",
                activity_json(12_000_000_000 + page, "Ride", 1_000.0, 300, "Ride")
            )
        } else {
            "[]".into()
        };
        ok_rates(Reply::json(200, body))
    })
    .await;
    let api = fresh_session(&server).client();
    let mut pages = api
        .athlete()
        .pages(1, Resume::Start, NonZeroUsize::new(1).unwrap())?;
    assert_eq!(
        pages.next_page().await?.unwrap().items[0].id,
        12_000_000_001
    );
    let resume = match pages.next_page().await {
        Err(PagesError::Bounded { resume, .. }) => resume,
        other => panic!("expected Bounded, got {other:?}"),
    };
    assert_eq!(resume, Resume::At(2));
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Ended { .. })
    ));
    assert_eq!(
        server.requests().len(),
        1,
        "the cap is checked before sending page 2"
    );

    // Later (same athlete, filters and page size): continue from the reported page.
    let rest = api
        .athlete()
        .pages(1, resume, NonZeroUsize::new(5).unwrap())?
        .collect_complete()
        .await?;
    let ids: Vec<u64> = rest
        .iter()
        .flat_map(|p| p.items.iter().map(|a| a.id))
        .collect();
    assert_eq!(ids, [12_000_000_002]);
    let targets: Vec<String> = server.requests().iter().map(|r| r.target.clone()).collect();
    assert_eq!(
        targets,
        [
            "/api/v3/athlete/activities?page=1&per_page=1",
            "/api/v3/athlete/activities?page=2&per_page=1",
            "/api/v3/athlete/activities?page=3&per_page=1",
        ]
    );
    Ok(())
}

/// A provider failure keeps the collected prefix; 500 is not retried, 502 is retried once.
#[tokio::test]
async fn w1_failed_page_keeps_prefix_and_transient_page_is_retried() -> Res {
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let server = TestServer::start(move |r: &Recorded| {
        let n = seen.fetch_add(1, Ordering::SeqCst);
        match (
            r.query_all("per_page")[0].as_str(),
            r.query_all("page")[0].as_str(),
        ) {
            // Traversal A (per_page=2): page 2 fails hard.
            ("2", "1") => ok_rates(Reply::json(
                200,
                format!(
                    "[{},{}]",
                    activity_json(12_000_000_010, "A", 1.0, 1, "Run"),
                    activity_json(12_000_000_009, "B", 1.0, 1, "Run")
                ),
            )),
            ("2", _) => ok_rates(Reply::json(
                500,
                problem("Internal Server Error", "Application", "", "error"),
            )),
            // Traversal B (per_page=3): the first attempt of page 1 is a 502.
            ("3", "1") if n == 2 => Reply::json(502, "<html>Bad Gateway</html>"),
            ("3", "1") => ok_rates(Reply::json(
                200,
                format!("[{}]", activity_json(12_000_000_011, "C", 1.0, 1, "Ride")),
            )),
            _ => ok_rates(Reply::json(200, "[]")),
        }
    })
    .await;
    let api = fresh_session(&server).client();

    let incomplete = api
        .athlete()
        .pages(2, Resume::Start, NonZeroUsize::new(10).unwrap())?
        .collect_complete()
        .await
        .expect_err("page 2 failed");
    assert_eq!(incomplete.collected.len(), 1);
    assert_eq!(incomplete.collected[0].items.len(), 2);
    assert_eq!(incomplete.error.resume(), Some(&Resume::At(2)));
    let failure = incomplete.error.provider_error().expect("provider failure");
    assert_eq!(failure.status(), Some(500));
    let page_failure = http(failure);
    assert!(!page_failure.ambiguous);
    match &page_failure.document {
        Ok(ErrorDocument::Problem(p)) => assert_eq!(p.message, "Internal Server Error"),
        other => panic!("expected a problem document, got {other:?}"),
    }
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "a 500 on a read is not retried"
    );

    let mut pages = api
        .athlete()
        .pages(3, Resume::Start, NonZeroUsize::new(10).unwrap())?;
    let page = pages.next_page().await?.unwrap();
    assert_eq!(page.items[0].id, 12_000_000_011);
    assert_eq!(page.meta.attempts, 2, "one 502, one success");
    // A short page is not the end: the listing ends at the following empty page.
    assert!(pages.next_page().await?.unwrap().items.is_empty());
    assert!(pages.next_page().await?.is_none());
    Ok(())
}

/// Detail failures as an application sees them.
#[tokio::test]
async fn w1_detail_failures() -> Res {
    let server = TestServer::start(|r: &Recorded| match r.path() {
        "/api/v3/activities/404" => ok_rates(Reply::json(
            404,
            problem("Record Not Found", "Activity", "id", "not found"),
        )),
        "/api/v3/activities/1" => ok_rates(Reply::json(200, r#"{"id":1,"name":"Truncated"#)),
        "/api/v3/activities/2" => {
            ok_rates(Reply::text(200, "text/html", "<html>maintenance</html>"))
        }
        "/api/v3/activities/3" => ok_rates(Reply::json(403, "not json at all")),
        "/api/v3/activities/4" => {
            let name = "x".repeat(1_100_000);
            ok_rates(Reply::json(200, activity_json(4, &name, 1.0, 1, "Ride")))
        }
        _ => Reply::json(500, "{}"),
    })
    .await;
    let api = fresh_session(&server).client();

    let missing = api.activities().get(404u64).send().await.unwrap_err();
    assert_eq!(missing.status(), Some(404));
    assert!(!missing.is_ambiguous());
    let failure = http(&missing);
    assert_eq!(failure.rates.read, Ok(Some(quota(100, 7, 1000, 210))));
    match &failure.document {
        Ok(ErrorDocument::Problem(p)) => {
            assert_eq!(p.message, "Record Not Found");
            assert_eq!(
                (p.errors[0].resource.as_str(), p.errors[0].code.as_str()),
                ("Activity", "not found")
            );
        }
        other => panic!("expected problem, got {other:?}"),
    }
    let shown = format!("{missing:?} / {missing}");
    assert!(!shown.contains("Record Not Found"), "{shown}");

    let truncated = api.activities().get(1u64).send().await.unwrap_err();
    match &truncated {
        Error::Concord(e) => assert_eq!(e.kind(), ErrorKind::Decode),
        other => panic!("expected Concord decode, got {other:?}"),
    }
    assert_eq!(truncated.status(), Some(200));

    let html = api.activities().get(2u64).send().await.unwrap_err();
    match &html {
        Error::Concord(e) => assert_eq!(e.kind(), ErrorKind::Metadata),
        other => panic!("expected Concord metadata, got {other:?}"),
    }

    let forbidden = api.activities().get(3u64).send().await.unwrap_err();
    assert_eq!(forbidden.status(), Some(403));
    assert!(
        http(&forbidden).document.is_err(),
        "unreadable error document is reported, status kept"
    );

    let huge = api.activities().get(4).send().await.unwrap_err();
    match &huge {
        Error::Concord(e) => assert_eq!(e.kind(), ErrorKind::Limit),
        other => panic!("expected Concord limit, got {other:?}"),
    }

    assert_eq!(
        server.requests().len(),
        5,
        "no retries for 404/decode/metadata/403/limit"
    );
    Ok(())
}

/// 401 is returned without replay; the next call refreshes first.
#[tokio::test]
async fn w1_unauthorized_then_next_call_refreshes() -> Res {
    let api_calls = Arc::new(AtomicUsize::new(0));
    let seen = api_calls.clone();
    let server = TestServer::start(move |r: &Recorded| match (r.method.as_str(), r.path()) {
        ("POST", "/oauth/token") => {
            Reply::json(200, token_json("access-after-401", "refresh-after-401"))
        }
        _ if seen.fetch_add(1, Ordering::SeqCst) == 0 => Reply::json(
            401,
            problem("Authorization Error", "Athlete", "access_token", "invalid"),
        ),
        _ => ok_rates(Reply::json(
            200,
            activity_json(77, "Walk", 2_000.0, 1_500, "Walk"),
        )),
    })
    .await;
    let session = fresh_session(&server);
    let api = session.client();
    let denied = api.activities().get(77u64).send().await.unwrap_err();
    assert_eq!(denied.status(), Some(401));
    assert_eq!(
        server.requests().len(),
        1,
        "no replay, no refresh on the failed call itself"
    );

    let ok = api.activities().get(77u64).send().await?;
    assert_eq!(ok.value.id, 77);
    let reqs = server.requests();
    let order: Vec<&str> = reqs.iter().map(|r| r.path()).collect();
    assert_eq!(
        order,
        [
            "/api/v3/activities/77",
            "/oauth/token",
            "/api/v3/activities/77"
        ]
    );
    assert_api_headers(&reqs[2], "access-after-401");
    assert_eq!(
        session.latest_tokens().await.refresh_token,
        "refresh-after-401"
    );
    Ok(())
}

/// Page sizes outside 1..=200 are refused before anything is sent.
#[tokio::test]
async fn w1_invalid_page_size_sends_nothing() -> Res {
    let server = TestServer::start(|_r: &Recorded| Reply::json(200, "[]")).await;
    let api = fresh_session(&server).client();
    for size in [0u16, 201] {
        match api
            .athlete()
            .pages(size, Resume::Start, NonZeroUsize::new(1).unwrap())
        {
            Err(Error::Invalid(_)) => {}
            Err(other) => panic!("expected Invalid for {size}, got {other:?}"),
            Ok(_) => panic!("expected Invalid for {size}"),
        }
    }
    assert!(server.requests().is_empty());
    Ok(())
}

/// Incremental sync "activities after my last sync": the bounded `pages` takes no filters, so the
/// application pages the generated single-page endpoint itself.
#[tokio::test]
async fn w1_incremental_sync_after_last_sync_uses_the_single_page_endpoint() -> Res {
    let server = TestServer::start(|r: &Recorded| {
        let body = match r.query_all("page")[0].as_str() {
            "1" => format!(
                "[{},{}]",
                activity_json(12_100_000_001, "Commute", 8_000.0, 1_500, "Ride"),
                activity_json(12_100_000_002, "Commute home", 8_100.0, 1_600, "Ride")
            ),
            _ => "[]".to_owned(),
        };
        ok_rates(Reply::json(200, body))
    })
    .await;
    let api = fresh_session(&server).client();
    let last_sync = 1_762_300_000u64;
    let mut page = 1u32;
    let mut synced = Vec::new();
    loop {
        let got = api
            .athlete()
            .activities()
            .set_after(last_sync)
            .set_per_page(50)
            .set_page(page)
            .send()
            .await?;
        assert_eq!(got.attempts, 1);
        if got.value.is_empty() {
            break;
        }
        synced.extend(got.value);
        page += 1;
        assert!(page <= 10, "application-side page cap");
    }
    assert_eq!(synced.len(), 2);
    let targets: Vec<String> = server.requests().into_iter().map(|r| r.target).collect();
    assert_eq!(
        targets,
        [
            "/api/v3/athlete/activities?page=1&per_page=50&after=1762300000",
            "/api/v3/athlete/activities?page=2&per_page=50&after=1762300000",
        ]
    );
    Ok(())
}

/// Two concurrent calls on clones of one session with an expired token: one refresh.
#[tokio::test]
async fn w1_concurrent_calls_share_one_refresh() -> Res {
    let server = TestServer::start(|r: &Recorded| match r.path() {
        "/oauth/token" => Reply::json(200, token_json("access-shared", "refresh-shared"))
            .delay(Duration::from_millis(150)),
        _ => ok_rates(Reply::json(
            200,
            activity_json(31, "Swim", 1_500.0, 1_800, "Swim"),
        )),
    })
    .await;
    let session = connect(
        &server,
        stored_tokens(ACCESS, REFRESH, now() - 60, 0),
        Duration::from_secs(5),
    );
    let (a, b) = (session.client(), session.client());
    let (x, y) = tokio::join!(a.activities().get(31).send(), b.activities().get(31).send());
    assert_eq!((x?.value.id, y?.value.id), (31, 31));
    let reqs = server.requests();
    assert_eq!(
        reqs.iter().filter(|r| r.path() == "/oauth/token").count(),
        1
    );
    for r in reqs.iter().filter(|r| r.path() != "/oauth/token") {
        assert_api_headers(r, "access-shared");
    }
    Ok(())
}

/// A read whose connection drops before any head is retried once; an upload is not.
#[tokio::test]
async fn w1_read_without_head_is_retried_once_upload_is_not() -> Res {
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let server = TestServer::start(move |r: &Recorded| match r.method.as_str() {
        "GET" if seen.fetch_add(1, Ordering::SeqCst) == 0 => Reply::hang_up(),
        "GET" => ok_rates(Reply::json(
            200,
            activity_json(41, "Hike", 9_000.0, 7_200, "Hike"),
        )),
        _ => Reply::hang_up(),
    })
    .await;
    let api = fresh_session(&server).client();
    let got = api.activities().get(41).send().await?;
    assert_eq!(got.attempts, 2);

    let err = api
        .uploads()
        .create(morning_ride())
        .send()
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Ambiguous(_)), "{err:?}");
    let posts = server
        .requests()
        .into_iter()
        .filter(|r| r.method == "POST")
        .count();
    assert_eq!(posts, 1);
    Ok(())
}

// ---------------------------------------------------------------- 429 across windows

/// A 429 that exhausts only the read (non-upload) short window gates reads, not uploads.
#[tokio::test]
async fn w429_read_window_gates_reads_but_not_uploads() -> Res {
    let server = TestServer::start(|r: &Recorded| match (r.method.as_str(), r.path()) {
        ("GET", _) => rated(
            Reply::json(429, r#"{"message":"Rate Limit Exceeded","errors":[{"resource":"Application","field":"rate limit","code":"exceeded"}]}"#),
            "200,2000",
            "150,900",
            "100,1000",
            "100,880",
        ),
        _ => ok_rates(Reply::json(201, upload_status(13_987_654_321, None, "Your activity is still being processed.", None))),
    })
    .await;
    let api = fresh_session(&server).client();

    let limited = api.activities().get(5u64).send().await.unwrap_err();
    assert_eq!(limited.status(), Some(429));
    let failure = http(&limited);
    assert_eq!(failure.rates.read, Ok(Some(quota(100, 100, 1000, 880))));
    assert_eq!(failure.rates.overall, Ok(Some(quota(200, 150, 2000, 900))));
    assert_eq!(server.requests().len(), 1, "429 is never retried");

    // The next read is refused locally until the next quarter hour.
    let gated = api.activities().get(6u64).send().await.unwrap_err();
    match &gated {
        Error::Admission(concord_ratelimit::Error::Gated(Some(wait))) => {
            assert!(*wait <= Duration::from_secs(15 * 60), "{wait:?}");
        }
        other => panic!("expected a timed read gate, got {other:?}"),
    }
    assert_eq!(server.requests().len(), 1);

    // An upload is charged only to the overall windows, which still have room.
    let receipt = api.uploads().create(morning_ride()).send().await?;
    assert_eq!(receipt.value.id, 13_987_654_321);
    assert_eq!(server.requests().len(), 2);
    Ok(())
}

/// A 429 on the upload that exhausts the overall daily window gates every later call.
#[tokio::test]
async fn w429_overall_daily_window_gates_reads_and_uploads() -> Res {
    let server = TestServer::start(|_r: &Recorded| {
        rated(
            Reply::json(
                429,
                problem(
                    "Rate Limit Exceeded",
                    "Application",
                    "rate limit",
                    "exceeded",
                ),
            ),
            "200,2000",
            "40,2000",
            "100,1000",
            "20,700",
        )
    })
    .await;
    let session = fresh_session(&server);
    let api = session.client();
    let limited = api
        .uploads()
        .create(morning_ride())
        .send()
        .await
        .unwrap_err();
    assert_eq!(limited.status(), Some(429));
    assert!(
        !limited.is_ambiguous(),
        "a 429 says the upload was not accepted"
    );
    assert_eq!(
        http(&limited).rates.overall,
        Ok(Some(quota(200, 40, 2000, 2000)))
    );

    // Clones share the observation: both a read and another upload are refused locally.
    let read = session
        .client()
        .activities()
        .get(1u64)
        .send()
        .await
        .unwrap_err();
    assert!(
        matches!(read, Error::Admission(concord_ratelimit::Error::Gated(_))),
        "{read:?}"
    );
    let again = api
        .uploads()
        .create(morning_ride())
        .send()
        .await
        .unwrap_err();
    assert!(
        matches!(again, Error::Admission(concord_ratelimit::Error::Gated(_))),
        "{again:?}"
    );
    assert_eq!(server.requests().len(), 1);
    Ok(())
}

/// A 429 on the upload that exhausts the overall short window also gates reads (they are charged
/// to the overall windows too) until the next quarter hour.
#[tokio::test]
async fn w429_overall_short_window_on_upload_gates_reads() -> Res {
    let server = TestServer::start(|_r: &Recorded| {
        rated(
            Reply::json(
                429,
                problem(
                    "Rate Limit Exceeded",
                    "Application",
                    "rate limit",
                    "exceeded",
                ),
            ),
            "200,2000",
            "200,1200",
            "100,1000",
            "30,400",
        )
    })
    .await;
    let api = fresh_session(&server).client();
    let limited = api
        .uploads()
        .create(morning_ride())
        .send()
        .await
        .unwrap_err();
    assert_eq!(limited.status(), Some(429));
    match api.activities().get(1).send().await.unwrap_err() {
        Error::Admission(concord_ratelimit::Error::Gated(Some(wait))) => {
            assert!(wait <= Duration::from_secs(15 * 60), "{wait:?}");
        }
        other => panic!("expected a timed gate on reads, got {other:?}"),
    }
    assert_eq!(server.requests().len(), 1);
    Ok(())
}

/// A 429 without counters or Retry-After gates conservatively.
#[tokio::test]
async fn w429_unexplained_gates_everything() -> Res {
    let server = TestServer::start(|_r: &Recorded| {
        Reply::json(
            429,
            problem(
                "Rate Limit Exceeded",
                "Application",
                "rate limit",
                "exceeded",
            ),
        )
    })
    .await;
    let api = fresh_session(&server).client();
    let limited = api.activities().get(9u64).send().await.unwrap_err();
    assert_eq!(limited.status(), Some(429));
    assert_eq!(http(&limited).rates.overall, Ok(None));
    let next = api
        .uploads()
        .create(morning_ride())
        .send()
        .await
        .unwrap_err();
    assert!(matches!(next, Error::Admission(_)), "{next:?}");
    assert_eq!(server.requests().len(), 1);
    Ok(())
}

/// Usage headers as observed: malformed and partial pairs are reported per class; a successful
/// response that shows an exhausted read window gates the next read but not an upload.
#[tokio::test]
async fn w429_usage_headers_are_observed_per_class() -> Res {
    let server = TestServer::start(|r: &Recorded| match (r.method.as_str(), r.path()) {
        ("GET", "/api/v3/activities/1") => Reply::json(200, activity_json(1, "A", 1.0, 1, "Run"))
            .header("x-ratelimit-limit", "200,2000")
            .header("x-readratelimit-limit", "100,1000")
            .header("x-readratelimit-usage", "lots,1"),
        ("GET", _) => rated(
            Reply::json(200, activity_json(2, "B", 1.0, 1, "Run")),
            "200,2000",
            "60,600",
            "100,1000",
            "100,600",
        ),
        _ => ok_rates(Reply::json(
            201,
            upload_status(
                18_000_000_001,
                None,
                "Your activity is still being processed.",
                None,
            ),
        )),
    })
    .await;
    let api = fresh_session(&server).client();
    let odd = api.activities().get(1).send().await?;
    assert_eq!(
        odd.rates.overall,
        Err(FieldError::Invalid),
        "partial pair (limit without usage)"
    );
    assert_eq!(odd.rates.read, Err(FieldError::Invalid), "malformed pair");

    let full = api.activities().get(2).send().await?;
    assert!(full.value.id == 2 && full.rates.read == Ok(Some(quota(100, 100, 1000, 600))));
    let gated = api.activities().get(3).send().await.unwrap_err();
    assert!(
        matches!(
            gated,
            Error::Admission(concord_ratelimit::Error::Gated(Some(_)))
        ),
        "{gated:?}"
    );
    let receipt = api.uploads().create(morning_ride()).send().await?;
    assert_eq!(receipt.value.id, 18_000_000_001);
    assert_eq!(server.requests().len(), 3);
    Ok(())
}

/// A 429 with room left in every window but a Retry-After: a global cooldown for reads and uploads.
#[tokio::test]
async fn w429_retry_after_is_a_global_cooldown() -> Res {
    let server = TestServer::start(|_r: &Recorded| {
        ok_rates(Reply::json(
            429,
            problem(
                "Rate Limit Exceeded",
                "Application",
                "rate limit",
                "exceeded",
            ),
        ))
        .header("retry-after", "30")
    })
    .await;
    let api = fresh_session(&server).client();
    let limited = api.activities().get(9).send().await.unwrap_err();
    assert_eq!(limited.status(), Some(429));
    for next in [
        api.activities().get(10).send().await.unwrap_err(),
        api.uploads()
            .create(morning_ride())
            .send()
            .await
            .unwrap_err(),
    ] {
        match next {
            Error::Admission(concord_ratelimit::Error::Gated(Some(wait))) => {
                assert!(
                    wait <= Duration::from_secs(30) && wait > Duration::from_secs(20),
                    "{wait:?}"
                );
            }
            other => panic!("expected the Retry-After cooldown, got {other:?}"),
        }
    }
    assert_eq!(server.requests().len(), 1);
    Ok(())
}

// ---------------------------------------------------------------- workflow 2

/// The ordinary path: submit once, keep the receipt, poll to completion, read the activity.
#[tokio::test]
async fn w2_upload_keep_receipt_poll_until_ready_then_read_activity() -> Res {
    let polls = Arc::new(AtomicUsize::new(0));
    let seen = polls.clone();
    let server = TestServer::start(move |r: &Recorded| match (r.method.as_str(), r.path()) {
        ("POST", "/api/v3/uploads") => ok_rates(Reply::json(
            201,
            upload_status(
                13_987_654_321,
                None,
                "Your activity is still being processed.",
                None,
            ),
        )),
        ("GET", "/api/v3/uploads/13987654321") => {
            let body = if seen.fetch_add(1, Ordering::SeqCst) == 0 {
                upload_status(
                    13_987_654_321,
                    None,
                    "Your activity is still being processed.",
                    None,
                )
            } else {
                upload_status(
                    13_987_654_321,
                    None,
                    "Your activity is ready.",
                    Some(16_200_000_001),
                )
            };
            ok_rates(Reply::json(200, body))
        }
        ("GET", "/api/v3/activities/16200000001") => ok_rates(Reply::json(
            200,
            activity_json(16_200_000_001, "Morning Ride", 31.2, 5, "Ride"),
        )),
        _ => Reply::json(
            404,
            problem("Record Not Found", "Upload", "id", "not found"),
        ),
    })
    .await;
    let api = fresh_session(&server).client();

    let receipt = api.uploads().create(morning_ride()).send().await?;
    assert_eq!(receipt.attempts, 1);
    assert_eq!(receipt.meta.status().as_u16(), 201);
    assert_eq!(receipt.value.id, 13_987_654_321);
    assert_eq!(receipt.value.id_str.as_deref(), Some("13987654321"));
    assert!(receipt.value.activity_id.is_none() && receipt.value.error.is_none());
    assert_eq!(
        server.requests().len(),
        1,
        "creating the upload does not poll"
    );

    let started = Instant::now();
    let done = api.uploads().poll(receipt.value, policy(5)).await?;
    let waited = started.elapsed();
    assert_eq!(done.activity_id, Some(16_200_000_001));
    assert_eq!(done.status, "Your activity is ready.");
    assert!(
        waited >= Duration::from_secs(2),
        "two polls, each after >= 1 s: {waited:?}"
    );
    assert_eq!(polls.load(Ordering::SeqCst), 2);

    let activity = api
        .activities()
        .get(done.activity_id.unwrap())
        .send()
        .await?;
    assert_eq!(activity.value.name, "Morning Ride");

    // Wire: one multipart POST with a streamed body, then status GETs by upload id.
    let reqs = server.requests();
    let upload = &reqs[0];
    assert_eq!(
        (upload.method.as_str(), upload.target.as_str()),
        ("POST", "/api/v3/uploads")
    );
    assert_api_headers(upload, ACCESS);
    let content_type = upload.header("content-type").unwrap().to_owned();
    assert!(
        content_type.starts_with("multipart/form-data; boundary="),
        "{content_type}"
    );
    assert!(
        upload.chunked,
        "a one-shot stream is sent chunked, never buffered"
    );
    let body = upload.body_text();
    for part in [
        "name=\"data_type\"\r\n\r\ngpx\r\n",
        "name=\"external_id\"\r\n\r\nbikecomputer-2026-11-07-0600\r\n",
        "name=\"name\"\r\n\r\nMorning Ride\r\n",
        "name=\"file\"; filename=\"morning-ride.gpx\"",
    ] {
        assert!(body.contains(part), "missing {part:?} in\n{body}");
    }
    assert!(body.contains(GPX), "file bytes are sent unchanged");
    assert!(
        !body.contains("sport_type"),
        "no sport override unless asked"
    );
    let rest: Vec<(&str, &str)> = reqs[1..]
        .iter()
        .map(|r| (r.method.as_str(), r.target.as_str()))
        .collect();
    assert_eq!(
        rest,
        [
            ("GET", "/api/v3/uploads/13987654321"),
            ("GET", "/api/v3/uploads/13987654321"),
            (
                "GET",
                "/api/v3/activities/16200000001?include_all_efforts=false"
            ),
        ]
    );
    Ok(())
}

/// One call: upload and poll. A processing error retains the receipt and its untrusted text.
#[tokio::test]
async fn w2_upload_and_poll_reports_processing_error() -> Res {
    let dup = "morning-ride.gpx duplicate of <a href='/activities/16100000000'>16100000000</a>";
    let server = TestServer::start(move |r: &Recorded| match r.method.as_str() {
        "POST" => ok_rates(Reply::json(
            201,
            upload_status(
                14_000_000_001,
                None,
                "Your activity is still being processed.",
                None,
            ),
        )),
        _ => ok_rates(Reply::json(
            200,
            upload_status(
                14_000_000_001,
                Some(dup),
                "There was an error processing your activity.",
                None,
            ),
        )),
    })
    .await;
    let api = fresh_session(&server).client();
    let err = api
        .uploads()
        .upload_and_poll(morning_ride(), policy(5))
        .await
        .unwrap_err();
    let status = match &err {
        Error::Processing(status) => status,
        other => panic!("expected Processing, got {other:?}"),
    };
    assert_eq!(status.id, 14_000_000_001);
    assert_eq!(status.error.as_deref(), Some(dup));
    assert!(status.activity_id.is_none());
    assert!(!err.is_ambiguous());
    let shown = format!("{err:?} / {err}");
    assert!(
        !shown.contains("duplicate"),
        "untrusted provider text withheld: {shown}"
    );
    assert_eq!(
        server.requests().len(),
        2,
        "one POST, one status GET, then stop"
    );
    Ok(())
}

/// The poll budget ends with the last receipt; resuming polls the same id and never re-uploads.
#[tokio::test]
async fn w2_poll_bound_then_resume_with_retained_receipt() -> Res {
    let polls = Arc::new(AtomicUsize::new(0));
    let seen = polls.clone();
    let server = TestServer::start(move |r: &Recorded| match r.method.as_str() {
        "POST" => ok_rates(Reply::json(
            201,
            upload_status(
                15_000_000_001,
                None,
                "Your activity is still being processed.",
                None,
            ),
        )),
        _ if seen.fetch_add(1, Ordering::SeqCst) < 1 => ok_rates(Reply::json(
            200,
            upload_status(
                15_000_000_001,
                None,
                "Your activity is still being processed.",
                None,
            ),
        )),
        _ => ok_rates(Reply::json(
            200,
            upload_status(
                15_000_000_001,
                None,
                "Your activity is ready.",
                Some(16_300_000_001),
            ),
        )),
    })
    .await;
    let api = fresh_session(&server).client();
    let err = api
        .uploads()
        .upload_and_poll(morning_ride(), policy(1))
        .await
        .unwrap_err();
    let last = match err {
        Error::PollBounded(last) => last,
        other => panic!("expected PollBounded, got {other:?}"),
    };
    assert_eq!(last.id, 15_000_000_001);
    assert!(last.activity_id.is_none());

    let done = api.uploads().poll(*last, policy(3)).await?;
    assert_eq!(done.activity_id, Some(16_300_000_001));
    let methods: Vec<String> = server.requests().into_iter().map(|r| r.method).collect();
    assert_eq!(methods, ["POST", "GET", "GET"]);
    Ok(())
}

/// The response does not arrive within the timeout: ambiguous, one POST, no replay, no polling.
#[tokio::test]
async fn w2_upload_timeout_is_ambiguous_and_never_replayed() -> Res {
    let server = TestServer::start(|r: &Recorded| match r.method.as_str() {
        "POST" => Reply::json(
            201,
            upload_status(
                16_000_000_001,
                None,
                "Your activity is still being processed.",
                None,
            ),
        )
        .delay(Duration::from_secs(3)),
        _ => Reply::json(
            200,
            upload_status(16_000_000_001, None, "Your activity is ready.", Some(1)),
        ),
    })
    .await;
    let session = connect(
        &server,
        stored_tokens(ACCESS, REFRESH, now() + 21_600, 21_600),
        Duration::from_millis(400),
    );
    let api = session.client();
    let started = Instant::now();
    let err = api
        .uploads()
        .upload_and_poll(morning_ride(), policy(3))
        .await
        .unwrap_err();
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "bounded by the session timeout"
    );
    assert!(matches!(err, Error::Ambiguous(_)), "{err:?}");
    assert!(err.is_ambiguous());
    assert_eq!(err.status(), None);
    // Give a hypothetical replay or poll time to show up.
    tokio::time::sleep(Duration::from_millis(1_500)).await;
    let reqs = server.requests();
    assert_eq!(
        reqs.len(),
        1,
        "exactly one POST and no status GET without a receipt"
    );
    assert_eq!(reqs[0].method, "POST");
    Ok(())
}

/// The file source fails mid-body (disk read error): ambiguous, no replay.
#[tokio::test]
async fn w2_source_failure_is_ambiguous_and_never_replayed() -> Res {
    let server =
        TestServer::start(|_r: &Recorded| Reply::json(201, upload_status(1, None, "x", None)))
            .await;
    let api = fresh_session(&server).client();
    let chunks: Vec<Result<Vec<u8>, std::io::Error>> = vec![
        Ok(GPX.as_bytes()[..100].to_vec()),
        Err(std::io::Error::other("disk read failed")),
    ];
    let file = UploadFile::new(
        "morning-ride.gpx",
        DataType::Gpx,
        reqwest::Body::wrap_stream(stream::iter(chunks)),
    )?;
    let err = api.uploads().create(file).send().await.unwrap_err();
    assert!(err.is_ambiguous(), "{err:?}");
    assert!(matches!(err, Error::Ambiguous(_)), "{err:?}");
    tokio::time::sleep(Duration::from_millis(300)).await;
    // The truncated chunked body never completes a request; one connection means one attempt.
    assert_eq!(server.requests().len(), 0);
    assert_eq!(server.connections(), 1, "never replayed");
    Ok(())
}

/// Upload 5xx: may have been accepted; one POST only (no 502–504 retry for a one-shot body).
#[tokio::test]
async fn w2_upload_server_errors_are_ambiguous_and_not_retried() -> Res {
    for status in [500u16, 502, 503] {
        let server = TestServer::start(move |_r: &Recorded| {
            ok_rates(Reply::json(status, "<html>error</html>"))
        })
        .await;
        let api = fresh_session(&server).client();
        let err = api
            .uploads()
            .create(morning_ride())
            .send()
            .await
            .unwrap_err();
        assert!(err.is_ambiguous(), "{status}: {err:?}");
        assert_eq!(err.status(), Some(status));
        assert!(
            matches!(&err, Error::Http(f) if f.ambiguous),
            "{status}: {err:?}"
        );
        assert_eq!(server.requests().len(), 1, "{status}: one POST");
    }
    Ok(())
}

/// Upload rejection (400) with the upload-status schema, and a malformed 201 receipt.
#[tokio::test]
async fn w2_upload_rejection_and_unreadable_receipt() -> Res {
    let server = TestServer::start(|r: &Recorded| {
        if r.body_text().contains("bad.fit") {
            ok_rates(Reply::json(
                400,
                upload_status(
                    0,
                    Some("Improperly formatted data."),
                    "There was an error processing your activity.",
                    None,
                ),
            ))
        } else {
            ok_rates(Reply::json(201, r#"{"id":17000000001,"status":"#))
        }
    })
    .await;
    let api = fresh_session(&server).client();

    let fit: Vec<Result<Vec<u8>, std::io::Error>> = vec![Ok(b"not really fit".to_vec())];
    let bad = UploadFile::new(
        "bad.fit",
        DataType::Fit,
        reqwest::Body::wrap_stream(stream::iter(fit)),
    )?;
    let rejected = api.uploads().create(bad).send().await.unwrap_err();
    assert_eq!(rejected.status(), Some(400));
    assert!(!rejected.is_ambiguous());
    match &http(&rejected).document {
        Ok(ErrorDocument::Upload(s)) => {
            assert_eq!(s.error.as_deref(), Some("Improperly formatted data."))
        }
        other => panic!("expected the upload schema, got {other:?}"),
    }

    let unreadable = api
        .uploads()
        .upload_and_poll(morning_ride(), policy(2))
        .await
        .unwrap_err();
    assert!(unreadable.is_ambiguous(), "{unreadable:?}");
    assert!(matches!(unreadable, Error::Ambiguous(_)), "{unreadable:?}");
    assert_eq!(unreadable.status(), Some(201));
    assert_eq!(
        server.requests().len(),
        2,
        "no polling without a readable receipt id"
    );
    Ok(())
}

/// Inputs refused before anything is sent or the source is read.
#[tokio::test]
async fn w2_invalid_inputs_send_nothing() -> Res {
    let server =
        TestServer::start(|_r: &Recorded| Reply::json(201, upload_status(1, None, "x", None)))
            .await;
    let api = fresh_session(&server).client();

    let polled = Arc::new(AtomicBool::new(false));
    let flag = polled.clone();
    let source = stream::poll_fn(move |_cx| {
        flag.store(true, Ordering::SeqCst);
        std::task::Poll::Ready(None::<Result<Vec<u8>, std::io::Error>>)
    });
    let refused = UploadFile::new(
        "ride\".gpx",
        DataType::Gpx,
        reqwest::Body::wrap_stream(source),
    );
    assert!(matches!(refused, Err(Error::Invalid(_))), "{refused:?}");
    assert!(!polled.load(Ordering::SeqCst), "the source was not polled");

    let fast = PollPolicy {
        max_polls: NonZeroUsize::new(3).unwrap(),
        interval: Duration::from_millis(200),
    };
    let err = api
        .uploads()
        .upload_and_poll(morning_ride(), fast)
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Invalid(_)), "{err:?}");
    assert!(
        server.requests().is_empty(),
        "the policy is checked before the one-shot upload"
    );
    Ok(())
}

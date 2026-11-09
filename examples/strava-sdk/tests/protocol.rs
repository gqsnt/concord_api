//! Wire assertions authored from the provider contracts, using only loopback fixtures.
mod support;
use concord_example_testserver::{Body, Recorded, Reply, TestServer, paused};
use concord_sdk_strava::{
    concord::{self, ErrorKind, reqwest},
    *,
};
use serde::Deserialize;
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use support::*;

#[tokio::test]
async fn activity_request_shapes_and_64_bit_detail() {
    let server = TestServer::start(|r: &Recorded| {
        assert_eq!(r.method, "GET");
        assert_eq!(r.header("authorization"), Some("Bearer ACCESS_SECRET_A"));
        assert_eq!(r.header("accept"), Some("application/json"));
        assert!(r.body.is_empty());
        match r.path() {
            "/api/v3/athlete/activities" => {
                assert_eq!(
                    r.target,
                    "/api/v3/athlete/activities?page=2&per_page=7&before=999&after=123"
                );
                Reply::json(200, SECOND)
            }
            "/api/v3/activities/9007199254740999" => {
                assert_eq!(r.query(), Some("include_all_efforts=false"));
                Reply::json(200, ACTIVITY)
            }
            _ => panic!("unexpected fixture path"),
        }
    })
    .await;
    let api = api(&server.url());
    Strava::validate_header_names().unwrap();
    OAuth::validate_header_names().unwrap();
    let list = api
        .athlete()
        .activities()
        .set_page(2)
        .set_per_page(7)
        .set_before(999)
        .set_after(123)
        .send()
        .await
        .unwrap();
    assert_eq!(list.value[0].sport_type, "FutureSport");
    let detail = api.activities().get(9007199254740999).send().await.unwrap();
    assert_eq!(detail.value.id, 9007199254740999);
    assert_eq!(detail.attempts, 1);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn multipart_is_lazy_nonreplayable_and_correct_on_wire() {
    let reads = Arc::new(AtomicUsize::new(0));
    let count = reads.clone();
    let source = futures_util::stream::once(async move {
        count.fetch_add(1, Ordering::SeqCst);
        Ok::<_, std::io::Error>(TRACK.to_vec())
    });
    let server = TestServer::start(|r: &Recorded| {
        assert_eq!(
            (r.method.as_str(), r.target.as_str()),
            ("POST", "/api/v3/uploads")
        );
        assert_eq!(r.header("authorization"), Some("Bearer ACCESS_SECRET_A"));
        let media = r.header("content-type").unwrap();
        let boundary = media
            .strip_prefix("multipart/form-data; boundary=")
            .unwrap();
        let text = r.body_text();
        assert!(text.starts_with(&format!("--{boundary}\r\n")));
        assert!(text.ends_with(&format!("--{boundary}--\r\n")));
        assert!(text.contains("name=\"data_type\"\r\n\r\ngpx\r\n"));
        assert!(text.contains("name=\"file\"; filename=\"synthetic.gpx\""));
        assert!(r.body.windows(TRACK.len()).any(|w| w == TRACK));
        assert!(text.contains("name=\"external_id\"\r\n\r\nsynthetic-track\r\n"));
        assert!(text.contains("name=\"name\"\r\n\r\nCanal\r\n"));
        assert!(text.contains("name=\"sport_type\"\r\n\r\nRide\r\n"));
        Reply::json(201, RECEIPT)
    })
    .await;
    let api = api(&server.url());
    let file = UploadFile::new(
        "synthetic.gpx",
        DataType::Gpx,
        reqwest::Body::wrap_stream(source),
    )
    .unwrap()
    .external_id("synthetic-track".into())
    .name("Canal".into())
    .sport_type("Ride".into());
    safe(&file);
    let op = api.uploads().create(file);
    safe(&op);
    // A separate one-shot operation can be prepared without polling its source.
    let prepared = op.prepare().unwrap();
    assert!(prepared.request().try_clone().is_none());
    assert_eq!(reads.load(Ordering::SeqCst), 0);
    safe(&prepared);
    // Prepared::send intentionally bypasses policies; here use a source with the same
    // one-shot accounting on the generated send path, after dropping the unpolled request.
    drop(prepared);
    assert_eq!(reads.load(Ordering::SeqCst), 0);
    let count = reads.clone();
    let source = futures_util::stream::once(async move {
        count.fetch_add(1, Ordering::SeqCst);
        Ok::<_, std::io::Error>(TRACK.to_vec())
    });
    let file = UploadFile::new(
        "synthetic.gpx",
        DataType::Gpx,
        reqwest::Body::wrap_stream(source),
    )
    .unwrap()
    .external_id("synthetic-track".into())
    .name("Canal".into())
    .sport_type("Ride".into());
    let receipt = api.uploads().create(file).send().await.unwrap();
    assert_eq!(receipt.value.id, 9007199254740997);
    assert_eq!(reads.load(Ordering::SeqCst), 1);
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

#[tokio::test]
async fn authorization_code_form_and_redacted_diagnostics() {
    let server = TestServer::start(|r: &Recorded| {
        assert_eq!(
            (r.method.as_str(), r.target.as_str()),
            ("POST", "/oauth/token")
        );
        assert!(r.query().is_none());
        assert!(r.header("authorization").is_none());
        assert_eq!(
            r.header("content-type"),
            Some("application/x-www-form-urlencoded")
        );
        assert_eq!(
            r.form_pairs(),
            [
                ("client_id".into(), "123".into()),
                ("client_secret".into(), "CLIENT_SECRET".into()),
                ("grant_type".into(), "authorization_code".into()),
                ("code".into(), "CODE_SECRET&+".into())
            ]
        );
        Reply::json(200, TOKENS).header("set-cookie", "COOKIE_SECRET")
    })
    .await;
    let grant = TokenRequest::authorization_code(&credentials(), "CODE_SECRET&+".into()).unwrap();
    safe(&grant);
    safe(credentials());
    safe(tokens(TOKENS));
    let oauth = OAuth::new(transport(Duration::from_secs(2)).unwrap())
        .with_base_url(server.url())
        .unwrap();
    let op = oauth.oauth().token(grant);
    safe(&op);
    let response = op.send().await.unwrap();
    safe(&response);
    safe(&response.value);
    assert_eq!(response.value.refresh_token, "REFRESH_SECRET_A");
    server.shutdown().await;
}

#[tokio::test]
async fn all_http_classes_keep_authoritative_status_and_provider_document() {
    // Auth, scope, missing resource, upload validation/duplicates, rate, server and redirects.
    for status in [302, 400, 401, 403, 404, 409, 422, 429, 500, 503] {
        let server = TestServer::start(move |_: &Recorded| {
            Reply::json(status, PROBLEM)
                .header("location", "http://127.0.0.1:1/URL_SECRET")
                .header("set-cookie", "COOKIE_SECRET")
        })
        .await;
        let error = api(&server.url())
            .uploads()
            .create(file())
            .send()
            .await
            .unwrap_err();
        assert_eq!(error.status(), Some(status));
        assert_eq!(error.is_ambiguous(), status >= 500);
        safe_error(&error);
        let Error::Http(e) = error else {
            panic!("expected HTTP error");
        };
        let ErrorDocument::Problem(problem) = e.document.as_ref().unwrap() else {
            panic!("general problem document");
        };
        assert_eq!(problem.errors[0].field, "file");
        assert_eq!(
            server.requests().len(),
            1,
            "writes and redirects never repeat"
        );
        server.shutdown().await;
    }
}

#[tokio::test]
async fn malformed_wrong_media_oversized_and_truncated_bodies_on_success_and_failure() {
    for status in [200, 400] {
        for case in 0..5 {
            let server = TestServer::start(move |_: &Recorded| match case {
                0 => Reply::json(status, INVALID),
                1 => Reply::text(status, "text/html", "BODY_SECRET"),
                2 => Reply::json(status, vec![b'x'; BODY_LIMIT + 1]),
                3 => Reply::json(status, "").with_body(Body::FailAfter(vec![
                    concord::bytes::Bytes::from_static(b"{\"message\":"),
                ])),
                _ => Reply::new(status).body(PROBLEM),
            })
            .await;
            let error = api(&server.url())
                .activities()
                .get(7)
                .send()
                .await
                .unwrap_err();
            safe_error(&error);
            assert_eq!(error.status(), Some(status));
            let expected = match case {
                0 => ErrorKind::Decode,
                1 | 4 => ErrorKind::Metadata,
                2 => ErrorKind::Limit,
                _ => ErrorKind::Transport,
            };
            let inner = if status == 200 {
                let Error::Concord(e) = &error else {
                    panic!("success body error");
                };
                e
            } else {
                let Error::Http(e) = &error else {
                    panic!("error document error");
                };
                e.document.as_ref().unwrap_err()
            };
            assert_eq!(inner.kind(), expected);
            assert_eq!(server.requests().len(), 1, "body errors are never replayed");
            server.shutdown().await;
        }
    }
}

#[derive(Deserialize)]
struct RateCase {
    overall_limit: String,
    overall_usage: String,
    read_limit: String,
    read_usage: String,
    overall_short: bool,
    overall_daily: bool,
    read_short: bool,
    read_daily: bool,
}
#[tokio::test]
async fn rate_429_exposes_every_exhausted_window_and_refuses_a_second_send() {
    let cases: Vec<RateCase> =
        serde_json::from_str(include_str!("../fixtures/rate-windows.json")).unwrap();
    for case in cases {
        let expected = [
            case.overall_short,
            case.overall_daily,
            case.read_short,
            case.read_daily,
        ];
        let server = TestServer::start(move |_: &Recorded| {
            Reply::json(429, PROBLEM)
                .header("x-ratelimit-limit", &case.overall_limit)
                .header("x-ratelimit-usage", &case.overall_usage)
                .header("x-readratelimit-limit", &case.read_limit)
                .header("x-readratelimit-usage", &case.read_usage)
        })
        .await;
        let api = api(&server.url());
        let error = api.athlete().activities().send().await.unwrap_err();
        let Error::Http(e) = error else {
            panic!("429");
        };
        let overall = e.rates.overall.unwrap().unwrap();
        let read = e.rates.read.unwrap().unwrap();
        assert_eq!(
            [
                overall.short.exhausted(),
                overall.daily.exhausted(),
                read.short.exhausted(),
                read.daily.exhausted()
            ],
            expected
        );
        let next = api.activities().get(7).send().await.unwrap_err();
        assert!(matches!(next, Error::Admission(_)));
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[test]
fn quota_parser_distinguishes_absent_partial_repeated_invalid_and_overflow() {
    use reqwest::header::{HeaderMap, HeaderValue};
    let mut h = HeaderMap::new();
    assert_eq!(RateLimits::parse(&h).overall, Ok(None));
    h.insert("x-ratelimit-limit", HeaderValue::from_static("200,2000"));
    assert_eq!(
        RateLimits::parse(&h).overall,
        Err(concord::FieldError::Invalid)
    );
    for (text, kind) in [
        ("+2,4", concord::FieldError::Invalid),
        ("2,4,8", concord::FieldError::Invalid),
        ("2,", concord::FieldError::Invalid),
        ("4294967296,1", concord::FieldError::Overflow),
    ] {
        h.insert("x-ratelimit-usage", HeaderValue::from_str(text).unwrap());
        assert_eq!(RateLimits::parse(&h).overall, Err(kind));
    }
    h.insert("x-ratelimit-usage", HeaderValue::from_static(" 2 ,\t4 "));
    assert_eq!(
        RateLimits::parse(&h).overall.unwrap().unwrap().daily.used,
        4
    );
    h.append("x-ratelimit-limit", HeaderValue::from_static("200,2000"));
    assert_eq!(
        RateLimits::parse(&h).overall,
        Err(concord::FieldError::Repeated)
    );
}

#[tokio::test]
async fn input_build_and_destination_refusals_do_not_dispatch() {
    let server = TestServer::start(|_: &Recorded| panic!("must not dispatch")).await;
    assert!(
        UploadFile::new(
            "bad\nFILE_SECRET",
            DataType::Gpx,
            reqwest::Body::from(TRACK)
        )
        .is_err()
    );
    assert!(Credentials::new(0, "CLIENT_SECRET".into()).is_err());
    assert!(TokenRequest::authorization_code(&credentials(), String::new()).is_err());
    let api = api(&server.url());
    assert!(
        api.athlete()
            .pages(
                0,
                pagination::Resume::Start,
                std::num::NonZeroUsize::new(1).unwrap()
            )
            .is_err()
    );
    let error = api
        .with_base_url("http://127.0.0.1:1")
        .unwrap()
        .activities()
        .get(1)
        .send()
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Invalid(_)));
    safe_error(&error);
    let bad = tokens(TOKENS);
    assert!(
        Session::connect(
            "http://remote.invalid",
            &server.url(),
            credentials(),
            bad,
            Duration::from_secs(1)
        )
        .is_err()
    );
    let mut bad = tokens(TOKENS);
    bad.token_type = "BODY_SECRET".into();
    let error = Session::connect(
        &server.url(),
        &server.url(),
        credentials(),
        bad,
        Duration::from_secs(1),
    )
    .unwrap()
    .client()
    .activities()
    .get(1)
    .send()
    .await
    .unwrap_err();
    assert!(matches!(error, Error::Auth(_)));
    safe_error(&error);
    assert!(server.requests().is_empty());
    server.shutdown().await;
    let error = OAuth::new(transport(Duration::from_secs(1)).unwrap())
        .with_base_url("http://127.0.0.1:0")
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Build);
    safe(error);
}

#[tokio::test]
async fn get_retries_only_transient_heads_or_headless_transport() {
    for hang in [false, true] {
        let count = Arc::new(AtomicUsize::new(0));
        let calls = count.clone();
        let server = TestServer::start(move |_: &Recorded| {
            if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                if hang {
                    Reply::hang_up()
                } else {
                    Reply::json(503, PROBLEM)
                }
            } else {
                Reply::json(200, ACTIVITY)
            }
        })
        .await;
        let response = api(&server.url()).activities().get(7).send().await.unwrap();
        assert_eq!(response.attempts, 2);
        assert_eq!(server.requests().len(), 2);
        server.shutdown().await;
    }
    let server = TestServer::start(|_: &Recorded| Reply::json(503, PROBLEM)).await;
    assert_eq!(
        api(&server.url())
            .activities()
            .get(7)
            .send()
            .await
            .unwrap_err()
            .status(),
        Some(503)
    );
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

#[tokio::test]
async fn upload_bad_success_body_and_stream_failure_are_ambiguous() {
    let server = TestServer::start(|_: &Recorded| Reply::json(201, INVALID)).await;
    let error = api(&server.url())
        .uploads()
        .create(file())
        .send()
        .await
        .unwrap_err();
    assert!(error.is_ambiguous());
    safe_error(&error);
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
    let server =
        TestServer::start(|_: &Recorded| panic!("incomplete body must not be recorded")).await;
    let source = futures_util::stream::once(async {
        Err::<Vec<u8>, _>(std::io::Error::other("FILE_SECRET"))
    });
    let file = UploadFile::new("a.gpx", DataType::Gpx, reqwest::Body::wrap_stream(source)).unwrap();
    let error = api(&server.url())
        .uploads()
        .create(file)
        .send()
        .await
        .unwrap_err();
    assert!(error.is_ambiguous());
    safe_error(&error);
    assert!(server.connections() <= 1);
    server.shutdown().await;
}

#[tokio::test]
async fn upload_400_can_carry_an_upload_status_document() {
    let server = TestServer::start(|_: &Recorded| Reply::json(400, FAILED)).await;
    let error = api(&server.url())
        .uploads()
        .create(file())
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.status(), Some(400));
    safe_error(&error);
    let Error::Http(failure) = error else {
        panic!("upload HTTP rejection");
    };
    let ErrorDocument::Upload(status) = failure.document.unwrap() else {
        panic!("upload schema");
    };
    assert_eq!(status.id, 9007199254740997);
    assert!(status.error.is_some());
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

#[tokio::test]
async fn retry_after_or_unexplained_429_gates_locally_without_repeating() {
    for hint in [None, Some("10"), Some("BODY_SECRET")] {
        let server = TestServer::start(move |_: &Recorded| {
            let reply = Reply::json(429, PROBLEM);
            if let Some(hint) = hint {
                reply.header("retry-after", hint)
            } else {
                reply
            }
        })
        .await;
        let api = api(&server.url());
        assert_eq!(
            api.activities().get(1).send().await.unwrap_err().status(),
            Some(429)
        );
        let error = api.activities().get(1).send().await.unwrap_err();
        assert!(matches!(error, Error::Admission(_)));
        safe_error(&error);
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

// ---- Activity update (PUT /activities/{id}); maintenance ticket regressions.

fn rename() -> ActivityUpdate {
    ActivityUpdate::new().name("Synthetic evening gravel".into())
}

#[tokio::test]
async fn activity_update_sends_only_the_set_fields_as_json() {
    let server = TestServer::start(|r: &Recorded| {
        assert_eq!(
            (r.method.as_str(), r.target.as_str()),
            ("PUT", "/api/v3/activities/9007199254740999")
        );
        assert_eq!(r.header("authorization"), Some("Bearer ACCESS_SECRET_A"));
        assert_eq!(r.header("content-type"), Some("application/json"));
        assert_eq!(r.header("accept"), Some("application/json"));
        Reply::json(200, UPDATED)
    })
    .await;
    let api = api(&server.url());
    let full = ActivityUpdate::new()
        .name("Synthetic evening gravel".into())
        .sport_type("GravelRide".into())
        .description("BODY_SECRET synthetic note".into())
        .gear_id("b9007199".into())
        .commute(true)
        .trainer(false);
    assert_eq!(
        format!("{full:?}"),
        r#"ActivityUpdate["name", "sport_type", "description", "gear_id", "trainer", "commute"]"#
    );
    let op = api.activities().update(9007199254740999, full.clone());
    safe(&op);
    safe(op.prepare().unwrap());
    let updated = api
        .activities()
        .update(9007199254740999, full)
        .send()
        .await
        .unwrap();
    assert_eq!(updated.attempts, 1);
    assert_eq!(updated.value.sport_type, "GravelRide");
    assert_eq!(updated.value.gear_id.as_deref(), Some("b9007199"));
    assert_eq!(
        (updated.value.commute, updated.value.trainer),
        (Some(true), Some(false))
    );
    api.activities()
        .update(9007199254740999, rename())
        .send()
        .await
        .unwrap();
    api.activities()
        .update(9007199254740999, ActivityUpdate::new().clear_gear())
        .send()
        .await
        .unwrap();
    let bodies: Vec<serde_json::Value> = server
        .requests()
        .iter()
        .map(|r| serde_json::from_slice(&r.body).unwrap())
        .collect();
    assert_eq!(
        bodies,
        [
            serde_json::json!({
                "name": "Synthetic evening gravel",
                "sport_type": "GravelRide",
                "description": "BODY_SECRET synthetic note",
                "gear_id": "b9007199",
                "trainer": false,
                "tags": {"Commute": true}
            }),
            serde_json::json!({"name": "Synthetic evening gravel"}),
            serde_json::json!({"gear_id": "none"}),
        ]
    );
    assert!(
        server
            .requests()
            .iter()
            .all(|r| !r.body_text().contains("\"commute\"") && !r.body_text().contains("\"type\"")),
        "legacy commute and deprecated type are never sent"
    );
    server.shutdown().await;
}

#[tokio::test]
async fn activity_update_is_charged_to_the_non_upload_windows() {
    // A read reporting an exhausted non-upload short window gates a later update, not an upload.
    let server = TestServer::start(|r: &Recorded| match r.method.as_str() {
        "GET" => Reply::json(200, ACTIVITY)
            .header("x-ratelimit-limit", "200,2000")
            .header("x-ratelimit-usage", "30,300")
            .header("x-readratelimit-limit", "100,1000")
            .header("x-readratelimit-usage", "100,300"),
        "POST" => Reply::json(201, RECEIPT),
        _ => Reply::json(200, UPDATED),
    })
    .await;
    let api = api(&server.url());
    api.activities().get(7).send().await.unwrap();
    let refused = api.activities().update(7, rename()).send().await;
    assert!(matches!(refused, Err(Error::Admission(_))), "{refused:?}");
    api.uploads().create(file()).send().await.unwrap();
    assert_eq!(
        server
            .requests()
            .iter()
            .map(|r| r.method.as_str().to_owned())
            .collect::<Vec<_>>(),
        ["GET", "POST"]
    );
    server.shutdown().await;

    // An update answered 429 that exhausts only the non-upload short window: reads are gated,
    // uploads are not, and the 429 is explained (no conservative day-long gate).
    let server = TestServer::start(|r: &Recorded| match r.method.as_str() {
        "PUT" => Reply::json(429, PROBLEM)
            .header("x-ratelimit-limit", "200,2000")
            .header("x-ratelimit-usage", "30,300")
            .header("x-readratelimit-limit", "100,1000")
            .header("x-readratelimit-usage", "100,300"),
        "POST" => Reply::json(201, RECEIPT),
        _ => Reply::json(200, ACTIVITY),
    })
    .await;
    let api = api_for_url(&server.url());
    let limited = api
        .activities()
        .update(7, rename())
        .send()
        .await
        .unwrap_err();
    assert_eq!(limited.status(), Some(429));
    assert!(!limited.is_ambiguous(), "a 429 refused the update");
    let Error::Http(failure) = &limited else {
        panic!("429");
    };
    assert!(failure.rates.read.unwrap().unwrap().short.exhausted());
    let read = api.activities().get(7).send().await.unwrap_err();
    assert!(matches!(read, Error::Admission(_)), "{read:?}");
    api.uploads().create(file()).send().await.unwrap();
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}
fn api_for_url(url: &str) -> Strava {
    support::api(url)
}

// Paused clock: the 200 ms timeout fires only once the server holds the delayed PUT (case 0);
// in the other cases no timeout can fire before the server's answer ends the call.
#[tokio::test(start_paused = true)]
async fn activity_update_failures_after_dispatch_are_uncertain_and_never_resent() {
    for case in 0..4 {
        let server = TestServer::start(move |r: &Recorded| {
            assert_eq!(r.method, "PUT");
            match case {
                0 => Reply::json(200, UPDATED).delay(Duration::from_secs(1)),
                1 => Reply::hang_up(),
                2 => Reply::json(503, PROBLEM),
                _ => Reply::json(200, INVALID),
            }
        })
        .await;
        let timeout = Duration::from_millis(200);
        let api = session(&server.url(), TOKENS, timeout).client();
        let update = api.activities().update(7, rename()).send();
        let error = if case == 0 {
            paused::time_out(&server, |_| true, timeout, update).await
        } else {
            paused::drive(update).await
        }
        .unwrap_err();
        assert!(error.is_ambiguous(), "case {case}: {error:?}");
        safe_error(&error);
        match (&error, case) {
            (Error::Ambiguous(inner), 0) => {
                assert_eq!(inner.delivery(), concord::Delivery::Unknown);
                assert!(inner.transport().unwrap().is_timeout());
                assert!(format!("{error}").contains("re-read the activity"));
            }
            (Error::Ambiguous(inner), 1) => {
                assert_eq!(inner.delivery(), concord::Delivery::Unknown);
            }
            (Error::Http(failure), 2) => assert_eq!(failure.meta.status().as_u16(), 503),
            // Applied (2xx) but unreadable: the caller still re-reads to learn the result.
            (Error::Ambiguous(inner), 3) => {
                assert_eq!(inner.kind(), concord::ErrorKind::Decode);
                assert_eq!(error.status(), Some(200));
            }
            other => panic!("case {case}: {other:?}"),
        }
        assert_eq!(server.requests().len(), 1, "an update is never resent");
        server.shutdown().await;
    }
}

#[tokio::test]
async fn activity_update_without_granted_activity_write_is_refused_before_any_request() {
    let server = TestServer::start(|r: &Recorded| match (r.method.as_str(), r.path()) {
        ("POST", "/oauth/token") => panic!("no refresh for a locally refused update"),
        ("PUT", _) => Reply::json(401, PROBLEM),
        ("POST", _) => Reply::json(201, RECEIPT),
        _ => Reply::json(200, ACTIVITY),
    })
    .await;
    for granted in [
        "read activity:read_all",
        "read,activity:read_all,profile:write",
    ] {
        let mut stored = tokens(EXPIRED);
        stored.scope = Some(granted.into());
        let api = Session::connect(
            &server.url(),
            &server.url(),
            credentials(),
            stored,
            Duration::from_secs(2),
        )
        .unwrap()
        .client();
        let error = api
            .activities()
            .update(7, rename())
            .send()
            .await
            .unwrap_err();
        assert!(
            matches!(error, Error::MissingScope("activity:write")),
            "{error:?}"
        );
        assert_eq!(error.status(), None);
        assert!(!error.is_ambiguous());
        safe_error(&error);
    }
    assert!(server.requests().is_empty(), "no refresh and no dispatch");

    // Reads and uploads keep their behavior with the same granted scopes: they are sent.
    let mut stored = tokens(TOKENS);
    stored.scope = Some("activity:read_all".into());
    let api = Session::connect(
        &server.url(),
        &server.url(),
        credentials(),
        stored,
        Duration::from_secs(2),
    )
    .unwrap()
    .client();
    api.activities().get(7).send().await.unwrap();
    api.uploads().create(file()).send().await.unwrap();
    assert_eq!(server.requests().len(), 2);

    // Unknown, or granted in either delimiter style: the update is sent once and the provider's
    // rejection keeps its status.
    for (n, granted) in [
        None,
        Some("read,activity:write"),
        Some("activity:write read"),
    ]
    .into_iter()
    .enumerate()
    {
        let mut stored = tokens(TOKENS);
        stored.scope = granted.map(String::from);
        let api = Session::connect(
            &server.url(),
            &server.url(),
            credentials(),
            stored,
            Duration::from_secs(2),
        )
        .unwrap()
        .client();
        let error = api
            .activities()
            .update(7, rename())
            .send()
            .await
            .unwrap_err();
        assert_eq!(error.status(), Some(401), "{granted:?}");
        assert!(!error.is_ambiguous());
        assert_eq!(
            server.requests().len(),
            3 + n,
            "a rejected update is not replayed"
        );
    }
    server.shutdown().await;
}

#[test]
fn update_reflection_checks_every_set_field_and_the_gear_sentinel() {
    let current: Activity = serde_json::from_str(UPDATED).unwrap();
    assert!(ActivityUpdate::new().is_reflected_in(&current));
    let same = ActivityUpdate::new()
        .name("Synthetic evening gravel".into())
        .sport_type("GravelRide".into())
        .description("Synthetic edited description".into())
        .gear_id("b9007199".into())
        .commute(true)
        .trainer(false);
    assert!(same.is_reflected_in(&current));
    // Each field alone, differing from the stored activity, is detected.
    for differs in [
        ActivityUpdate::new().name("Other".into()),
        ActivityUpdate::new().sport_type("Ride".into()),
        ActivityUpdate::new().description("Other".into()),
        ActivityUpdate::new().gear_id("b1".into()),
        ActivityUpdate::new().clear_gear(),
        ActivityUpdate::new().commute(false),
        ActivityUpdate::new().trainer(true),
    ] {
        assert!(!differs.is_reflected_in(&current), "{differs:?}");
    }
    let cleared = Activity {
        gear_id: None,
        description: None,
        ..current.clone()
    };
    assert!(ActivityUpdate::new().clear_gear().is_reflected_in(&cleared));
    assert!(
        !ActivityUpdate::new()
            .gear_id("none".into())
            .is_reflected_in(&current)
    );
    assert!(
        !ActivityUpdate::new()
            .description("Synthetic edited description".into())
            .is_reflected_in(&cleared)
    );
}

//! Wire contract, failure-stage, redaction and replay tests against loopback fixtures.

mod support;

use std::{
    error::Error as _,
    time::{Duration, SystemTime},
};

use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use concord_sdk_intercom::{
    ERROR_BODY_LIMIT, Error, ErrorCode, HeaderNumber, Intercom, NewContact, PageSize, RateLimits,
    SUCCESS_BODY_LIMIT,
    concord::{
        ErrorKind,
        bytes::Bytes,
        reqwest::{
            self,
            header::{HeaderMap, HeaderValue},
        },
    },
};
use support::*;

#[tokio::test]
async fn wire_headers_queries_paths_and_email_only_create() {
    let server = TestServer::start(|request: &Recorded| -> Reply {
        if request.path() == "/prefix/contacts" && request.method == "GET" {
            Reply::json(200, LIST_LAST)
        } else {
            Reply::json(200, FIRST)
        }
    })
    .await;
    Intercom::validate_header_names().unwrap();
    let api = api(&server)
        .with_base_url(format!("{}/prefix", server.url()))
        .unwrap();
    api.contacts().list().send().await.unwrap();
    api.contacts()
        .list()
        .set_per_page(PageSize::new(150).unwrap())
        .set_starting_after("cursor+/=?&fixture")
        .send()
        .await
        .unwrap();
    api.contacts().get("id/with?&").send().await.unwrap();
    api.contacts()
        .create(NewContact::with_email("minimal@example.test"))
        .send()
        .await
        .unwrap();
    let requests = server.requests();
    assert_eq!(requests.len(), 4);
    assert_eq!(requests[0].method, "GET");
    assert_eq!(requests[0].target, "/prefix/contacts?per_page=10");
    assert_eq!(
        requests[1].target,
        "/prefix/contacts?per_page=150&starting_after=cursor%2B%2F%3D%3F%26fixture"
    );
    assert_eq!(requests[2].target, "/prefix/contacts/id%2Fwith%3F%26");
    assert_eq!(requests[3].method, "POST");
    assert_eq!(requests[3].target, "/prefix/contacts");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&requests[3].body).unwrap(),
        serde_json::json!({"email":"minimal@example.test"})
    );
    for request in &requests {
        assert_eq!(
            request.header("authorization"),
            Some("Bearer SYNTHETIC_SECRET")
        );
        assert_eq!(request.header("intercom-version"), Some("2.17"));
        assert_eq!(request.header("accept"), Some("application/json"));
    }
    for request in &requests[..3] {
        assert!(request.body.is_empty());
        assert_eq!(request.header("content-type"), None);
    }
    assert_eq!(requests[3].header("content-type"), Some("application/json"));
    server.shutdown().await;
}

#[tokio::test]
async fn all_documented_http_classes_and_provider_codes_remain_typed() {
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../fixtures/error-cases.json")).unwrap();
    for case in cases {
        let status = case["status"].as_u64().unwrap() as u16;
        let body = serde_json::to_vec(&case["body"]).unwrap();
        let server = TestServer::start(move |_: &Recorded| -> Reply {
            Reply::json(status, &body).header("link", "</contacts/canonical>; rel=\"canonical\"")
        })
        .await;
        let error = api(&server)
            .contacts()
            .create(NewContact::with_email("synthetic@example.test"))
            .send()
            .await
            .unwrap_err();
        assert_eq!(error.status().unwrap().as_u16(), status);
        let Error::Provider(provider) = error else {
            panic!("{error}")
        };
        let document = provider.document.as_ref().unwrap();
        assert_eq!(document.request_id, None);
        assert_eq!(document.errors.len(), 1);
        match &document.errors[0].code {
            ErrorCode::Text(s) => assert_eq!(case["body"]["errors"][0]["code"], s.as_str()),
            ErrorCode::Number(n) => assert_eq!(case["body"]["errors"][0]["code"], *n),
        }
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[derive(Clone, Copy, Debug)]
enum Damage {
    Malformed,
    TruncatedJson,
    WrongShape,
    Empty,
    WrongMedia,
    MissingMedia,
    DuplicateMedia,
    Oversize,
    ChunkedOversize,
    CutOff,
}

fn damaged(status: u16, damage: Damage) -> Reply {
    let limit = if status == 200 {
        SUCCESS_BODY_LIMIT
    } else {
        ERROR_BODY_LIMIT
    };
    match damage {
        Damage::Malformed => Reply::json(status, include_str!("../fixtures/malformed.json")),
        Damage::TruncatedJson => Reply::json(status, include_str!("../fixtures/truncated.json")),
        Damage::WrongShape => Reply::json(status, include_str!("../fixtures/wrong-shape.json")),
        Damage::Empty => Reply::json(status, ""),
        Damage::WrongMedia => Reply::text(status, "text/html", FIRST),
        Damage::MissingMedia => Reply::new(status).body(FIRST),
        Damage::DuplicateMedia => {
            Reply::json(status, FIRST).header("content-type", "application/json")
        }
        Damage::Oversize => Reply::json(status, vec![b'x'; limit + 1]),
        Damage::ChunkedOversize => Reply::new(status)
            .header("content-type", "application/json")
            .with_body(Body::Chunks(vec![
                (Duration::ZERO, Bytes::from(vec![b'x'; limit])),
                (Duration::ZERO, Bytes::from_static(b"x")),
            ])),
        Damage::CutOff => Reply::new(status)
            .header("content-type", "application/json")
            .with_body(Body::FailAfter(vec![Bytes::from_static(
                b"{\"type\":\"SYNTHETIC_SECRET\"",
            )])),
    }
}

#[tokio::test]
async fn success_and_error_bodies_are_bounded_and_fail_at_the_correct_stage() {
    for status in [200, 429] {
        for (damage, expected) in [
            (Damage::Malformed, ErrorKind::Decode),
            (Damage::TruncatedJson, ErrorKind::Decode),
            (Damage::WrongShape, ErrorKind::Decode),
            (Damage::Empty, ErrorKind::Decode),
            (Damage::WrongMedia, ErrorKind::Metadata),
            (Damage::MissingMedia, ErrorKind::Metadata),
            (Damage::DuplicateMedia, ErrorKind::Metadata),
            (Damage::Oversize, ErrorKind::Limit),
            (Damage::ChunkedOversize, ErrorKind::Limit),
            (Damage::CutOff, ErrorKind::Transport),
        ] {
            let server =
                TestServer::start(move |_: &Recorded| -> Reply { damaged(status, damage) }).await;
            let error = api(&server)
                .contacts()
                .get("SYNTHETIC_SECRET")
                .send()
                .await
                .unwrap_err();
            assert_eq!(error.status().unwrap().as_u16(), status, "{damage:?}");
            assert_redacted(&error);
            let underlying = match &error {
                Error::Concord(e) if status == 200 => e,
                Error::Provider(e) if status == 429 => e.document.as_ref().unwrap_err(),
                _ => panic!("wrong classification: {error}"),
            };
            assert_eq!(underlying.kind(), expected, "{status} {damage:?}");
            assert_eq!(underlying.head().unwrap().status().as_u16(), status);
            assert_eq!(
                underlying.operation().unwrap().name(),
                "Intercom.contacts.get"
            );
            assert_eq!(server.requests().len(), 1);
            server.shutdown().await;
        }
    }
}

fn assert_redacted(error: &Error) {
    assert!(!format!("{error:?} {error}").contains("SYNTHETIC_SECRET"));
    assert!(error.source().is_none());
}

#[tokio::test]
async fn invalid_error_envelopes_preserve_http_failure_instead_of_claiming_a_typed_document() {
    // Synthetic violations of Error Objects' discriminator and one-or-more errors contract.
    for body in [
        r#"{"type":"error.list","errors":[]}"#,
        r#"{"type":"error.list"}"#,
        r#"{"type":"error.list","errors":[{}]}"#,
        r#"{"type":"error.list","errors":[{"code":{}}]}"#,
        r#"{"type":"list","errors":[{"code":"unauthorized"}]}"#,
    ] {
        let server =
            TestServer::start(move |_: &Recorded| -> Reply { Reply::json(401, body) }).await;
        let error = api(&server)
            .contacts()
            .get("first")
            .send()
            .await
            .unwrap_err();
        assert_eq!(error.status().unwrap().as_u16(), 401);
        let Error::Provider(provider) = error else {
            panic!("{error}")
        };
        assert_eq!(provider.document.unwrap_err().kind(), ErrorKind::Decode);
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn missing_pagination_metadata_and_wrong_discriminators_cannot_mean_exhaustion() {
    for body in [
        include_str!("../fixtures/list-missing-pages.json"),
        "{\"type\":\"error.list\",\"data\":[],\"total_count\":0,\"pages\":null}",
        "{\"type\":\"list\",\"data\":[],\"total_count\":0,\"pages\":{\"next\":{}}}",
    ] {
        let server =
            TestServer::start(move |_: &Recorded| -> Reply { Reply::json(200, body) }).await;
        let error = api(&server).contacts().list().send().await.unwrap_err();
        let Error::Concord(error) = error else {
            panic!("{error}")
        };
        assert_eq!(error.kind(), ErrorKind::Decode);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn declared_limit_also_applies_to_the_prepared_send_path() {
    let server =
        TestServer::start(|_: &Recorded| -> Reply { damaged(200, Damage::ChunkedOversize) }).await;
    let error = api(&server)
        .contacts()
        .get("first")
        .prepare()
        .unwrap()
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Limit);
    server.shutdown().await;
}

#[tokio::test]
async fn ambiguous_creates_and_headerless_429_creates_are_never_replayed() {
    for status in [0, 429, 503] {
        let server = TestServer::start(move |_: &Recorded| -> Reply {
            match status {
                0 => Reply::hang_up(),
                429 => rate_error(),
                _ => Reply::json(status, RATE_ERROR),
            }
        })
        .await;
        let error = api(&server)
            .contacts()
            .create(NewContact::with_email("created@example.test"))
            .send()
            .await
            .unwrap_err();
        if status == 0 {
            assert_eq!(error.status(), None);
        } else {
            assert_eq!(error.status().unwrap().as_u16(), status);
        }
        assert_redacted(&error);
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn redirect_is_returned_and_never_followed() {
    let server = TestServer::start(|_: &Recorded| -> Reply {
        Reply::new(307).header("location", "/unexpected")
    })
    .await;
    // Exercise the public transport constructor as well as the test transport.
    let api = Intercom::with_token(concord_sdk_intercom::http_client().unwrap(), "synthetic")
        .unwrap()
        .with_base_url(server.url())
        .unwrap();
    let error = api
        .contacts()
        .create(NewContact::with_email("created@example.test"))
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.status().unwrap().as_u16(), 307);
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

#[tokio::test]
async fn secrets_are_redacted_through_handles_models_headers_and_provider_errors() {
    let server = TestServer::start(|_: &Recorded| -> Reply {
        Reply::json(401, include_str!("../fixtures/echo-error.json"))
            .header("set-cookie", "SYNTHETIC_SECRET")
            .header("x-request-id", "SYNTHETIC_SECRET")
    })
    .await;
    let api = api(&server);
    let op = api.contacts().list().set_starting_after("SYNTHETIC_SECRET");
    assert!(
        !format!(
            "{api:?} {:?} {op:?} {:?}",
            api.contacts(),
            op.clone().prepare().unwrap()
        )
        .contains("SYNTHETIC_SECRET")
    );
    let error = op.send().await.unwrap_err();
    assert_redacted(&error);
    let Error::Provider(provider) = error else {
        panic!("{error}")
    };
    assert!(!format!("{provider:?}").contains("SYNTHETIC_SECRET"));
    let document = provider.document.as_ref().unwrap();
    assert!(
        !format!(
            "{document:?} {:?} {:?}",
            document.errors, document.errors[0].code
        )
        .contains("SYNTHETIC_SECRET")
    );
    assert_eq!(document.request_id.as_deref(), Some("SYNTHETIC_SECRET"));
    assert_eq!(
        document.errors[0].message.as_deref(),
        Some("Echo SYNTHETIC_SECRET")
    );
    assert_eq!(
        document.errors[0].data.as_ref().unwrap()["echo"],
        "SYNTHETIC_SECRET"
    );
    assert_eq!(provider.meta.headers()["x-request-id"], "SYNTHETIC_SECRET");
    let prepared = api.contacts().get("SYNTHETIC_SECRET").prepare().unwrap();
    assert!(!format!("{prepared:?}").contains("SYNTHETIC_SECRET"));
    assert!(prepared.request().headers()["authorization"].is_sensitive());
    assert!(
        !format!("{:?}", NewContact::with_email("SYNTHETIC_SECRET")).contains("SYNTHETIC_SECRET")
    );
    server.shutdown().await;

    let server = TestServer::start(|_: &Recorded| -> Reply {
        Reply::json(200, CREATED).header("x-secret", "SYNTHETIC_SECRET")
    })
    .await;
    let response = support::api(&server)
        .contacts()
        .get("created")
        .send()
        .await
        .unwrap();
    assert!(!format!("{response:?} {:?}", response.value).contains("SYNTHETIC_SECRET"));
    server.shutdown().await;
}

#[tokio::test]
async fn invalid_inputs_and_tokens_fail_before_io_with_redacted_diagnostics() {
    let server = TestServer::start(|_: &Recorded| -> Reply { Reply::json(200, LIST_FIRST) }).await;
    let api = api(&server);
    for id in ["", ".", ".."] {
        let error = api.contacts().get(id).send().await.unwrap_err();
        assert!(matches!(&error, Error::Concord(e) if e.kind() == ErrorKind::Build));
        assert_redacted(&error);
    }
    for size in [0, 151, u16::MAX] {
        assert!(matches!(PageSize::new(size), Err(Error::PageSize)));
    }
    for token in ["", "bad token", "SYNTHETIC_SECRET\r\nx: injected"] {
        let error = Intercom::with_token(reqwest::Client::new(), token).unwrap_err();
        assert!(!format!("{error:?} {error}").contains("SYNTHETIC_SECRET"));
    }
    assert!(server.requests().is_empty());
    server.shutdown().await;
}

#[test]
fn rate_headers_distinguish_unknown_invalid_and_real_unix_reset() {
    let mut headers = HeaderMap::new();
    assert_eq!(
        RateLimits::from_headers(&headers).reset_unix_seconds,
        HeaderNumber::Missing
    );
    headers.insert("x-ratelimit-limit", HeaderValue::from_static("167"));
    headers.insert("x-ratelimit-remaining", HeaderValue::from_static("0"));
    headers.insert("x-ratelimit-reset", HeaderValue::from_static("1487332520"));
    assert_eq!(
        RateLimits::from_headers(&headers),
        RateLimits {
            limit: HeaderNumber::Value(167),
            remaining: HeaderNumber::Value(0),
            reset_unix_seconds: HeaderNumber::Value(1487332520),
        }
    );
    for value in [
        "",
        "-1",
        "+1",
        "1.5",
        "1, 2",
        "18446744073709551616",
        "SYNTHETIC_SECRET",
    ] {
        headers.insert("x-ratelimit-reset", HeaderValue::from_str(value).unwrap());
        assert_eq!(
            RateLimits::from_headers(&headers).reset_unix_seconds,
            HeaderNumber::Invalid
        );
    }
    headers.insert("x-ratelimit-reset", HeaderValue::from_static("1"));
    headers.append("x-ratelimit-reset", HeaderValue::from_static("1"));
    assert_eq!(
        RateLimits::from_headers(&headers).reset_unix_seconds,
        HeaderNumber::Invalid
    );
}

#[tokio::test]
async fn rate_metadata_survives_bad_body_and_retry_hint_does_not_trigger_scheduling() {
    let server = TestServer::start(|_: &Recorded| -> Reply {
        Reply::json(429, "broken")
            .header("retry-after", "3")
            .header("x-ratelimit-reset", "1487332520")
            .header("x-ratelimit-remaining", "0")
    })
    .await;
    let error = api(&server).contacts().list().send().await.unwrap_err();
    let Error::Provider(error) = error else {
        panic!("{error}")
    };
    assert_eq!(
        error.rate_limits().reset_unix_seconds,
        HeaderNumber::Value(1487332520)
    );
    assert_eq!(
        error.retry_after(SystemTime::UNIX_EPOCH).unwrap(),
        Some(Duration::from_secs(3))
    );
    assert_eq!(error.document.unwrap_err().kind(), ErrorKind::Decode);
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

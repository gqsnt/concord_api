#![allow(missing_docs)]

mod common;

use concord::{ErrorKind, reqwest::StatusCode};
use concord_example_testserver::{Body, Reply, TestServer};
use concord_sdk_frankfurter::{
    Date, Decimal, ERROR_BODY_LIMIT, Error, ErrorBodyError, Frankfurter,
};
use serde::Deserialize;

use common::{HISTORICAL, LATEST, assert_wire, client, json};

fn concord_kind(error: &Error) -> ErrorKind {
    match error {
        Error::Concord { source, .. } => source.kind(),
        error => panic!("expected Concord error, got {error:?}"),
    }
}

#[tokio::test]
async fn latest_wire_has_no_optional_query_and_no_body() {
    let server = TestServer::start(|_| json(200, LATEST).header("etag", "fixture-v1")).await;
    let result = client(&server)
        .latest_pair("EUR", "USD")
        .fetch()
        .await
        .unwrap();
    assert_eq!(result.meta.status, StatusCode::OK);
    assert_eq!(result.meta.headers["etag"], "fixture-v1");
    assert_eq!(
        result.meta.operation.unwrap().name(),
        "Frankfurter.latest_pair"
    );
    assert_eq!(result.meta.operation.unwrap().method(), Some("GET"));
    let request = server.single();
    assert_eq!(request.host, server.addr().to_string());
    assert_wire(&request, "/v2/rate/EUR/USD");
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn historical_wire_has_required_date_and_optional_provider_filter() {
    let server = TestServer::start(|_| json(200, HISTORICAL)).await;
    let result = client(&server)
        .historical_pair("EUR", "USD", "2024-01-15".parse::<Date>().unwrap())
        .set_providers("ECB,TCMB")
        .fetch()
        .await
        .unwrap();
    assert_eq!(
        result.meta.operation.unwrap().name(),
        "Frankfurter.historical_pair"
    );
    assert_wire(
        &server.single(),
        "/v2/rate/EUR/USD?date=2024-01-15&providers=ECB%2CTCMB",
    );
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn path_segments_and_query_values_cannot_inject_routes_or_parameters() {
    let server =
        TestServer::start(|_| json(422, include_bytes!("../fixtures/invalid-currency.json"))).await;
    let error = client(&server)
        .latest_pair("EUR/USD", "U?S#D")
        .set_providers("ECB&date=2000-01-01")
        .fetch()
        .await
        .unwrap_err();
    assert_eq!(error.status(), Some(StatusCode::UNPROCESSABLE_ENTITY));
    assert_wire(
        &server.single(),
        "/v2/rate/EUR%2FUSD/U%3FS%23D?providers=ECB%26date%3D2000-01-01",
    );
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[derive(Deserialize)]
struct DecimalCase {
    wire: String,
    exact: String,
}

fn rate_document(number: &str) -> Vec<u8> {
    format!(r#"{{"date":"2026-03-25","base":"EUR","quote":"USD","rate":{number}}}"#).into_bytes()
}

#[tokio::test]
async fn decimals_are_exact_including_more_than_f64_precision_and_exponent_range() {
    let cases: Vec<DecimalCase> =
        serde_json::from_slice(include_bytes!("../fixtures/decimal-cases.json")).unwrap();
    for case in cases {
        let body = rate_document(&case.wire);
        let server = TestServer::start(move |_| json(200, &body)).await;
        let result = client(&server)
            .latest_pair("EUR", "USD")
            .fetch()
            .await
            .unwrap();
        let expected: Decimal = case.exact.parse().unwrap();
        assert_eq!(result.rate.rate, expected, "wire token {}", case.wire);
        if case.wire == "9007199254740993" {
            assert_ne!(result.rate.rate, Decimal::from(9_007_199_254_740_992_u64));
        }
        if case.wire == "0.1" {
            assert_eq!(
                result.rate.rate + "0.2".parse::<Decimal>().unwrap(),
                "0.3".parse::<Decimal>().unwrap()
            );
        }
        assert_wire(&server.single(), "/v2/rate/EUR/USD");
        assert_eq!(server.shutdown().await.open_after, 0);
    }
}

#[tokio::test]
async fn invalid_or_non_positive_numeric_values_never_produce_a_rate() {
    let cases: Vec<String> =
        serde_json::from_slice(include_bytes!("../fixtures/invalid-rates.json")).unwrap();
    for case in cases {
        let body = rate_document(&case);
        let server = TestServer::start(move |_| json(200, &body)).await;
        let error = client(&server)
            .latest_pair("EUR", "USD")
            .fetch()
            .await
            .unwrap_err();
        assert_eq!(concord_kind(&error), ErrorKind::Decode, "wire token {case}");
        assert_eq!(error.status(), Some(StatusCode::OK));
        assert_eq!(
            error.meta().unwrap().headers["x-fixture-id"],
            "frankfurter-offline"
        );
        assert_eq!(server.requests().len(), 1);
        assert_eq!(server.shutdown().await.open_after, 0);
    }
}

#[tokio::test]
async fn invalid_success_shapes_and_incomplete_json_do_not_succeed() {
    let mut cases = vec![
        include_bytes!("../fixtures/malformed.json").to_vec(),
        include_bytes!("../fixtures/v1-shape.json").to_vec(),
        b"{}".to_vec(),
        b"[]".to_vec(),
        b"null".to_vec(),
        Vec::new(),
    ];
    cases.push([LATEST, b"false"].concat());
    cases.push(
        String::from_utf8(LATEST.to_vec())
            .unwrap()
            .replace("2026-03-25", "2026-02-30")
            .into_bytes(),
    );
    cases.push(
        String::from_utf8(LATEST.to_vec())
            .unwrap()
            .replace("\"rate\":1.1568", "\"rate\":1.1568,\"rate\":2")
            .into_bytes(),
    );
    for body in cases {
        let server = TestServer::start(move |_| json(200, &body)).await;
        let error = client(&server)
            .latest_pair("EUR", "USD")
            .fetch()
            .await
            .unwrap_err();
        assert_eq!(concord_kind(&error), ErrorKind::Decode);
        assert_eq!(error.status(), Some(StatusCode::OK));
        assert_eq!(server.shutdown().await.open_after, 0);
    }
}

#[tokio::test]
async fn documented_http_errors_preserve_status_headers_and_typed_problem() {
    let cases: &[(u16, &[u8], Option<u16>)] = &[
        (
            400,
            include_bytes!("../fixtures/bad-request.json"),
            Some(400),
        ),
        (404, include_bytes!("../fixtures/not-found.json"), None),
        (
            422,
            include_bytes!("../fixtures/invalid-currency.json"),
            Some(422),
        ),
        (503, include_bytes!("../fixtures/unavailable.json"), None),
    ];
    for &(status, body, document_status) in cases {
        let server =
            TestServer::start(move |_| json(status, body).header("retry-after", "7")).await;
        let error = client(&server)
            .latest_pair("EUR", "ABC")
            .fetch()
            .await
            .unwrap_err();
        assert_eq!(error.status().unwrap().as_u16(), status);
        assert_eq!(error.operation().unwrap().name(), "Frankfurter.latest_pair");
        let Error::Provider(error) = error else {
            panic!("expected provider error")
        };
        assert_eq!(error.meta.headers["retry-after"], "7");
        let problem = error.document.unwrap();
        assert_eq!(problem.status, document_status);
        assert!(!problem.message.is_empty());
        if status == 422 {
            assert_eq!(problem.message, "invalid currency: ABC");
        }
        assert_wire(&server.single(), "/v2/rate/EUR/ABC");
        assert_eq!(server.shutdown().await.open_after, 0);
    }
}

#[tokio::test]
async fn http_status_remains_authoritative_when_document_disagrees() {
    let server =
        TestServer::start(|_| json(422, include_bytes!("../fixtures/status-mismatch.json"))).await;
    let error = client(&server)
        .latest_pair("EUR", "USD")
        .fetch()
        .await
        .unwrap_err();
    assert_eq!(error.status(), Some(StatusCode::UNPROCESSABLE_ENTITY));
    let Error::Provider(error) = error else {
        panic!("expected provider error")
    };
    assert_eq!(error.document.unwrap().status, Some(200));
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn malformed_error_document_keeps_http_failure() {
    for body in [
        include_bytes!("../fixtures/malformed.json").as_slice(),
        b"{}",
        b"",
    ] {
        let server = TestServer::start(move |_| json(502, body)).await;
        let error = client(&server)
            .latest_pair("EUR", "USD")
            .fetch()
            .await
            .unwrap_err();
        assert_eq!(error.status(), Some(StatusCode::BAD_GATEWAY));
        let Error::Provider(error) = error else {
            panic!("expected provider error")
        };
        assert!(matches!(error.document, Err(ErrorBodyError::Decode(_))));
        assert_eq!(server.shutdown().await.open_after, 0);
    }
}

#[tokio::test]
async fn non_json_error_is_distinct_from_invalid_json_error() {
    let server = TestServer::start(|_| {
        Reply::new(502)
            .header("content-type", "text/html")
            .body("<html>fixture proxy error</html>")
    })
    .await;
    let error = client(&server)
        .latest_pair("EUR", "USD")
        .fetch()
        .await
        .unwrap_err();
    assert_eq!(error.status(), Some(StatusCode::BAD_GATEWAY));
    let Error::Provider(error) = error else {
        panic!("expected provider error")
    };
    assert!(matches!(error.document, Err(ErrorBodyError::Metadata(_))));
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn error_document_limit_is_exact_and_preserves_status() {
    for padding in [0, 1] {
        let prefix = b"{\"message\":\"";
        let suffix = b"\"}";
        let body = [
            prefix.as_slice(),
            &vec![b'x'; ERROR_BODY_LIMIT - prefix.len() - suffix.len() + padding],
            suffix.as_slice(),
        ]
        .concat();
        let server = TestServer::start(move |_| json(429, &body).header("retry-after", "11")).await;
        let error = client(&server)
            .latest_pair("EUR", "USD")
            .fetch()
            .await
            .unwrap_err();
        let Error::Provider(error) = error else {
            panic!("expected provider error")
        };
        assert_eq!(error.meta.status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(error.meta.headers["retry-after"], "11");
        if padding == 0 {
            assert!(error.document.is_ok());
        } else {
            let Err(ErrorBodyError::Read(source)) = error.document else {
                panic!("expected body limit")
            };
            assert_eq!(source.kind(), ErrorKind::Limit);
            assert_eq!(
                source.operation().unwrap().name(),
                "Frankfurter.latest_pair"
            );
        }
        assert_eq!(server.requests().len(), 1);
        assert_eq!(server.shutdown().await.open_after, 0);
    }
}

#[tokio::test]
async fn complete_json_without_http_body_completion_is_a_failure() {
    let server = TestServer::start(|_| {
        json(200, b"").with_body(Body::FailAfter(vec![LATEST.to_vec().into()]))
    })
    .await;
    let error = client(&server)
        .latest_pair("EUR", "USD")
        .fetch()
        .await
        .unwrap_err();
    assert_eq!(concord_kind(&error), ErrorKind::Transport);
    assert_eq!(error.status(), Some(StatusCode::OK));
    assert_eq!(
        error.meta().unwrap().headers["x-fixture-id"],
        "frankfurter-offline"
    );
    assert_eq!(server.requests().len(), 1);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn truncated_error_body_retains_the_failure_status_and_read_cause() {
    let server = TestServer::start(|_| {
        json(503, b"").with_body(Body::FailAfter(vec![b"{\"message\":".to_vec().into()]))
    })
    .await;
    let error = client(&server)
        .latest_pair("EUR", "USD")
        .fetch()
        .await
        .unwrap_err();
    assert_eq!(error.status(), Some(StatusCode::SERVICE_UNAVAILABLE));
    let Error::Provider(error) = error else {
        panic!("expected provider error")
    };
    let Err(ErrorBodyError::Read(source)) = error.document else {
        panic!("expected read failure")
    };
    assert_eq!(source.kind(), ErrorKind::Transport);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn success_requires_exactly_one_json_content_type() {
    for media in [0, 1, 2] {
        let server = TestServer::start(move |_| {
            let reply = Reply::new(200).body(LATEST);
            match media {
                0 => reply,
                1 => reply.header("content-type", "text/plain"),
                _ => reply
                    .header("content-type", "application/json")
                    .header("content-type", "application/json"),
            }
        })
        .await;
        let error = client(&server)
            .latest_pair("EUR", "USD")
            .fetch()
            .await
            .unwrap_err();
        assert_eq!(concord_kind(&error), ErrorKind::Metadata);
        assert_eq!(error.status(), Some(StatusCode::OK));
        let Error::Concord { source, .. } = error else {
            unreachable!()
        };
        assert_eq!(
            source
                .into_response()
                .unwrap()
                .bytes()
                .await
                .unwrap()
                .as_ref(),
            LATEST
        );
        assert_eq!(server.shutdown().await.open_after, 0);
    }
}

#[tokio::test]
async fn failure_before_response_has_no_fabricated_status_or_metadata() {
    let server = TestServer::start(|_| {
        let mut reply = Reply::new(200);
        reply.hang_up = true;
        reply
    })
    .await;
    let error = client(&server)
        .latest_pair("EUR", "USD")
        .fetch()
        .await
        .unwrap_err();
    assert_eq!(concord_kind(&error), ErrorKind::Transport);
    assert!(error.meta().is_none());
    assert!(error.status().is_none());
    assert_eq!(error.operation().unwrap().name(), "Frankfurter.latest_pair");
    assert_eq!(server.requests().len(), 1);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn debug_and_display_do_not_expose_provider_text_or_header_values() {
    let server = TestServer::start(|_| {
        json(422, b"{\"message\":\"synthetic-private-message\"}")
            .header("set-cookie", "synthetic-private-cookie")
    })
    .await;
    let error = client(&server)
        .latest_pair("EUR", "USD")
        .fetch()
        .await
        .unwrap_err();
    let rendered = format!("{error:?} {error}");
    assert!(!rendered.contains("synthetic-private-message"));
    assert!(!rendered.contains("synthetic-private-cookie"));
    assert!(!rendered.contains(&server.url()));
    let Error::Provider(error) = error else {
        panic!("expected provider error")
    };
    assert_eq!(error.document.unwrap().message, "synthetic-private-message");
    assert_eq!(error.meta.headers["set-cookie"], "synthetic-private-cookie");
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[test]
fn hosted_construction_is_io_free_and_base_override_is_validated() {
    let api = Frankfurter::new(concord::reqwest::Client::new());
    let request = api.latest_pair("EUR", "USD").prepare().unwrap();
    assert_eq!(
        request.request().url().as_str(),
        "https://api.frankfurter.dev/v2/rate/EUR/USD"
    );
    assert!(api.clone().with_base_url("not a URL").is_err());
    let prefixed = api.with_base_url("http://127.0.0.1:1234/prefix").unwrap();
    assert_eq!(
        prefixed
            .latest_pair("EUR", "USD")
            .prepare()
            .unwrap()
            .request()
            .url()
            .path(),
        "/prefix/v2/rate/EUR/USD"
    );
    assert!(
        "2024-02-30"
            .parse::<concord_sdk_frankfurter::Date>()
            .is_err()
    );
}

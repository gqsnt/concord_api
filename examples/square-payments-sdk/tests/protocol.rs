//! Independent Square wire contracts and adversarial response fixtures.

mod support;

use concord_example_testserver::{Body, Gate, Recorded, Reply, Script, TestServer, paused};
use concord_sdk_square_payments::{
    CompletePayment, CreatePayment, ERROR_BODY_LIMIT, Error, InputError, Money, PagesError, Resume,
    SUCCESS_BODY_LIMIT, Square, SquareConfig, bearer,
    concord::{Delivery, ErrorKind, bytes::Bytes},
    transport,
};
use serde_json::{Value, json};
use std::{num::NonZeroUsize, time::Duration};
use support::*;

fn kind(error: &Error) -> ErrorKind {
    let Error::Concord(error) = error else {
        panic!("expected Concord error")
    };
    error.kind()
}

fn redacted(value: &str) {
    for secret in [
        TOKEN,
        CURSOR,
        "caller-key-001",
        "cnon:synthetic-payment-token",
        "echoed-secret",
    ] {
        assert!(!value.contains(secret), "diagnostic disclosed a secret");
    }
}

#[tokio::test]
async fn wire_shapes_and_opaque_encoding() {
    Square::validate_header_names().unwrap();
    let server = TestServer::start(|request: &Recorded| {
        headers(request);
        if request.method == "POST" {
            Reply::json(200, PAYMENT)
        } else if request.path() == "/v2/payments" {
            Reply::json(200, PAGE_2)
        } else {
            Reply::json(200, PAYMENT)
        }
    })
    .await;
    let api = client(&server);
    api.payments().create(create(1000)).send().await.unwrap();
    api.payments().get("id/with ?%+").send().await.unwrap();
    api.payments()
        .list()
        .set_cursor(CURSOR)
        .set_limit(2u32)
        .send()
        .await
        .unwrap();
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].target, "/v2/payments");
    assert_eq!(requests[0].header_all("content-type"), ["application/json"]);
    assert_eq!(requests[0].header("idempotency-key"), None);
    assert_eq!(
        serde_json::from_slice::<Value>(&requests[0].body).unwrap(),
        serde_json::from_str::<Value>(REQUEST).unwrap()
    );
    assert_eq!(requests[1].method, "GET");
    assert_eq!(requests[1].target, "/v2/payments/id%2Fwith%20%3F%25%2B");
    assert!(requests[1].body.is_empty());
    assert_eq!(requests[1].header("content-type"), None);
    assert_eq!(
        requests[2].target,
        "/v2/payments?cursor=opaque%2B%2F%3D+%26%3F%25cursor&limit=2"
    );
    assert_eq!(requests[2].query_all("cursor"), [CURSOR]);
    assert!(requests[2].body.is_empty());
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[test]
fn input_validation_and_redaction() {
    let money = |amount, currency: &str| Money {
        amount,
        currency: currency.into(),
    };
    for (key, source, amount, currency, expected) in [
        ("", "token", 1, "USD", InputError::IdempotencyKey),
        ("key", "", 1, "USD", InputError::Source),
        ("key", "CASH", 1, "USD", InputError::Source),
        ("key", "EXTERNAL", 1, "USD", InputError::Source),
        ("key", "ccof:excluded", 1, "USD", InputError::Source),
        ("key", "token", 0, "USD", InputError::Amount),
        ("key", "token", -1, "USD", InputError::Amount),
        ("key", "token", 1, "usd", InputError::Currency),
        ("key", "token", 1, "US", InputError::Currency),
    ] {
        let error = CreatePayment::new(key, source, money(amount, currency)).unwrap_err();
        assert_eq!(error, expected);
        redacted(&format!("{error} {error:?}"));
    }
    assert!(CreatePayment::new("x".repeat(45), "token", money(1, "USD")).is_ok());
    assert_eq!(
        CreatePayment::new("x".repeat(46), "token", money(1, "USD")).unwrap_err(),
        InputError::IdempotencyKey
    );
    assert_eq!(
        CreatePayment::new("é".repeat(23), "token", money(1, "USD")).unwrap_err(),
        InputError::IdempotencyKey
    );
    for token in ["", "echoed-secret\n", "has space", "é"] {
        let error = bearer(token).unwrap_err();
        redacted(&format!("{error} {error:?}"));
    }
    redacted(&format!("{:?}", create(1000)));
}

#[tokio::test]
async fn build_failures_send_nothing() {
    let server = TestServer::start(|_: &Recorded| panic!("unexpected I/O")).await;
    let api = client(&server);
    for id in ["", ".", ".."] {
        let error = api.payments().get(id).send().await.unwrap_err();
        assert_eq!(kind(&error), ErrorKind::Build);
        assert!(!error.create_outcome_unknown());
        let Error::Concord(error) = error else {
            unreachable!()
        };
        assert_eq!(error.delivery(), Delivery::NotSent);
        redacted(&format!("{error} {error:?}"));
    }
    let error = api
        .with_base_url("https://user:echoed-secret@example.invalid")
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Build);
    redacted(&format!("{error} {error:?}"));
    assert_eq!(server.connections(), 0);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn documented_provider_statuses_categories_and_codes() {
    let cases: Vec<_> = include_str!("../fixtures/error-cases.tsv")
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            (fields[0].parse::<u16>().unwrap(), fields[1], fields[2])
        })
        .collect();
    let script = Script::new(cases.iter().map(|(status, category, code)| {
        Reply::json(*status, json!({"errors": [{
            "category": category, "code": code, "detail": "echoed-secret", "field": "echoed-secret"
        }]}).to_string()).header("retry-after", "2").header("set-cookie", "echoed-secret")
    }));
    let server = TestServer::start(move |_: &Recorded| script.next()).await;
    let api = client(&server);
    for (status, category, code) in &cases {
        let error = api
            .payments()
            .create(create(1000))
            .send()
            .await
            .unwrap_err();
        redacted(&format!("{error} {error:?}"));
        assert_eq!(error.create_outcome_unknown(), *status >= 500);
        let Error::Provider(error) = error else {
            panic!("expected HTTP error")
        };
        assert_eq!(error.meta.status().as_u16(), *status);
        assert_eq!(error.meta.headers()["retry-after"], "2");
        assert_eq!(
            error.is_idempotency_mismatch(),
            *code == "IDEMPOTENCY_KEY_REUSED"
        );
        let entries = &error.document.as_ref().unwrap().errors;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].category, *category);
        assert_eq!(entries[0].code, *code);
        assert_eq!(entries[0].detail.as_deref(), Some("echoed-secret"));
    }
    assert_eq!(
        server.requests().len(),
        cases.len(),
        "HTTP errors are never automatically retried"
    );
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn malformed_truncated_empty_wrong_shape_and_wrong_media() {
    let cases: Vec<Value> = serde_json::from_str(include_str!("../fixtures/faults.json")).unwrap();
    for status in [200, 503] {
        for case in &cases {
            let reply = match case.get("media").and_then(Value::as_str) {
                None => Reply::json(status, case["body"].as_str().unwrap()),
                Some("") => Reply::new(status).body(case["body"].as_str().unwrap()),
                Some(media) => Reply::text(status, media, case["body"].as_str().unwrap()),
            };
            let script = Script::new([reply]);
            let server = TestServer::start(move |_: &Recorded| script.next()).await;
            let error = client(&server)
                .payments()
                .create(create(1000))
                .send()
                .await
                .unwrap_err();
            assert!(error.create_outcome_unknown());
            redacted(&format!("{error} {error:?}"));
            let inner = match &error {
                Error::Concord(inner) if status == 200 => inner,
                Error::Provider(provider) if status == 503 => {
                    assert_eq!(provider.meta.status().as_u16(), 503);
                    provider.document.as_ref().unwrap_err()
                }
                _ => panic!("wrong error classification"),
            };
            assert_eq!(
                format!("{:?}", inner.kind()),
                case["kind"].as_str().unwrap(),
                "{}",
                case["name"]
            );
            assert_eq!(inner.head().unwrap().status().as_u16(), status);
            assert_eq!(server.requests().len(), 1);
            assert_eq!(server.shutdown().await.open_after, 0);
        }
    }
}

#[tokio::test]
async fn byte_limits_for_success_and_error_fixed_and_chunked() {
    for (status, limit) in [(200, SUCCESS_BODY_LIMIT), (503, ERROR_BODY_LIMIT)] {
        for chunked in [false, true] {
            let body = vec![b' '; limit + 1];
            let reply = if chunked {
                Reply::json(status, "")
                    .with_body(Body::Chunks(vec![(Duration::ZERO, Bytes::from(body))]))
            } else {
                Reply::json(status, body)
            };
            let script = Script::new([reply]);
            let server = TestServer::start(move |_: &Recorded| script.next()).await;
            let error = client(&server)
                .payments()
                .get("echoed-secret")
                .send()
                .await
                .unwrap_err();
            let inner = match &error {
                Error::Concord(inner) => inner,
                Error::Provider(provider) => provider.document.as_ref().unwrap_err(),
            };
            assert_eq!(inner.kind(), ErrorKind::Limit);
            assert_eq!(inner.head().unwrap().status().as_u16(), status);
            redacted(&format!("{error} {error:?}"));
            assert_eq!(server.shutdown().await.open_after, 0);
        }
    }
}

#[tokio::test]
async fn response_cut_off_mid_body_keeps_status() {
    for status in [200, 503] {
        let server = TestServer::start(move |_: &Recorded| {
            Reply::json(status, "").with_body(Body::FailAfter(vec![Bytes::from_static(
                b"{\"echoed-secret\":",
            )]))
        })
        .await;
        let error = client(&server)
            .payments()
            .create(create(1000))
            .send()
            .await
            .unwrap_err();
        assert!(error.create_outcome_unknown());
        redacted(&format!("{error} {error:?}"));
        let inner = match &error {
            Error::Concord(inner) => inner,
            Error::Provider(provider) => provider.document.as_ref().unwrap_err(),
        };
        assert_eq!(inner.kind(), ErrorKind::Transport);
        assert_eq!(inner.delivery(), Delivery::Answered);
        assert_eq!(inner.head().unwrap().status().as_u16(), status);
        assert_eq!(server.requests().len(), 1);
        assert_eq!(server.shutdown().await.open_after, 0);
    }
}

#[tokio::test]
async fn metadata_refused_before_stalled_body_and_repeated_media_refused() {
    let script = Script::new([
        Reply::text(200, "text/html", "").with_body(Body::Stall),
        Reply::json(200, PAYMENT).header("content-type", "application/json"),
    ]);
    let server = TestServer::start(move |_: &Recorded| script.next()).await;
    for _ in 0..2 {
        let error = client(&server)
            .payments()
            .get("id")
            .send()
            .await
            .unwrap_err();
        assert_eq!(kind(&error), ErrorKind::Metadata);
    }
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn all_sensitive_diagnostic_surfaces() {
    let server = TestServer::start(|_: &Recorded| {
        Reply::json(400, json!({"errors": [
        {"category": "echoed-secret", "code": "echoed-secret", "detail": TOKEN, "field": CURSOR},
        {"category": "future", "code": "future"}
    ]}).to_string()).header("x-diagnostic", "echoed-secret")
    })
    .await;
    let api = client(&server);
    redacted(&format!(
        "{api:?} {:?} {:?}",
        SquareConfig {
            authorization: bearer(TOKEN).unwrap()
        },
        api.payments()
    ));
    let operation = api.payments().list().set_cursor(CURSOR);
    redacted(&format!("{operation:?}"));
    redacted(&format!("{:?}", operation.prepare().unwrap()));
    let create = api.payments().create(create(1000));
    redacted(&format!("{create:?}"));
    redacted(&format!("{:?}", create.clone().prepare().unwrap()));
    let error = create.send().await.unwrap_err();
    redacted(&format!("{error} {error:?}"));
    assert!(std::error::Error::source(&error).is_none());
    let Error::Provider(error) = error else {
        unreachable!()
    };
    assert_eq!(error.document.as_ref().unwrap().errors.len(), 2);
    redacted(&format!("{:?}", error.document));
    let page: concord_sdk_square_payments::PaymentsPage = serde_json::from_str(PAGE_1).unwrap();
    redacted(&format!("{page:?}"));
    let parsed: concord_sdk_square_payments::ErrorDocument =
        serde_json::from_str(MISMATCH).unwrap();
    assert_eq!(parsed.errors[0].code, "IDEMPOTENCY_KEY_REUSED");
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn page_failure_preserves_typed_error_and_failed_cursor() {
    let script = Script::new([
        Reply::json(200, PAGE_1),
        Reply::json(
            429,
            r#"{"errors":[{"category":"RATE_LIMIT_ERROR","code":"RATE_LIMITED"}]}"#,
        ),
    ]);
    let server = TestServer::start(move |_: &Recorded| script.next()).await;
    let mut pages = client(&server)
        .payments()
        .list()
        .pages(Resume::Start, NonZeroUsize::new(3).unwrap());
    pages.next_page().await.unwrap().unwrap();
    let error = pages.next_page().await.unwrap_err();
    redacted(&format!("{error} {error:?}"));
    let PagesError::Provider {
        error: Error::Provider(provider),
        failed,
        ..
    } = error
    else {
        panic!("expected page failure")
    };
    assert_eq!(provider.meta.status().as_u16(), 429);
    assert_eq!(failed, Resume::At(CURSOR.to_owned()));
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Ended { .. })
    ));
    assert_eq!(server.requests().len(), 2);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn redirects_are_errors_and_never_followed() {
    let destination =
        TestServer::start(|_: &Recorded| panic!("redirect must not be followed")).await;
    let url = destination.url();
    let server =
        TestServer::start(move |_: &Recorded| Reply::new(307).header("location", &url)).await;
    let error = client(&server)
        .payments()
        .create(create(1000))
        .send()
        .await
        .unwrap_err();
    assert!(error.create_outcome_unknown());
    let Error::Provider(error) = error else {
        unreachable!()
    };
    assert_eq!(error.meta.status().as_u16(), 307);
    assert_eq!(destination.connections(), 0);
    assert_eq!(server.requests().len(), 1);
    assert_eq!(server.shutdown().await.open_after, 0);
    assert_eq!(destination.shutdown().await.open_after, 0);
}

// ------------------------------------------------ delayed capture (maintenance)

const AUTHORIZATION_REQUEST: &str = include_str!("../fixtures/authorization-request.json");
const AUTHORIZED: &str = include_str!("../fixtures/authorized-payment.json");
const VERSION_MISMATCH: &str = include_str!("../fixtures/version-mismatch.json");
const VERSION_TOKEN: &str = "synthetic-version-token-1";

fn authorization(amount: i64) -> CreatePayment {
    CreatePayment::authorization(
        "caller-key-001",
        "cnon:synthetic-payment-token",
        Money {
            amount,
            currency: "USD".into(),
        },
    )
    .unwrap()
}

/// The authorized fixture with another status, as complete/cancel answer it.
fn authorized_as(status: &str) -> String {
    let mut document: Value = serde_json::from_str(AUTHORIZED).unwrap();
    document["payment"]["status"] = json!(status);
    document["payment"]["version_token"] = json!("synthetic-version-token-2");
    document.to_string()
}

#[tokio::test]
async fn delayed_capture_wire_shapes() {
    let server = TestServer::start(|request: &Recorded| {
        headers(request);
        assert_eq!(request.method, "POST");
        assert!(
            request
                .headers
                .iter()
                .all(|(name, _)| !name.contains("idempotency"))
        );
        if request.path().ends_with("/complete") {
            Reply::json(200, authorized_as("COMPLETED"))
        } else if request.path().ends_with("/cancel") {
            Reply::json(200, authorized_as("CANCELED"))
        } else {
            Reply::json(200, AUTHORIZED)
        }
    })
    .await;
    let api = client(&server);
    let held = api
        .payments()
        .create(authorization(1000))
        .send()
        .await
        .unwrap()
        .payment;
    assert_eq!(held.status, "APPROVED");
    assert_eq!(held.version_token.as_deref(), Some(VERSION_TOKEN));
    let completed = api
        .payments()
        .complete("id/with ?%+", CompletePayment::expecting(VERSION_TOKEN))
        .send()
        .await
        .unwrap()
        .payment;
    assert_eq!(completed.status, "COMPLETED");
    api.payments()
        .complete("id", CompletePayment::unconditional())
        .send()
        .await
        .unwrap();
    let canceled = api
        .payments()
        .cancel("id/with ?%+")
        .send()
        .await
        .unwrap()
        .payment;
    assert_eq!(canceled.status, "CANCELED");

    let requests = server.requests();
    assert_eq!(requests.len(), 4, "one request per send");
    // Authorization only: the create body with `autocomplete: false`, nothing else changed.
    assert_eq!(requests[0].target, "/v2/payments");
    let sent: Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(
        sent,
        serde_json::from_str::<Value>(AUTHORIZATION_REQUEST).unwrap()
    );
    let mut immediate: Value = serde_json::from_str(REQUEST).unwrap();
    immediate["autocomplete"] = json!(false);
    assert_eq!(sent, immediate);
    // Complete with a token: one encoded ID segment, then `/complete`.
    assert_eq!(
        requests[1].target,
        "/v2/payments/id%2Fwith%20%3F%25%2B/complete"
    );
    assert_eq!(requests[1].header_all("content-type"), ["application/json"]);
    assert_eq!(
        serde_json::from_slice::<Value>(&requests[1].body).unwrap(),
        json!({ "version_token": VERSION_TOKEN })
    );
    // Complete without a token is Square's documented empty object, not an absent body.
    assert_eq!(requests[2].target, "/v2/payments/id/complete");
    assert_eq!(requests[2].header_all("content-type"), ["application/json"]);
    assert_eq!(requests[2].body, b"{}");
    // Cancel has no body.
    assert_eq!(
        requests[3].target,
        "/v2/payments/id%2Fwith%20%3F%25%2B/cancel"
    );
    assert!(requests[3].body.is_empty());
    assert_eq!(requests[3].header("content-type"), None);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn delayed_capture_errors_are_typed_and_never_resent() {
    let script = Script::new([
        Reply::json(400, VERSION_MISMATCH),
        // Square documents no status for VERSION_MISMATCH: the code decides.
        Reply::json(409, VERSION_MISMATCH),
        Reply::json(
            400,
            r#"{"errors":[{"category":"INVALID_REQUEST_ERROR","code":"DELAYED_TRANSACTION_CAPTURED"}]}"#,
        ),
        Reply::json(
            503,
            r#"{"errors":[{"category":"API_ERROR","code":"SERVICE_UNAVAILABLE"}]}"#,
        ),
        Reply::json(
            400,
            r#"{"errors":[{"category":"INVALID_REQUEST_ERROR","code":"DELAYED_TRANSACTION_CANCELED"}]}"#,
        ),
        Reply::json(
            500,
            r#"{"errors":[{"category":"API_ERROR","code":"INTERNAL_SERVER_ERROR"}]}"#,
        ),
        Reply::json(503, r#"{"errors":[]}"#),
    ]);
    let server = TestServer::start(move |_: &Recorded| script.next()).await;
    let api = client(&server);
    let complete = || {
        api.payments()
            .complete("echoed-secret", CompletePayment::expecting(VERSION_TOKEN))
            .send()
    };
    let cancel = || api.payments().cancel("echoed-secret").send();
    let provider = |error: Error| match error {
        Error::Provider(provider) => provider,
        Error::Concord(_) => panic!("expected an HTTP error"),
    };
    // (outcome_unknown, is_version_mismatch, status) per send.
    let mut seen = Vec::new();
    for error in [
        complete().await.unwrap_err(),
        complete().await.unwrap_err(),
        complete().await.unwrap_err(),
        complete().await.unwrap_err(),
        cancel().await.unwrap_err(),
        cancel().await.unwrap_err(),
    ] {
        redacted(&format!("{error} {error:?}"));
        assert!(!format!("{error} {error:?}").contains(VERSION_TOKEN));
        assert!(!error.create_outcome_unknown(), "not a create");
        let unknown = error.outcome_unknown();
        let provider = provider(error);
        assert!(!provider.is_idempotency_mismatch());
        seen.push((
            unknown,
            provider.is_version_mismatch(),
            provider.meta.status().as_u16(),
        ));
    }
    assert_eq!(
        seen,
        [
            (false, true, 400),
            (false, true, 409),
            (false, false, 400),
            (true, false, 503),
            (false, false, 400),
            (true, false, 500),
        ]
    );
    // A read's server error is never an unknown write outcome.
    let error = api.payments().get("id").send().await.unwrap_err();
    assert!(!error.outcome_unknown());
    assert_eq!(server.requests().len(), 7, "nothing was resent");

    // Refused before sending: known not to have happened.
    for error in [
        api.payments()
            .complete("..", CompletePayment::unconditional())
            .send()
            .await
            .unwrap_err(),
        api.payments().cancel("").send().await.unwrap_err(),
    ] {
        assert_eq!(kind(&error), ErrorKind::Build);
        assert!(!error.outcome_unknown());
    }
    assert_eq!(server.requests().len(), 7);
    let operation = api
        .payments()
        .complete("echoed-secret", CompletePayment::expecting(VERSION_TOKEN));
    for text in [
        format!("{operation:?}"),
        format!("{:?}", operation.prepare().unwrap()),
        format!("{:?}", CompletePayment::expecting(VERSION_TOKEN)),
    ] {
        redacted(&text);
        assert!(!text.contains(VERSION_TOKEN));
    }
    assert_eq!(server.shutdown().await.open_after, 0);
}

// Paused clock: the 150 ms timeout fires for each write only once the server holds it.
#[tokio::test(start_paused = true)]
async fn timed_out_complete_and_cancel_are_unknown_but_not_create_replays() {
    // Each write commits and then holds its response head; the transport times out.
    let server = TestServer::start(|_: &Recorded| {
        Reply::json(200, authorized_as("COMPLETED")).after(Gate::new())
    })
    .await;
    let timeout = Duration::from_millis(150);
    let api = Square::new(
        transport(timeout).unwrap(),
        SquareConfig {
            authorization: bearer(TOKEN).unwrap(),
        },
    )
    .with_base_url(server.url())
    .unwrap();
    let payments = api.payments();
    let complete = payments
        .complete(
            "synthetic-payment-002",
            CompletePayment::expecting(VERSION_TOKEN),
        )
        .send();
    let complete = paused::time_out(&server, |_| true, timeout, complete)
        .await
        .unwrap_err();
    let cancel = payments.cancel("synthetic-payment-002").send();
    let cancel = paused::time_out(&server, |_| true, timeout, cancel)
        .await
        .unwrap_err();
    for error in [&complete, &cancel] {
        assert!(error.outcome_unknown());
        assert!(!error.create_outcome_unknown(), "re-read, never replay");
        let Error::Concord(inner) = error else {
            panic!("expected a transport timeout")
        };
        assert!(inner.transport().unwrap().is_timeout());
        assert_eq!(inner.delivery(), Delivery::Unknown);
    }
    let create = payments.create(authorization(1000)).send();
    let create = paused::time_out(&server, |_| true, timeout, create)
        .await
        .unwrap_err();
    assert!(create.outcome_unknown() && create.create_outcome_unknown());
    let requests = server.requests();
    assert_eq!(requests.len(), 3, "no automatic resend of any write");
    assert!(requests[0].target.ends_with("/complete"));
    assert!(requests[1].target.ends_with("/cancel"));
    assert_eq!(server.shutdown().await.open_after, 0);
}

//! Wire-level shapes, every documented error class, malformed / truncated / oversized /
//! wrong-media bodies, the destination binding and redaction. Expectations come from the cited
//! documentation (see PROVENANCE.md) and the fixtures, not from the implementation.

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use concord_sdk_paypal::concord::ErrorKind;
use concord_sdk_paypal::concord::bytes::Bytes;
use concord_sdk_paypal::{
    BODY_LIMIT, CaptureId, CaptureRequest, CaptureStatus, Credentials, ERROR_BODY_LIMIT, Error,
    Intent, Money, OrderId, OrderRequest, OrderStatus, PayPal, PurchaseUnitRequest, RequestId,
    TOKEN_BODY_LIMIT, TokenError, UncertainCause, http_client,
};
use serde_json::Value;
use support::*;

fn order_id() -> OrderId {
    OrderId::new(ORDER).unwrap()
}

fn order_request() -> OrderRequest {
    let unit = PurchaseUnitRequest::new(Money::new("USD", "100.00").unwrap())
        .unwrap()
        .reference_id("synthetic-unit-1")
        .unwrap()
        .invoice_id("SYNTH-INV-0001")
        .unwrap();
    OrderRequest::capture(vec![unit]).unwrap()
}

fn body_json(r: &Recorded) -> Value {
    serde_json::from_slice(&r.body).unwrap()
}

async fn get_order(paypal: &PayPal) -> Result<concord_sdk_paypal::Order, Error> {
    paypal.order(order_id()).get().send().await
}

async fn capture(paypal: &PayPal) -> Result<concord_sdk_paypal::Order, Error> {
    paypal
        .order(order_id())
        .capture(CaptureRequest::new())
        .send()
        .await
}

// ---------------------------------------------------------------------------------------------
// Wire shapes

/// Authentication page: `POST /v1/oauth2/token`, Basic client ID and secret, form
/// `grant_type=client_credentials`. Requests page: `Accept` for a response body.
#[tokio::test]
async fn the_token_request_is_the_documented_client_credentials_grant() {
    let fake = FakePayPal::new();
    let server = fake.start().await;
    get_order(&api(&server)).await.unwrap();

    let token = &server.requests()[0];
    assert_eq!(token.method, "POST");
    assert_eq!(token.target, TOKEN_PATH);
    assert_eq!(token.header("authorization"), Some(BASIC));
    assert_eq!(
        token.header("content-type"),
        Some("application/x-www-form-urlencoded")
    );
    assert_eq!(token.body_text(), "grant_type=client_credentials");
    assert_eq!(token.header("accept"), Some("application/json"));
}

/// Create order: `POST /v2/checkout/orders`, Bearer token, JSON body, `PayPal-Request-Id`,
/// `Prefer: return=representation`.
#[tokio::test]
async fn create_order_sends_the_documented_request() {
    let fake = FakePayPal::new();
    let server = fake.start().await;
    api(&server)
        .orders()
        .create(order_request())
        .set_request_id(RequestId::new("synthetic-create-0001").unwrap())
        .send()
        .await
        .unwrap();

    let create = &server.requests()[1];
    assert_eq!(create.method, "POST");
    assert_eq!(create.target, ORDERS_PATH);
    assert_eq!(
        create.header("authorization"),
        Some(bearer_of("token.json").as_str())
    );
    assert_eq!(create.header("content-type"), Some("application/json"));
    assert_eq!(create.header("accept"), Some("application/json"));
    assert_eq!(create.header("prefer"), Some("return=representation"));
    assert_eq!(
        create.header("paypal-request-id"),
        Some("synthetic-create-0001")
    );
    assert_eq!(body_json(create), fixture_json("create-order-request.json"));
}

/// Show order details and Show captured payment details are bodiless `GET`s; Capture payment for
/// order sends the empty object (the specification's request examples) as JSON.
#[tokio::test]
async fn reads_and_capture_send_the_documented_requests() {
    let fake = FakePayPal::new();
    let server = fake.start().await;
    fake.approve();
    let paypal = api(&server);
    get_order(&paypal).await.unwrap();
    paypal
        .order(order_id())
        .capture(CaptureRequest::new())
        .set_request_id(RequestId::new("synthetic-capture-0001").unwrap())
        .send()
        .await
        .unwrap();
    paypal
        .capture(CaptureId::new(CAPTURE).unwrap())
        .get()
        .send()
        .await
        .unwrap();

    let requests = server.requests();
    let (read, capture, shown) = (&requests[1], &requests[2], &requests[3]);
    for get in [read, shown] {
        assert_eq!(get.method, "GET");
        assert!(get.body.is_empty());
        assert_eq!(get.header("content-type"), None);
        assert_eq!(get.header("paypal-request-id"), None);
        assert_eq!(get.header("accept"), Some("application/json"));
    }
    assert_eq!(read.target, ORDER_PATH);
    assert_eq!(shown.target, CAPTURE_PATH);
    assert_eq!(capture.method, "POST");
    assert_eq!(capture.target, CAPTURE_ORDER_PATH);
    assert_eq!(capture.body_text(), "{}");
    assert_eq!(capture.header("content-type"), Some("application/json"));
    assert_eq!(capture.header("prefer"), Some("return=representation"));
    assert_eq!(
        capture.header("paypal-request-id"),
        Some("synthetic-capture-0001")
    );
}

/// A capture ID is one path segment, whatever it contains.
#[tokio::test]
async fn a_capture_id_is_one_percent_encoded_segment() {
    let server = serve(|_| json_fixture(200, "capture.json")).await;
    api(&server)
        .capture(CaptureId::new("A/B?C").unwrap())
        .get()
        .send()
        .await
        .unwrap();
    assert_eq!(
        server.requests()[1].target,
        "/v2/payments/captures/A%2FB%3FC"
    );
}

/// `prepare()` is the declared request: no credential, no I/O.
#[tokio::test]
async fn prepare_is_undecorated_and_sends_nothing() {
    let fake = FakePayPal::new();
    let server = fake.start().await;
    let prepared = api(&server).order(order_id()).get().prepare().unwrap();
    assert!(prepared.request().headers().get("authorization").is_none());
    assert!(server.requests().is_empty());
}

/// Inputs are refused before any request is built.
#[test]
fn inputs_are_validated_before_any_request() {
    for bad in ["", "lowercase1", "HAS-DASH", &"A".repeat(37)] {
        assert!(
            matches!(OrderId::new(bad), Err(Error::Invalid(_))),
            "{bad:?}"
        );
    }
    assert!(OrderId::new("A".repeat(36)).is_ok());
    for bad in ["", "a b", "tab\t", &"x".repeat(65)] {
        assert!(CaptureId::new(bad).is_err(), "{bad:?}");
    }
    for bad in ["", "has space", "line\nbreak", &"r".repeat(10_001)] {
        assert!(RequestId::new(bad).is_err(), "{bad:?}");
    }
    assert!(RequestId::new("r".repeat(10_000)).is_ok());

    for (currency, value) in [("usd", "1.00"), ("US", "1.00"), ("USD", "1e3"), ("USD", "")] {
        assert!(Money::new(currency, value).is_err(), "{currency} {value}");
    }
    assert!(Money::new("USD", "1".repeat(33)).is_err());
    for value in ["1", "1.5", ".50", "-1.00", "0001"] {
        assert!(Money::new("USD", value).is_ok(), "{value}");
    }
    for value in ["0", "0.00", "-1.00"] {
        let money = Money::new("USD", value).unwrap();
        assert!(PurchaseUnitRequest::new(money).is_err(), "{value}");
    }

    let unit = |reference: Option<&str>| {
        let unit = PurchaseUnitRequest::new(Money::new("USD", "1.00").unwrap()).unwrap();
        match reference {
            Some(r) => unit.reference_id(r).unwrap(),
            None => unit,
        }
    };
    assert!(OrderRequest::capture(vec![]).is_err());
    assert!(OrderRequest::capture((0..11).map(|i| unit(Some(&i.to_string()))).collect()).is_err());
    assert!(OrderRequest::capture(vec![unit(Some("a")), unit(Some("a"))]).is_err());
    assert!(OrderRequest::capture(vec![unit(Some("a")), unit(None)]).is_err());
    assert!(OrderRequest::capture(vec![unit(Some("a")), unit(Some("b"))]).is_ok());
    assert!(OrderRequest::capture(vec![unit(None)]).is_ok());
    let base = unit(None);
    assert!(base.clone().reference_id("").is_err());
    assert!(base.clone().invoice_id("i".repeat(128)).is_err());
    assert!(base.clone().custom_id("c".repeat(256)).is_err());
    assert!(base.description("d".repeat(3001)).is_err());

    for (id, secret) in [("", "s"), ("i", ""), ("a:b", "s"), ("i", "line\nbreak")] {
        assert!(Credentials::new(id, secret).is_err(), "{id:?}");
    }
}

// ---------------------------------------------------------------------------------------------
// Documented error classes

/// The errors pages' classes on a read: answered at once, except `5xx` (repeated within the read
/// bounds) and `401` (repeated once with a new token). `429` is never repeated: PayPal publishes
/// no quota.
#[tokio::test]
async fn every_documented_error_class_on_a_read() {
    let cases: [(u16, &str, &str, Option<&str>, usize); 9] = [
        (
            400,
            "error-invalid-request.json",
            "INVALID_REQUEST",
            Some("INVALID_PARAMETER_VALUE"),
            1,
        ),
        (
            401,
            "error-authentication-failure.json",
            "AUTHENTICATION_FAILURE",
            None,
            2,
        ),
        (
            403,
            "error-not-authorized.json",
            "NOT_AUTHORIZED",
            Some("PERMISSION_DENIED"),
            1,
        ),
        (
            404,
            "error-resource-not-found.json",
            "RESOURCE_NOT_FOUND",
            Some("INVALID_RESOURCE_ID"),
            1,
        ),
        (
            409,
            "error-previous-request-in-progress.json",
            "RESOURCE_CONFLICT",
            Some("PREVIOUS_REQUEST_IN_PROGRESS"),
            1,
        ),
        (
            415,
            "error-unsupported-media-type.json",
            "UNSUPPORTED_MEDIA_TYPE",
            Some("UNSUPPORTED_MEDIA_TYPE"),
            1,
        ),
        (
            429,
            "error-rate-limit-reached.json",
            "RATE_LIMIT_REACHED",
            None,
            1,
        ),
        (
            500,
            "error-internal-server-error.json",
            "INTERNAL_SERVER_ERROR",
            None,
            3,
        ),
        (
            503,
            "error-service-unavailable.json",
            "SERVICE_UNAVAILABLE",
            None,
            3,
        ),
    ];
    for (status, file, name, issue, attempts) in cases {
        let server = serve(move |_| json_fixture(status, file)).await;
        let error = get_order(&api(&server)).await.unwrap_err();
        let Error::Http(http) = &error else {
            panic!("{status}: expected Http, got {error:?}")
        };
        assert_eq!(http.status().as_u16(), status);
        assert_eq!(http.name(), Some(name), "{status}");
        if let Some(issue) = issue {
            assert!(http.has_issue(issue), "{status}");
        }
        let expected = fixture_json(file);
        assert_eq!(
            http.document().unwrap().debug_id(),
            expected["debug_id"].as_str()
        );
        assert_eq!(http.attempts(), attempts, "{status}");
        assert_eq!(count(&server, "GET", ORDER_PATH), attempts, "{status}");
        assert_eq!(http.is_rate_limited(), status == 429);
        assert_eq!(http.operation(), "PayPal.order.get");
    }
}

/// The same classes on a capture without a request ID: sent once; a `4xx` is a definite
/// refusal, a `5xx` may have captured.
#[tokio::test]
async fn every_documented_error_class_on_a_capture_without_request_id() {
    let definite = [
        (400, "error-invalid-request.json"),
        (401, "error-authentication-failure.json"),
        (403, "error-not-authorized.json"),
        (404, "error-resource-not-found.json"),
        (409, "error-previous-request-in-progress.json"),
        (415, "error-unsupported-media-type.json"),
        (422, "error-order-not-approved.json"),
        (429, "error-rate-limit-reached.json"),
    ];
    for (status, file) in definite {
        let server = serve(move |_| json_fixture(status, file)).await;
        let error = capture(&api(&server)).await.unwrap_err();
        assert_eq!(
            error.status().map(|s| s.as_u16()),
            Some(status),
            "{error:?}"
        );
        assert_eq!(count(&server, "POST", CAPTURE_ORDER_PATH), 1, "{status}");
    }
    for (status, file) in [
        (500, "error-internal-server-error.json"),
        (503, "error-service-unavailable.json"),
    ] {
        let server = serve(move |_| json_fixture(status, file)).await;
        let error = capture(&api(&server)).await.unwrap_err();
        let uncertain = error.uncertain().expect("uncertain");
        let UncertainCause::ServerError(http) = uncertain.cause() else {
            panic!("{status}: {:?}", uncertain.cause())
        };
        assert_eq!(http.status().as_u16(), status);
        assert_eq!(count(&server, "POST", CAPTURE_ORDER_PATH), 1, "{status}");
    }
}

/// A create with a request ID is repeated after a `5xx` with the same ID and body; without one
/// it is sent once and uncertain.
#[tokio::test]
async fn a_create_is_repeated_only_with_its_request_id() {
    let seen = Arc::new(AtomicUsize::new(0));
    let first_fails = {
        let seen = seen.clone();
        move |_: &Recorded| {
            if seen.fetch_add(1, Ordering::SeqCst) == 0 {
                json_fixture(500, "error-internal-server-error.json")
            } else {
                json_fixture(201, "order-created.json")
            }
        }
    };
    let server = serve(first_fails).await;
    let paypal = api(&server);
    let order = paypal
        .orders()
        .create(order_request())
        .set_request_id(RequestId::new("synthetic-create-0002").unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(order.status, OrderStatus::Created);
    let creates: Vec<_> = server
        .requests()
        .into_iter()
        .filter(|r| r.path() == ORDERS_PATH)
        .collect();
    assert_eq!(creates.len(), 2);
    assert_eq!(
        creates[0].header("paypal-request-id"),
        creates[1].header("paypal-request-id")
    );
    assert_eq!(creates[0].body, creates[1].body);

    let server = serve(|_| json_fixture(500, "error-internal-server-error.json")).await;
    let error = api(&server)
        .orders()
        .create(order_request())
        .send()
        .await
        .unwrap_err();
    assert_eq!(
        error.uncertain().unwrap().operation(),
        "PayPal.orders.create"
    );
    assert_eq!(count(&server, "POST", ORDERS_PATH), 1);
}

/// A refusal after an uncertain attempt does not settle it: the earlier attempt may have
/// captured (here `409 PREVIOUS_REQUEST_IN_PROGRESS` after a `500`).
#[tokio::test]
async fn a_refusal_after_an_uncertain_attempt_stays_uncertain() {
    let seen = Arc::new(AtomicUsize::new(0));
    let server = serve({
        let seen = seen.clone();
        move |_| {
            if seen.fetch_add(1, Ordering::SeqCst) == 0 {
                json_fixture(500, "error-internal-server-error.json")
            } else {
                json_fixture(409, "error-previous-request-in-progress.json")
            }
        }
    })
    .await;
    let error = api(&server)
        .order(order_id())
        .capture(CaptureRequest::new())
        .set_request_id(RequestId::new("synthetic-capture-0005").unwrap())
        .send()
        .await
        .unwrap_err();
    let uncertain = error.uncertain().expect("uncertain");
    let UncertainCause::LaterRefusal(http) = uncertain.cause() else {
        panic!("{:?}", uncertain.cause())
    };
    assert!(http.has_issue("PREVIOUS_REQUEST_IN_PROGRESS"));
    assert_eq!(uncertain.attempts(), 2);
}

/// PayPal documents no `Retry-After`; one that arrives is reported, never waited for.
#[tokio::test]
async fn a_429_is_returned_with_any_retry_after_and_not_repeated() {
    let server =
        serve(|_| json_fixture(429, "error-rate-limit-reached.json").header("retry-after", "7"))
            .await;
    let error = get_order(&api(&server)).await.unwrap_err();
    let Error::Http(http) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(http.retry_after(), Ok(Some(Duration::from_secs(7))));
    assert_eq!(count(&server, "GET", ORDER_PATH), 1);

    let server = serve(|_| json_fixture(429, "error-rate-limit-reached.json")).await;
    let Error::Http(http) = get_order(&api(&server)).await.unwrap_err() else {
        panic!()
    };
    assert_eq!(http.retry_after(), Ok(None));

    let server = serve(|_| {
        json_fixture(429, "error-rate-limit-reached.json")
            .header("retry-after", "1")
            .header("retry-after", "2")
    })
    .await;
    let Error::Http(http) = get_order(&api(&server)).await.unwrap_err() else {
        panic!()
    };
    assert!(http.retry_after().is_err());
}

/// The responses page: wrong client credentials are `401` `invalid_client`. Nothing else is
/// sent, and the failure is not cached.
#[tokio::test]
async fn a_refused_token_request_sends_no_api_request_and_is_not_cached() {
    let server =
        TestServer::start(|_: &Recorded| json_fixture(401, "token-invalid-client.json")).await;
    let paypal = api(&server);
    for _ in 0..2 {
        let error = get_order(&paypal).await.unwrap_err();
        let Error::Token(TokenError::Refused { status, document }) = &error else {
            panic!("{error:?}")
        };
        assert_eq!(status.as_u16(), 401);
        assert_eq!(document.as_ref().unwrap().error(), "invalid_client");
    }
    assert_eq!(
        targets(&server),
        [format!("POST {TOKEN_PATH}"), format!("POST {TOKEN_PATH}")]
    );
}

/// Concurrent callers waiting on one failed acquisition all receive its failure.
#[tokio::test]
async fn concurrent_callers_share_one_failed_token_request() {
    let server = TestServer::start(|_: &Recorded| {
        json_fixture(401, "token-invalid-client.json").delay(Duration::from_millis(200))
    })
    .await;
    let paypal = api(&server);
    let mut calls = tokio::task::JoinSet::new();
    for _ in 0..4 {
        let paypal = paypal.clone();
        calls.spawn(async move { get_order(&paypal).await });
    }
    while let Some(result) = calls.join_next().await {
        assert!(matches!(
            result.unwrap(),
            Err(Error::Token(TokenError::Refused { .. }))
        ));
    }
    assert_eq!(server.requests().len(), 1);
}

/// Token documents that cannot be used: not `Bearer`, a lifetime inside the refresh margin, the
/// wrong media type, malformed or oversized JSON.
#[tokio::test]
async fn unusable_token_documents_are_refused() {
    let oversized = format!(r#"{{"padding": "{}"}}"#, "x".repeat(TOKEN_BODY_LIMIT));
    let cases: Vec<(Reply, TokenError)> = vec![
        (
            json_fixture(200, "token-not-bearer.json"),
            TokenError::Unusable("token_type is not Bearer"),
        ),
        (
            json_fixture(200, "token-short-lived.json"),
            TokenError::Unusable("expires_in is not longer than the refresh margin"),
        ),
        (
            Reply::text(200, "text/html", fixture("token.json")),
            TokenError::Failed {
                kind: ErrorKind::Metadata,
            },
        ),
        (
            Reply::new(200)
                .header("content-type", "application/json")
                .body("{\"access_token\": \"A21AA"),
            TokenError::Failed {
                kind: ErrorKind::Decode,
            },
        ),
        (
            Reply::new(200)
                .header("content-type", "application/json")
                .body(oversized),
            TokenError::Failed {
                kind: ErrorKind::Limit,
            },
        ),
    ];
    for (reply, expected) in cases {
        let reply = Arc::new(std::sync::Mutex::new(Some(reply)));
        let server = TestServer::start(move |_: &Recorded| {
            reply
                .lock()
                .unwrap()
                .take()
                .unwrap_or_else(|| Reply::new(599))
        })
        .await;
        let error = get_order(&api(&server)).await.unwrap_err();
        let Error::Token(token) = &error else {
            panic!("{error:?}")
        };
        assert_eq!(token, &expected);
        assert_eq!(
            server.requests().len(),
            1,
            "no API request after {expected:?}"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Malformed, truncated, oversized and wrong-media bodies

/// A scripted successful answer that cannot be read.
type BadSuccess = (
    &'static str,
    Box<dyn Fn() -> Reply + Send + Sync>,
    ErrorKind,
);

fn bad_successes() -> Vec<BadSuccess> {
    vec![
        (
            "truncated",
            Box::new(|| json_fixture(200, "order-truncated.json")),
            ErrorKind::Decode,
        ),
        (
            "wrong shape",
            Box::new(|| json_fixture(200, "order-wrong-shape.json")),
            ErrorKind::Decode,
        ),
        (
            "html",
            Box::new(|| Reply::text(200, "text/html", "<html>synthetic proxy</html>")),
            ErrorKind::Metadata,
        ),
        (
            "unlabelled",
            Box::new(|| Reply::new(200).body(fixture("order-created.json"))),
            ErrorKind::Metadata,
        ),
        (
            "oversized",
            Box::new(|| {
                Reply::new(200)
                    .header("content-type", "application/json")
                    .body(vec![b' '; BODY_LIMIT + 1])
            }),
            ErrorKind::Limit,
        ),
        (
            "cut off",
            Box::new(|| {
                Reply::new(200)
                    .header("content-type", "application/json")
                    .with_body(Body::FailAfter(vec![Bytes::from_static(
                        b"{\"id\": \"SYNTH",
                    )]))
            }),
            ErrorKind::Transport,
        ),
    ]
}

/// On a read: the Concord error of that stage, sent once (the answer arrived).
#[tokio::test]
async fn unreadable_successful_reads_fail_with_their_stage() {
    for (label, reply, kind) in bad_successes() {
        let server = serve(move |_| reply()).await;
        let error = get_order(&api(&server)).await.unwrap_err();
        let Error::Concord(e) = &error else {
            panic!("{label}: {error:?}")
        };
        assert_eq!(e.kind(), kind, "{label}");
        assert_eq!(count(&server, "GET", ORDER_PATH), 1, "{label}");
    }
}

/// On a capture: the capture happened and its result is unknown.
#[tokio::test]
async fn unreadable_successful_captures_are_uncertain() {
    for (label, reply, kind) in bad_successes() {
        let server = serve(move |_| reply()).await;
        let error = capture(&api(&server)).await.unwrap_err();
        let uncertain = error
            .uncertain()
            .unwrap_or_else(|| panic!("{label}: {error:?}"));
        let UncertainCause::UnreadableSuccess(e) = uncertain.cause() else {
            panic!("{label}: {:?}", uncertain.cause())
        };
        assert_eq!(e.kind(), kind, "{label}");
        assert_eq!(count(&server, "POST", CAPTURE_ORDER_PATH), 1, "{label}");
    }
}

/// An error document that cannot be read keeps the status.
#[tokio::test]
async fn unreadable_error_documents_keep_the_status() {
    let cases: Vec<(Reply, ErrorKind)> = vec![
        (
            Reply::text(422, "text/html", "<html>synthetic gateway</html>"),
            ErrorKind::Metadata,
        ),
        (Reply::new(422), ErrorKind::Metadata),
        (
            Reply::new(422)
                .header("content-type", "application/json")
                .body("{\"name\": \"UNPRO"),
            ErrorKind::Decode,
        ),
        (
            Reply::new(422)
                .header("content-type", "application/json")
                .body(vec![b' '; ERROR_BODY_LIMIT + 1]),
            ErrorKind::Limit,
        ),
    ];
    for (reply, kind) in cases {
        let reply = Arc::new(std::sync::Mutex::new(Some(reply)));
        let server = serve(move |_| reply.lock().unwrap().take().unwrap()).await;
        let error = get_order(&api(&server)).await.unwrap_err();
        let Error::Http(http) = &error else {
            panic!("{error:?}")
        };
        assert_eq!(http.status().as_u16(), 422);
        assert_eq!(http.document().unwrap_err().kind(), kind);
        assert!(error.to_string().contains("HTTP 422"));
    }
}

/// Values outside the documented enumerations are kept, unknown fields ignored.
#[tokio::test]
async fn unknown_values_are_kept_as_sent() {
    let server = serve(|_| json_fixture(200, "order-unknown-values.json")).await;
    let order = get_order(&api(&server)).await.unwrap();
    assert_eq!(
        order.status,
        OrderStatus::Other("SYNTHETIC_FUTURE_STATUS".into())
    );
    assert_eq!(
        order.intent,
        Some(Intent::Other("SYNTHETIC_FUTURE_INTENT".into()))
    );
    let capture = order.captures().next().unwrap();
    assert_eq!(capture.status, CaptureStatus::Pending);
    assert_eq!(
        capture.status_details.as_ref().unwrap().reason.as_deref(),
        Some("SYNTHETIC_FUTURE_REASON")
    );
    assert!(order.approval_url().is_none());
}

// ---------------------------------------------------------------------------------------------
// Transport, destination and deadlines

/// The session's credentials belong to one origin: a client moved elsewhere sends nothing, not
/// even a token request.
#[tokio::test]
async fn a_client_moved_to_another_origin_sends_nothing() {
    let home = FakePayPal::new().start().await;
    let elsewhere = serve(|_| json_fixture(200, "order-created.json")).await;
    let moved = api(&home).with_base_url(elsewhere.url()).unwrap();
    let error = get_order(&moved).await.unwrap_err();
    assert!(matches!(error, Error::Destination), "{error:?}");
    assert!(home.requests().is_empty());
    assert!(elsewhere.requests().is_empty());
}

/// `http_client()` follows no redirect: a `302` is the answer.
#[tokio::test]
async fn the_sdk_transport_does_not_follow_redirects() {
    let server = serve(|r| {
        if r.path() == ORDER_PATH {
            Reply::new(302).header("location", "/elsewhere")
        } else {
            json_fixture(200, "order-created.json")
        }
    })
    .await;
    let paypal = PayPal::builder(http_client().unwrap(), credentials())
        .base_url(server.url())
        .build()
        .unwrap();
    let error = get_order(&paypal).await.unwrap_err();
    assert_eq!(error.status().map(|s| s.as_u16()), Some(302));
    assert_eq!(
        targets(&server),
        [format!("POST {TOKEN_PATH}"), format!("GET {ORDER_PATH}")]
    );
}

/// A write deadline that cancels an attempt in flight leaves the capture uncertain; a read
/// deadline with no completed attempt is `Deadline`.
#[tokio::test]
async fn a_deadline_that_cancels_an_attempt_is_reported() {
    let late =
        |_: &Recorded| json_fixture(201, "order-captured.json").delay(Duration::from_secs(5));
    let server = serve(late).await;
    let mut bounded = fast(2);
    bounded.timeout = Some(Duration::from_millis(200));
    let paypal = builder(&server)
        .writes(bounded)
        .reads(bounded)
        .build()
        .unwrap();

    let error = paypal
        .order(order_id())
        .capture(CaptureRequest::new())
        .set_request_id(RequestId::new("synthetic-capture-0006").unwrap())
        .send()
        .await
        .unwrap_err();
    assert!(matches!(
        error.uncertain().map(|u| u.cause()),
        Some(UncertainCause::Interrupted)
    ));

    let server = serve(|_| Reply::new(200).delay(Duration::from_secs(5))).await;
    let paypal = builder(&server).reads(bounded).build().unwrap();
    assert!(matches!(get_order(&paypal).await, Err(Error::Deadline)));
}

// ---------------------------------------------------------------------------------------------
// Redaction

/// No credential, access token, provider message or echoed value appears in formatted output.
#[tokio::test]
async fn secrets_and_provider_text_never_appear_in_formatted_output() {
    let token = fixture_json("token.json")["access_token"]
        .as_str()
        .unwrap()
        .to_owned();
    let secrets = [
        CLIENT_SECRET,
        CLIENT_ID,
        BASIC,
        "c3ludGhldGlj",
        token.as_str(),
    ];
    let provider_text = [
        "synthetic-echoed-value",
        "A parameter value is not valid",
        "Request is not well-formed",
        "Client Authentication failed",
    ];
    let check = |label: &str, shown: String| {
        for needle in secrets.iter().chain(&provider_text) {
            assert!(!shown.contains(needle), "{label} shows {needle:?}: {shown}");
        }
    };

    let server = serve(|_| json_fixture(400, "error-invalid-request.json")).await;
    let paypal = api(&server);
    check("credentials", format!("{:?}", credentials()));
    check("client", format!("{paypal:?}"));
    check("builder", format!("{:?}", builder(&server)));
    let order = paypal.order(order_id());
    check("scope", format!("{order:?}"));
    check("prepared", format!("{:?}", order.get().prepare().unwrap()));

    let error = get_order(&paypal).await.unwrap_err();
    check("http display", error.to_string());
    check("http debug", format!("{error:?}"));
    assert!(error.to_string().contains("INVALID_PARAMETER_VALUE"));
    assert!(error.to_string().contains("synthdebug400"));

    let server = serve(|_| json_fixture(500, "error-internal-server-error.json")).await;
    let error = capture(&api(&server)).await.unwrap_err();
    check("uncertain display", error.to_string());
    check("uncertain debug", format!("{error:?}"));

    let server =
        TestServer::start(|_: &Recorded| json_fixture(401, "token-invalid-client.json")).await;
    let error = get_order(&api(&server)).await.unwrap_err();
    check("token display", error.to_string());
    check("token debug", format!("{error:?}"));

    // A token document that fails to decode quotes nothing of its body.
    let body = format!(r#"{{"access_token": "{token}", "token_type": 7}}"#);
    let server = TestServer::start(move |_: &Recorded| {
        Reply::new(200)
            .header("content-type", "application/json")
            .body(body.clone())
    })
    .await;
    let error = get_order(&api(&server)).await.unwrap_err();
    check("token decode display", error.to_string());
    check("token decode debug", format!("{error:?}"));
}

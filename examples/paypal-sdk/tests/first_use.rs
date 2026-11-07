//! Fresh consumer application: public README and rustdoc only; all traffic is loopback.

use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_paypal::{
    CaptureRequest, CaptureStatus, Credentials, Error, Money, OrderId, OrderRequest, OrderStatus,
    PayPal, PurchaseUnitRequest, RequestId, TokenError, UncertainCause, http_client,
};
use concord_sdk_paypal::{concord::ErrorKind, concord_retry::Policy};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    num::NonZeroUsize,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

const CLIENT: &str = "fixture-client-paypal-32";
const SECRET: &str = "fixture-secret-paypal-32";
const BASIC: &str = "Basic Zml4dHVyZS1jbGllbnQtcGF5cGFsLTMyOmZpeHR1cmUtc2VjcmV0LXBheXBhbC0zMg==";
const TOKEN_A: &str = "fixture-access-token-A";
const TOKEN_B: &str = "fixture-access-token-B";
const TOKEN_C: &str = "fixture-access-token-C";
const ORDER: &str = "5O190127TN364715T";
const CAPTURE: &str = "3C679366HH908993F";
const PENDING_CAPTURE: &str = "74L756601X447022Y";
const CREATE_ID: &str = "create-cart-2026-117";
const CAPTURE_ID: &str = "capture-cart-2026-117";

fn reply(status: u16, body: Value) -> Reply {
    Reply::json(status, serde_json::to_vec(&body).unwrap())
}

fn token(value: &str) -> Reply {
    // Deliberately no refresh_token: acquisition and renewal must use client credentials.
    reply(
        200,
        json!({"access_token": value, "token_type": "Bearer", "expires_in": 120}),
    )
}

fn order(status: &str) -> Value {
    json!({
        "id": ORDER, "status": status, "intent": "CAPTURE",
        "purchase_units": [{"reference_id": "cart-2026-117",
            "amount": {"currency_code": "USD", "value": "100.00"}}],
        "create_time": "2026-11-07T10:15:00Z",
        "links": [{"rel": "approve", "method": "GET",
            "href": "https://www.sandbox.paypal.com/checkoutnow?token=5O190127TN364715T"}]
    })
}

fn capture(id: &str, status: &str, amount: &str) -> Value {
    json!({
        "id": id, "status": status, "final_capture": true,
        "amount": {"currency_code": "USD", "value": amount},
        "invoice_id": "INV-2026-117", "custom_id": "cart-2026-117",
        "supplementary_data": {"related_ids": {"order_id": ORDER}},
        "create_time": "2026-11-07T10:17:00Z",
        "links": [{"rel": "self", "method": "GET",
            "href": format!("https://api-m.sandbox.paypal.com/v2/payments/captures/{id}")}]
    })
}

fn captured_order() -> Value {
    let mut result = order("COMPLETED");
    let mut pending = capture(PENDING_CAPTURE, "PENDING", "40.00");
    pending["status_details"] = json!({"reason": "PENDING_REVIEW"});
    result["purchase_units"] = json!([
        {"reference_id": "cart-2026-117", "payments": {"captures": [
            capture(CAPTURE, "COMPLETED", "60.00")]}},
        {"reference_id": "cart-2026-118", "payments": {"captures": [
            pending]}}
    ]);
    result
}

fn problem(name: &str, issue: &str) -> Value {
    json!({"name": name, "message": SECRET, "debug_id": "dbg-first-use-32",
        "details": [{"issue": issue, "description": TOKEN_A, "value": CLIENT}]})
}

async fn fixture(replies: Vec<Reply>) -> TestServer {
    let replies = Mutex::new(VecDeque::from(replies));
    TestServer::start(move |_request: &Recorded| {
        replies
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected extra wire request")
    })
    .await
}

fn policy() -> Policy {
    Policy::new(NonZeroUsize::new(2).unwrap())
}

fn fixture_client(server: &TestServer) -> PayPal {
    PayPal::builder(
        http_client().unwrap(),
        Credentials::new(CLIENT, SECRET).unwrap(),
    )
    .base_url(server.url())
    .reads(policy())
    .writes(policy())
    .build()
    .unwrap()
}

fn assert_redacted(value: impl std::fmt::Debug) {
    let text = format!("{value:?}");
    for sensitive in [CLIENT, SECRET, BASIC, TOKEN_A, TOKEN_B, TOKEN_C] {
        assert!(
            !text.contains(sensitive),
            "sensitive fixture value in Debug"
        );
    }
}

fn assert_error_redacted(error: &Error) {
    assert_redacted(error);
    let text = error.to_string();
    for sensitive in [CLIENT, SECRET, BASIC, TOKEN_A, TOKEN_B, TOKEN_C] {
        assert!(
            !text.contains(sensitive),
            "sensitive fixture value in Display"
        );
    }
}

fn assert_token_wire(request: &Recorded) {
    assert_eq!(request.method, "POST");
    assert_eq!(request.target, "/v1/oauth2/token");
    assert_eq!(request.header("authorization"), Some(BASIC));
    assert_eq!(request.header("accept"), Some("application/json"));
    assert_eq!(
        request.header("content-type"),
        Some("application/x-www-form-urlencoded")
    );
    assert_eq!(
        request.form_pairs(),
        vec![("grant_type".into(), "client_credentials".into())]
    );
    assert_eq!(request.header("paypal-request-id"), None);
}

fn assert_api_wire(request: &Recorded, method: &str, path: &str, token: &str) {
    assert_eq!(request.method, method);
    assert_eq!(request.target, path); // Exact target also proves absence of query parameters.
    assert_eq!(
        request.header("authorization"),
        Some(format!("Bearer {token}").as_str())
    );
    assert_eq!(request.header_all("authorization").len(), 1);
    assert_eq!(request.header("accept"), Some("application/json"));
    if method == "GET" {
        assert!(request.body.is_empty());
        assert_eq!(request.header("paypal-request-id"), None);
        assert_eq!(request.header("prefer"), None);
    } else {
        assert_eq!(request.header("content-type"), Some("application/json"));
        assert_eq!(request.header("prefer"), Some("return=representation"));
    }
}

#[tokio::test]
async fn client_credentials_create_order_and_obtain_current_state() {
    let server = fixture(vec![
        token(TOKEN_A),
        reply(201, order("CREATED")),
        reply(200, order("APPROVED")),
        reply(200, order("APPROVED")),
        token(TOKEN_B),
        reply(200, order("APPROVED")),
        reply(401, problem("AUTHENTICATION_FAILURE", "INVALID_TOKEN")),
        token(TOKEN_C),
        reply(200, order("APPROVED")),
        reply(429, problem("RATE_LIMIT_REACHED", "RATE_LIMIT_REACHED"))
            .header("Retry-After", "7")
            .header("PayPal-Debug-Id", "header-debug-32"),
        Reply::json(200, b"{malformed order"),
        Reply::json(404, b"{malformed error").header("PayPal-Debug-Id", "malformed-debug-32"),
        Reply::text(200, "text/html", SECRET),
    ])
    .await;
    let elapsed = Arc::new(AtomicU64::new(0));
    let clock_elapsed = elapsed.clone();
    let start = Instant::now();
    let credentials = Credentials::new(CLIENT, SECRET).unwrap();
    assert_redacted(&credentials);
    let builder = PayPal::builder(http_client().unwrap(), credentials)
        .base_url(server.url())
        .reads(policy())
        .writes(policy())
        .token_clock(move || start + Duration::from_secs(clock_elapsed.load(Ordering::SeqCst)));
    assert_redacted(&builder);
    let paypal = builder.build().unwrap();
    assert_redacted(&paypal);

    let unit = PurchaseUnitRequest::new(Money::new("USD", "100.00").unwrap())
        .unwrap()
        .reference_id("cart-2026-117")
        .unwrap()
        .invoice_id("INV-2026-117")
        .unwrap();
    let created = paypal
        .orders()
        .create(OrderRequest::capture(vec![unit]).unwrap())
        .set_request_id(RequestId::new(CREATE_ID).unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(created.id.as_str(), ORDER);
    assert_eq!(created.status, OrderStatus::Created);
    assert_eq!(
        created.approval_url(),
        Some("https://www.sandbox.paypal.com/checkoutnow?token=5O190127TN364715T")
    );
    assert_eq!(created.captures().count(), 0);

    let current = paypal.order(created.id.clone()).get().send().await.unwrap();
    assert_eq!(current.status, OrderStatus::Approved);
    assert_eq!(current.captures().count(), 0);
    elapsed.store(59, Ordering::SeqCst);
    paypal
        .clone()
        .order(created.id.clone())
        .get()
        .send()
        .await
        .unwrap();
    assert_eq!(
        server.requests().len(),
        4,
        "clone shares the still-valid token"
    );
    elapsed.store(60, Ordering::SeqCst);
    paypal.order(created.id.clone()).get().send().await.unwrap();
    assert_eq!(
        server.requests().len(),
        6,
        "renew exactly at lifetime minus margin"
    );
    let renewed = paypal.order(created.id.clone()).get().send().await.unwrap();
    assert_eq!(renewed.status, OrderStatus::Approved); // Read 401 obtains another token.

    let limited = paypal
        .order(created.id.clone())
        .get()
        .send()
        .await
        .unwrap_err();
    assert_error_redacted(&limited);
    let Error::Http(http) = limited else {
        panic!("expected 429 HTTP error")
    };
    assert_eq!(http.status().as_u16(), 429);
    assert!(http.is_rate_limited());
    assert_eq!(http.attempts(), 1);
    assert_eq!(http.operation(), "PayPal.order.get");
    assert_eq!(http.retry_after().unwrap(), Some(Duration::from_secs(7)));
    assert_eq!(http.meta().headers()["paypal-debug-id"], "header-debug-32");
    assert_eq!(
        http.document().unwrap().debug_id(),
        Some("dbg-first-use-32")
    );
    assert_eq!(http.document().unwrap().message_unredacted(), Some(SECRET));
    assert!(http.has_issue("RATE_LIMIT_REACHED"));

    let malformed = paypal
        .order(created.id.clone())
        .get()
        .send()
        .await
        .unwrap_err();
    let Error::Concord(decode) = malformed else {
        panic!("expected failed order decode")
    };
    assert_eq!(decode.kind(), ErrorKind::Decode);
    assert_eq!(decode.head().unwrap().status().as_u16(), 200);
    let missing = paypal
        .order(created.id.clone())
        .get()
        .send()
        .await
        .unwrap_err();
    assert_error_redacted(&missing);
    let Error::Http(http) = missing else {
        panic!("expected malformed 404 document")
    };
    assert_eq!(http.status().as_u16(), 404);
    assert_eq!(http.attempts(), 1);
    assert_eq!(http.document().unwrap_err().kind(), ErrorKind::Decode);
    assert_eq!(
        http.meta().headers()["paypal-debug-id"],
        "malformed-debug-32"
    );
    let wrong_media = paypal
        .order(created.id.clone())
        .get()
        .send()
        .await
        .unwrap_err();
    assert_error_redacted(&wrong_media);
    let Error::Concord(metadata) = wrong_media else {
        panic!("expected media refusal")
    };
    assert_eq!(metadata.kind(), ErrorKind::Metadata);
    assert_eq!(metadata.head().unwrap().status().as_u16(), 200);
    assert_eq!(
        created.status,
        OrderStatus::Created,
        "later failures preserve caller's earlier result"
    );

    let requests = server.requests();
    assert_eq!(
        requests.len(),
        13,
        "429, bad JSON and bad media were not retried"
    );
    for index in [0, 4, 7] {
        assert_token_wire(&requests[index]);
    }
    assert_api_wire(&requests[1], "POST", "/v2/checkout/orders", TOKEN_A);
    assert_eq!(requests[1].header("paypal-request-id"), Some(CREATE_ID));
    assert_eq!(
        serde_json::from_slice::<Value>(&requests[1].body).unwrap(),
        json!({
        "intent": "CAPTURE", "purchase_units": [{"reference_id": "cart-2026-117",
        "invoice_id": "INV-2026-117", "amount": {"currency_code": "USD", "value": "100.00"}}]})
    );
    for (index, token) in [
        (2, TOKEN_A),
        (3, TOKEN_A),
        (5, TOKEN_B),
        (6, TOKEN_B),
        (8, TOKEN_C),
        (9, TOKEN_C),
        (10, TOKEN_C),
        (11, TOKEN_C),
        (12, TOKEN_C),
    ] {
        assert_api_wire(
            &requests[index],
            "GET",
            &format!("/v2/checkout/orders/{ORDER}"),
            token,
        );
    }
    let before = requests.len();
    assert!(matches!(
        OrderId::new("not/an/order"),
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        Money::new("usd", "100.00"),
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        OrderRequest::capture(vec![]),
        Err(Error::Invalid(_))
    ));
    assert_eq!(server.requests().len(), before);
    assert_eq!(server.shutdown().await.open_after, 0);

    // Refused acquisition is returned, not cached; the next logical read tries again.
    let server = fixture(vec![
        reply(
            401,
            json!({"error": "invalid_client", "error_description": SECRET}),
        ),
        token(TOKEN_A),
        reply(200, order("APPROVED")),
    ])
    .await;
    let paypal = fixture_client(&server);
    let refused = paypal
        .order(OrderId::new(ORDER).unwrap())
        .get()
        .send()
        .await
        .unwrap_err();
    assert_error_redacted(&refused);
    assert!(
        matches!(refused, Error::Token(TokenError::Refused { status, document: Some(_) })
        if status.as_u16() == 401)
    );
    assert_eq!(
        server.requests().len(),
        1,
        "no API call and no refresh-token attempt"
    );
    paypal
        .order(OrderId::new(ORDER).unwrap())
        .get()
        .send()
        .await
        .unwrap();
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert_token_wire(&requests[0]);
    assert_token_wire(&requests[1]);
    assert_api_wire(
        &requests[2],
        "GET",
        &format!("/v2/checkout/orders/{ORDER}"),
        TOKEN_A,
    );
    assert_eq!(server.shutdown().await.open_after, 0);

    let server = fixture(vec![reply(
        200,
        json!({
        "access_token": TOKEN_A, "token_type": "Bearer", "expires_in": 60}),
    )])
    .await;
    let error = fixture_client(&server)
        .order(OrderId::new(ORDER).unwrap())
        .get()
        .send()
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Token(TokenError::Unusable(_))));
    assert_token_wire(&server.single());
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn capture_approved_order_and_retrieve_capture_with_partial_accounting() {
    let mut approved = order("APPROVED");
    approved["purchase_units"] = json!([
        {"reference_id": "cart-2026-117", "amount": {"currency_code": "USD", "value": "60.00"}},
        {"reference_id": "cart-2026-118", "amount": {"currency_code": "USD", "value": "40.00"}}
    ]);
    let server = fixture(vec![
        token(TOKEN_A),
        reply(200, approved),
        reply(
            500,
            problem("INTERNAL_SERVER_ERROR", "INTERNAL_SERVICE_ERROR"),
        ),
        reply(200, captured_order()), // Stored result of the same permitted capture request.
        reply(200, capture(CAPTURE, "COMPLETED", "60.00")),
        reply(404, problem("RESOURCE_NOT_FOUND", "INVALID_RESOURCE_ID"))
            .header("PayPal-Debug-Id", "accounting-debug-32"),
    ])
    .await;
    let paypal = fixture_client(&server);
    let current = paypal
        .order(OrderId::new(ORDER).unwrap())
        .get()
        .send()
        .await
        .unwrap();
    assert_eq!(current.status, OrderStatus::Approved);
    assert_eq!(current.purchase_units.len(), 2);
    let completed = paypal
        .order(current.id.clone())
        .capture(CaptureRequest::new())
        .set_request_id(RequestId::new(CAPTURE_ID).unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(completed.status, OrderStatus::Completed);
    let embedded: Vec<_> = completed.captures().collect();
    assert_eq!(embedded.len(), 2);
    assert_eq!(embedded[0].status, CaptureStatus::Completed);
    assert_eq!(embedded[1].status, CaptureStatus::Pending);
    assert_eq!(
        embedded[1]
            .status_details
            .as_ref()
            .unwrap()
            .reason
            .as_deref(),
        Some("PENDING_REVIEW")
    );
    assert!(
        !embedded
            .iter()
            .all(|c| c.status == CaptureStatus::Completed),
        "completed order alone does not prove every payment completed"
    );

    // Caller owns progress: retain successful details even if another detail read fails.
    let mut retrieved = Vec::new();
    let mut failed = Vec::new();
    for embedded in completed.captures() {
        match paypal.capture(embedded.id.clone()).get().send().await {
            Ok(detail) => retrieved.push(detail),
            Err(error) => failed.push((embedded.id.clone(), error)),
        }
    }
    assert_eq!(retrieved.len(), 1);
    let detail = &retrieved[0];
    assert_eq!(detail.id.as_str(), CAPTURE);
    assert_eq!(detail.status, CaptureStatus::Completed);
    assert_eq!(detail.amount.as_ref().unwrap().currency_code(), "USD");
    assert_eq!(detail.amount.as_ref().unwrap().value(), "60.00");
    assert_eq!(detail.final_capture, Some(true));
    assert_eq!(detail.invoice_id.as_deref(), Some("INV-2026-117"));
    assert_eq!(detail.order_id().unwrap(), &current.id);
    assert_eq!(detail.create_time.as_deref(), Some("2026-11-07T10:17:00Z"));
    assert_eq!(detail.links.len(), 1);
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].0.as_str(), PENDING_CAPTURE);
    assert_error_redacted(&failed[0].1);
    let Error::Http(http) = &failed[0].1 else {
        panic!("expected accounting 404")
    };
    assert_eq!(http.status().as_u16(), 404);
    assert_eq!(http.name(), Some("RESOURCE_NOT_FOUND"));
    assert!(http.has_issue("INVALID_RESOURCE_ID"));
    assert_eq!(http.operation(), "PayPal.capture.get");
    assert_eq!(http.attempts(), 1);
    assert_eq!(
        http.meta().headers()["paypal-debug-id"],
        "accounting-debug-32"
    );
    assert_eq!(
        completed.captures().count(),
        2,
        "captured order survives partial accounting failure"
    );

    let requests = server.requests();
    assert_eq!(
        requests.len(),
        6,
        "two capture attempts, two details, no link-following or pagination"
    );
    assert_token_wire(&requests[0]);
    assert_api_wire(
        &requests[1],
        "GET",
        &format!("/v2/checkout/orders/{ORDER}"),
        TOKEN_A,
    );
    for index in [2, 3] {
        assert_api_wire(
            &requests[index],
            "POST",
            &format!("/v2/checkout/orders/{ORDER}/capture"),
            TOKEN_A,
        );
        assert_eq!(
            requests[index].header("paypal-request-id"),
            Some(CAPTURE_ID)
        );
        assert_eq!(requests[index].body, b"{}");
    }
    assert_eq!(requests[2].body, requests[3].body);
    assert_ne!(requests[2].header("paypal-request-id"), Some(CREATE_ID));
    assert_api_wire(
        &requests[4],
        "GET",
        &format!("/v2/payments/captures/{CAPTURE}"),
        TOKEN_A,
    );
    assert_api_wire(
        &requests[5],
        "GET",
        &format!("/v2/payments/captures/{PENDING_CAPTURE}"),
        TOKEN_A,
    );
    assert_eq!(server.shutdown().await.open_after, 0);

    // A lost answer without a request ID must never silently repeat a capture.
    let server = fixture(vec![
        token(TOKEN_A),
        Reply::hang_up(),
        reply(200, captured_order()),
    ])
    .await;
    let paypal = fixture_client(&server);
    let unknown = paypal
        .order(OrderId::new(ORDER).unwrap())
        .capture(CaptureRequest::new())
        .send()
        .await
        .unwrap_err();
    assert_error_redacted(&unknown);
    let uncertainty = unknown
        .uncertain()
        .expect("capture may have reached PayPal");
    assert_eq!(uncertainty.operation(), "PayPal.order.capture");
    assert_eq!(uncertainty.attempts(), 1);
    assert_eq!(uncertainty.request_id(), None);
    assert!(
        matches!(uncertainty.cause(), UncertainCause::NoResponse(error)
        if error.kind() == ErrorKind::Transport)
    );
    assert_eq!(server.requests().len(), 2);
    let reconciled = paypal
        .order(OrderId::new(ORDER).unwrap())
        .get()
        .send()
        .await
        .unwrap();
    assert_eq!(reconciled.status, OrderStatus::Completed);
    assert_eq!(reconciled.captures().next().unwrap().id.as_str(), CAPTURE);
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert_token_wire(&requests[0]);
    assert_api_wire(
        &requests[1],
        "POST",
        &format!("/v2/checkout/orders/{ORDER}/capture"),
        TOKEN_A,
    );
    assert_eq!(requests[1].header("paypal-request-id"), None);
    assert_eq!(requests[1].body, b"{}");
    assert_api_wire(
        &requests[2],
        "GET",
        &format!("/v2/checkout/orders/{ORDER}"),
        TOKEN_A,
    );
    assert_eq!(server.shutdown().await.open_after, 0);

    // Both transport ambiguity and 5xx obey the same no-ID single-attempt boundary.
    let server = fixture(vec![
        token(TOKEN_A),
        reply(
            500,
            problem("INTERNAL_SERVER_ERROR", "INTERNAL_SERVICE_ERROR"),
        ),
    ])
    .await;
    let error = fixture_client(&server)
        .order(OrderId::new(ORDER).unwrap())
        .capture(CaptureRequest::new())
        .send()
        .await
        .unwrap_err();
    let uncertainty = error.uncertain().unwrap();
    assert_eq!(uncertainty.attempts(), 1);
    assert_eq!(uncertainty.request_id(), None);
    let UncertainCause::ServerError(http) = uncertainty.cause() else {
        panic!("expected uncertain 500")
    };
    assert_eq!(http.status().as_u16(), 500);
    assert_eq!(server.requests().len(), 2);
    assert_eq!(server.shutdown().await.open_after, 0);

    // A business refusal with an ID is still returned once, with provider diagnostics.
    let server = fixture(vec![
        token(TOKEN_A),
        reply(422, problem("UNPROCESSABLE_ENTITY", "ORDER_NOT_APPROVED"))
            .header("PayPal-Debug-Id", "approval-debug-32"),
    ])
    .await;
    let error = fixture_client(&server)
        .order(OrderId::new(ORDER).unwrap())
        .capture(CaptureRequest::new())
        .set_request_id(RequestId::new("capture-unapproved-117").unwrap())
        .send()
        .await
        .unwrap_err();
    assert_error_redacted(&error);
    assert_eq!(error.status().unwrap().as_u16(), 422);
    let Error::Http(http) = error else {
        panic!("expected approval refusal")
    };
    assert_eq!(http.attempts(), 1);
    assert!(http.has_issue("ORDER_NOT_APPROVED"));
    assert_eq!(
        http.meta().headers()["paypal-debug-id"],
        "approval-debug-32"
    );
    assert_eq!(server.requests().len(), 2);
    assert_eq!(server.shutdown().await.open_after, 0);

    // A successful status with an unreadable body is uncertain, never an invitation to replay.
    let server = fixture(vec![
        token(TOKEN_A),
        Reply::json(201, b"{broken capture").header("PayPal-Debug-Id", "decode-debug-32"),
    ])
    .await;
    let error = fixture_client(&server)
        .order(OrderId::new(ORDER).unwrap())
        .capture(CaptureRequest::new())
        .set_request_id(RequestId::new("capture-malformed-117").unwrap())
        .send()
        .await
        .unwrap_err();
    assert_error_redacted(&error);
    let uncertainty = error.uncertain().unwrap();
    assert_eq!(uncertainty.attempts(), 1);
    assert_eq!(
        uncertainty.request_id().unwrap().as_str(),
        "capture-malformed-117"
    );
    let UncertainCause::UnreadableSuccess(decode) = uncertainty.cause() else {
        panic!("expected unreadable success")
    };
    assert_eq!(decode.kind(), ErrorKind::Decode);
    assert_eq!(decode.head().unwrap().status().as_u16(), 201);
    assert_eq!(
        decode.head().unwrap().headers()["paypal-debug-id"],
        "decode-debug-32"
    );
    assert_eq!(server.requests().len(), 2);
    assert_eq!(server.shutdown().await.open_after, 0);

    // A refusal on a permitted replay cannot resolve the previous uncertain write.
    let server = fixture(vec![
        token(TOKEN_A),
        reply(
            500,
            problem("INTERNAL_SERVER_ERROR", "INTERNAL_SERVICE_ERROR"),
        ),
        reply(
            422,
            problem("UNPROCESSABLE_ENTITY", "ORDER_ALREADY_CAPTURED"),
        ),
    ])
    .await;
    let error = fixture_client(&server)
        .order(OrderId::new(ORDER).unwrap())
        .capture(CaptureRequest::new())
        .set_request_id(RequestId::new("capture-ambiguous-117").unwrap())
        .send()
        .await
        .unwrap_err();
    assert_error_redacted(&error);
    let uncertainty = error.uncertain().unwrap();
    assert_eq!(uncertainty.attempts(), 2);
    assert_eq!(
        uncertainty.request_id().unwrap().as_str(),
        "capture-ambiguous-117"
    );
    let UncertainCause::LaterRefusal(http) = uncertainty.cause() else {
        panic!("earlier capture is still uncertain")
    };
    assert_eq!(http.status().as_u16(), 422);
    assert!(http.has_issue("ORDER_ALREADY_CAPTURED"));
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    for request in &requests[1..] {
        assert_api_wire(
            request,
            "POST",
            &format!("/v2/checkout/orders/{ORDER}/capture"),
            TOKEN_A,
        );
        assert_eq!(
            request.header("paypal-request-id"),
            Some("capture-ambiguous-117")
        );
        assert_eq!(request.body, b"{}");
    }
    assert_eq!(server.shutdown().await.open_after, 0);
}

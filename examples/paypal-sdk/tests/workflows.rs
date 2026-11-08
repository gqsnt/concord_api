//! The two connected workflows end to end against the local fake, with the required failures:
//! a capture is never repeated unless it carries a `PayPal-Request-Id` (a timed-out capture
//! without one is surfaced as uncertain), and tokens come from client credentials only (no
//! refresh token): one acquisition for concurrent callers, a new acquisition after expiry.
//! Expectations come from the fixtures and the cited rules, not from the implementation.

mod support;

use std::time::Duration;

use concord_sdk_paypal::{
    CaptureId, CaptureRequest, CaptureStatus, Error, Money, OrderId, OrderRequest, OrderStatus,
    PurchaseUnitRequest, RequestId, UncertainCause,
};
use support::*;

fn order_request() -> OrderRequest {
    let unit = PurchaseUnitRequest::new(Money::new("USD", "100.00").unwrap())
        .unwrap()
        .reference_id("synthetic-unit-1")
        .unwrap()
        .invoice_id("SYNTH-INV-0001")
        .unwrap();
    OrderRequest::capture(vec![unit]).unwrap()
}

fn order_id() -> OrderId {
    OrderId::new(ORDER).unwrap()
}

/// Workflow 1: client credentials, create an order, obtain its current state.
#[tokio::test]
async fn create_an_order_with_client_credentials_then_read_its_state() {
    let fake = FakePayPal::new();
    let server = fake.start().await;
    let paypal = api(&server);

    let created = paypal
        .orders()
        .create(order_request())
        .set_request_id(RequestId::new("synthetic-create-0001").unwrap())
        .send()
        .await
        .unwrap();
    let expected = fixture_json("order-created.json");
    assert_eq!(created.id.as_str(), expected["id"]);
    assert_eq!(created.status, OrderStatus::Created);
    let approve = expected["links"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["rel"] == "approve")
        .unwrap();
    assert_eq!(created.approval_url(), approve["href"].as_str());

    let current = paypal.order(created.id.clone()).get().send().await.unwrap();
    assert_eq!(current.id, created.id);
    assert_eq!(current.status, OrderStatus::Created);
    let unit = &current.purchase_units[0];
    assert_eq!(unit.reference_id.as_deref(), Some("synthetic-unit-1"));
    assert_eq!(unit.amount.as_ref().unwrap().value(), "100.00");

    // One token for both calls, then the two API requests, both with that token.
    assert_eq!(
        targets(&server),
        [
            format!("POST {TOKEN_PATH}"),
            format!("POST {ORDERS_PATH}"),
            format!("GET {ORDER_PATH}"),
        ]
    );
    let bearer = bearer_of("token.json");
    for request in &server.requests()[1..] {
        assert_eq!(request.header("authorization"), Some(bearer.as_str()));
    }
}

/// Workflow 2: capture an approved order, then retrieve the resulting capture.
#[tokio::test]
async fn capture_an_approved_order_then_retrieve_the_capture() {
    let fake = FakePayPal::new();
    let server = fake.start().await;
    let paypal = api(&server);
    let created = paypal
        .orders()
        .create(order_request())
        .send()
        .await
        .unwrap();

    // The buyer approves at the approval URL; that UI is outside the SDK.
    fake.approve();
    let approved = paypal.order(created.id.clone()).get().send().await.unwrap();
    assert_eq!(approved.status, OrderStatus::Approved);

    let captured = paypal
        .order(approved.id.clone())
        .capture(CaptureRequest::new())
        .set_request_id(RequestId::new("synthetic-capture-0001").unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(captured.status, OrderStatus::Completed);
    let expected =
        &fixture_json("order-captured.json")["purchase_units"][0]["payments"]["captures"][0];
    let capture = captured.captures().next().unwrap();
    assert_eq!(capture.id.as_str(), expected["id"]);
    assert_eq!(capture.status, CaptureStatus::Completed);
    assert_eq!(capture.final_capture, Some(true));

    let shown = paypal
        .capture(capture.id.clone())
        .get()
        .send()
        .await
        .unwrap();
    let expected = fixture_json("capture.json");
    assert_eq!(shown.id.as_str(), expected["id"]);
    assert_eq!(shown.status, CaptureStatus::Completed);
    let amount = shown.amount.as_ref().unwrap();
    assert_eq!(amount.currency_code(), expected["amount"]["currency_code"]);
    assert_eq!(amount.value(), expected["amount"]["value"]);
    assert_eq!(shown.order_id(), Some(&created.id));
    assert_eq!(fake.captures_performed(), 1);
    assert_eq!(count(&server, "POST", TOKEN_PATH), 1);
}

/// Required failure: a capture without a `PayPal-Request-Id` that times out is sent once and
/// surfaced as uncertain, although the server did capture.
#[tokio::test(start_paused = true)]
async fn a_timed_out_capture_without_request_id_is_uncertain_and_never_repeated() {
    let fake = FakePayPal::new();
    let server = fake.start().await;
    fake.approve();
    fake.plan_captures(&[CapturePlan::AnswerAfter(Duration::from_secs(2))]);
    let timeout = Duration::from_millis(300);
    let paypal = builder_with(&server, timeout)
        .writes(fast(5)) // even with room for more attempts
        .build()
        .unwrap();

    let capture = paypal
        .order(order_id())
        .capture(CaptureRequest::new())
        .send();
    let error = time_out(&server, CAPTURE_ORDER_PATH, timeout, 1, capture)
        .await
        .unwrap_err();
    let Error::Uncertain(uncertain) = &error else {
        panic!("expected an uncertain capture, got {error:?}")
    };
    assert_eq!(uncertain.operation(), "PayPal.order.capture");
    assert!(uncertain.request_id().is_none());
    assert_eq!(uncertain.attempts(), 1);
    let UncertainCause::NoResponse(cause) = uncertain.cause() else {
        panic!("expected no response, got {:?}", uncertain.cause())
    };
    assert!(cause.transport().is_some_and(|e| e.is_timeout()));
    assert!(error.to_string().contains("no PayPal-Request-Id"));

    // Exactly one capture reached the server, and it was performed: the outcome is learned by
    // reading the order, not by capturing again.
    assert_eq!(count(&server, "POST", CAPTURE_ORDER_PATH), 1);
    assert_eq!(fake.captures_performed(), 1);
    let order = drive(paypal.order(order_id()).get().send()).await.unwrap();
    assert_eq!(order.status, OrderStatus::Completed);
}

/// A `5xx` capture without a request ID is uncertain too, and also sent once.
#[tokio::test]
async fn a_failed_capture_without_request_id_is_uncertain_and_sent_once() {
    let fake = FakePayPal::new();
    let server = fake.start().await;
    fake.approve();
    fake.plan_captures(&[CapturePlan::ServerError]);
    let paypal = builder(&server).writes(fast(5)).build().unwrap();

    let error = paypal
        .order(order_id())
        .capture(CaptureRequest::new())
        .send()
        .await
        .unwrap_err();
    let uncertain = error.uncertain().expect("uncertain");
    let UncertainCause::ServerError(http) = uncertain.cause() else {
        panic!("expected a server error, got {:?}", uncertain.cause())
    };
    assert_eq!(http.status().as_u16(), 500);
    assert_eq!(http.name(), Some("INTERNAL_SERVER_ERROR"));
    assert_eq!(count(&server, "POST", CAPTURE_ORDER_PATH), 1);
}

/// With a request ID the operation permits repetition: a timed-out capture is sent again with
/// the same ID and body, and PayPal's stored result (`200`) completes the call. One capture.
#[tokio::test(start_paused = true)]
async fn a_timed_out_capture_with_request_id_is_repeated_with_the_same_id() {
    let fake = FakePayPal::new();
    let server = fake.start().await;
    fake.approve();
    fake.plan_captures(&[CapturePlan::AnswerAfter(Duration::from_secs(2))]);
    let timeout = Duration::from_millis(300);
    let paypal = builder_with(&server, timeout).build().unwrap();

    let capture = paypal
        .order(order_id())
        .capture(CaptureRequest::new())
        .set_request_id(RequestId::new("synthetic-capture-0002").unwrap())
        .send();
    let order = time_out(&server, CAPTURE_ORDER_PATH, timeout, 1, capture)
        .await
        .unwrap();
    assert_eq!(order.status, OrderStatus::Completed);
    assert_eq!(order.captures().next().unwrap().id.as_str(), CAPTURE);

    let captures: Vec<_> = server
        .requests()
        .into_iter()
        .filter(|r| r.path() == CAPTURE_ORDER_PATH)
        .collect();
    assert_eq!(captures.len(), 2);
    for request in &captures {
        assert_eq!(
            request.header("paypal-request-id"),
            Some("synthetic-capture-0002")
        );
        assert_eq!(request.body, captures[0].body);
    }
    assert_eq!(fake.captures_performed(), 1);
}

/// A capture with a request ID whose every attempt fails is uncertain and returns the ID to
/// resend deliberately.
#[tokio::test]
async fn a_capture_failing_every_attempt_is_uncertain_with_its_request_id() {
    let fake = FakePayPal::new();
    let server = fake.start().await;
    fake.approve();
    fake.plan_captures(&[CapturePlan::ServerError, CapturePlan::ServerError]);
    let paypal = api(&server);
    let id = RequestId::new("synthetic-capture-0003").unwrap();

    let error = paypal
        .order(order_id())
        .capture(CaptureRequest::new())
        .set_request_id(id.clone())
        .send()
        .await
        .unwrap_err();
    let uncertain = error.uncertain().expect("uncertain");
    assert_eq!(uncertain.request_id(), Some(&id));
    assert_eq!(uncertain.attempts(), 2);
    assert!(matches!(uncertain.cause(), UncertainCause::ServerError(_)));
    assert!(error.to_string().contains("same PayPal-Request-Id"));
    assert_eq!(count(&server, "POST", CAPTURE_ORDER_PATH), 2);
    assert_eq!(fake.captures_performed(), 0);

    // Resending with the returned ID captures once.
    let order = paypal
        .order(order_id())
        .capture(CaptureRequest::new())
        .set_request_id(uncertain.request_id().unwrap().clone())
        .send()
        .await
        .unwrap();
    assert_eq!(order.status, OrderStatus::Completed);
    assert_eq!(fake.captures_performed(), 1);
}

/// A capture before approval is a definite refusal (`422 ORDER_NOT_APPROVED`), not uncertain.
#[tokio::test]
async fn a_capture_before_approval_is_refused() {
    let fake = FakePayPal::new();
    let server = fake.start().await;
    let paypal = api(&server);
    let error = paypal
        .order(order_id())
        .capture(CaptureRequest::new())
        .set_request_id(RequestId::new("synthetic-capture-0004").unwrap())
        .send()
        .await
        .unwrap_err();
    let Error::Http(http) = &error else {
        panic!("expected a refusal, got {error:?}")
    };
    assert_eq!(http.status().as_u16(), 422);
    assert!(http.has_issue("ORDER_NOT_APPROVED"));
    assert_eq!(count(&server, "POST", CAPTURE_ORDER_PATH), 1);
}

/// Concurrent first calls share one client-credentials acquisition.
#[tokio::test]
async fn concurrent_calls_fetch_one_token() {
    let fake = FakePayPal::new();
    fake.delay_tokens(Duration::from_millis(200));
    let server = fake.start().await;
    let paypal = api(&server);
    let mut calls = tokio::task::JoinSet::new();
    for _ in 0..8 {
        let paypal = paypal.clone();
        calls.spawn(async move { paypal.order(order_id()).get().send().await });
    }
    while let Some(result) = calls.join_next().await {
        assert_eq!(result.unwrap().unwrap().status, OrderStatus::Created);
    }
    assert_eq!(count(&server, "POST", TOKEN_PATH), 1);
    assert_eq!(count(&server, "GET", ORDER_PATH), 8);
}

/// Required failure: there is no refresh token. When `expires_in` (less the margin) has run out,
/// the next call obtains a new token with client credentials again; before that, none.
#[tokio::test]
async fn an_expired_token_is_obtained_again_with_client_credentials() {
    let fake = FakePayPal::new();
    fake.next_tokens(&["token.json", "token-renewed.json"]);
    let server = fake.start().await;
    let clock = TestClock::new();
    let paypal = builder(&server)
        .token_refresh_margin(Duration::from_secs(60))
        .token_clock(clock.reader())
        .build()
        .unwrap();
    let lifetime = Duration::from_secs(fixture_json("token.json")["expires_in"].as_u64().unwrap());

    paypal.order(order_id()).get().send().await.unwrap();
    clock.advance(lifetime - Duration::from_secs(61));
    paypal.order(order_id()).get().send().await.unwrap();
    assert_eq!(count(&server, "POST", TOKEN_PATH), 1);
    clock.advance(Duration::from_secs(2)); // now inside the refresh margin
    paypal.order(order_id()).get().send().await.unwrap();

    let requests = server.requests();
    let tokens: Vec<_> = requests.iter().filter(|r| r.path() == TOKEN_PATH).collect();
    assert_eq!(tokens.len(), 2);
    for token in &tokens {
        assert_eq!(token.header("authorization"), Some(BASIC));
        assert_eq!(
            token.form_pairs(),
            [("grant_type".to_owned(), "client_credentials".to_owned())]
        );
    }
    let bearers: Vec<_> = requests
        .iter()
        .filter(|r| r.path() == ORDER_PATH)
        .map(|r| r.header("authorization").unwrap().to_owned())
        .collect();
    assert_eq!(
        bearers,
        [
            bearer_of("token.json"),
            bearer_of("token.json"),
            bearer_of("token-renewed.json")
        ]
    );
}

/// A `401` on a capture without a request ID is not replayed with a new token (the capture is
/// sent once); the token it used is discarded, so the next call obtains another.
#[tokio::test]
async fn an_unauthorized_capture_is_not_replayed_but_its_token_is_discarded() {
    let fake = FakePayPal::new();
    let server = concord_example_testserver::TestServer::start({
        let fake = fake.clone();
        let refused = std::sync::atomic::AtomicBool::new(false);
        move |r: &concord_example_testserver::Recorded| {
            if r.path() == CAPTURE_ORDER_PATH
                && !refused.swap(true, std::sync::atomic::Ordering::SeqCst)
            {
                return json_fixture(401, "error-authentication-failure.json");
            }
            fake.serve(r)
        }
    })
    .await;
    fake.next_tokens(&["token.json", "token-renewed.json"]);
    fake.approve();
    let paypal = api(&server);

    let error = paypal
        .order(order_id())
        .capture(CaptureRequest::new())
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.status().map(|s| s.as_u16()), Some(401));
    assert_eq!(count(&server, "POST", CAPTURE_ORDER_PATH), 1);
    assert_eq!(count(&server, "POST", TOKEN_PATH), 1);

    let order = paypal.order(order_id()).get().send().await.unwrap();
    assert_eq!(order.status, OrderStatus::Approved);
    assert_eq!(count(&server, "POST", TOKEN_PATH), 2);
    let last = server.requests().pop().unwrap();
    assert_eq!(
        last.header("authorization"),
        Some(bearer_of("token-renewed.json").as_str())
    );
}

/// A read answered `401` is sent once more with a new token.
#[tokio::test]
async fn an_unauthorized_read_is_repeated_once_with_a_new_token() {
    let fake = FakePayPal::new();
    let server = concord_example_testserver::TestServer::start({
        let fake = fake.clone();
        let refused = std::sync::atomic::AtomicBool::new(false);
        move |r: &concord_example_testserver::Recorded| {
            if r.path() == CAPTURE_PATH && !refused.swap(true, std::sync::atomic::Ordering::SeqCst)
            {
                return json_fixture(401, "error-authentication-failure.json");
            }
            fake.serve(r)
        }
    })
    .await;
    fake.next_tokens(&["token.json", "token-renewed.json"]);
    fake.approve();
    let paypal = api(&server);
    paypal
        .order(order_id())
        .capture(CaptureRequest::new())
        .send()
        .await
        .unwrap();

    let capture = paypal
        .capture(CaptureId::new(CAPTURE).unwrap())
        .get()
        .send()
        .await
        .unwrap();
    assert_eq!(capture.status, CaptureStatus::Completed);
    assert_eq!(count(&server, "GET", CAPTURE_PATH), 2);
    assert_eq!(count(&server, "POST", TOKEN_PATH), 2);
}

//! Manual capture (maintenance ticket): wire contract, receipt confirmation, replay rules and
//! one consumer workflow. Loopback fixtures only; the real Adyen API is never called.
mod common;
use common::*;
use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_adyen_checkout::{concord::ErrorKind, *};
use serde_json::{Value, json};
use std::{
    num::NonZeroUsize,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

const RECEIVED: &str = include_str!("../fixtures/capture-received.json");
/// `pspReference` of `fixtures/authorised.json`.
const PAYMENT_PSP: &str = "8812345678901234";
/// `pspReference` of `fixtures/capture-received.json`.
const CAPTURE_PSP: &str = "9913140798220028";
const CAPTURE_PATH: &str = "/v71/payments/8812345678901234/captures";

fn capture() -> CaptureRequest {
    CaptureRequest {
        merchant_account: "TestMerchant".into(),
        amount: Amount {
            currency: "EUR".into(),
            value: 600,
        },
        reference: Some("order-sdk31-capture-1".into()),
    }
}

/// Adyen's documented success: `201 Created`, echoing the idempotency key.
fn received(request: &Recorded, body: &str) -> Reply {
    Reply::json(201, body).header(
        "Idempotency-Key",
        request.header("idempotency-key").unwrap(),
    )
}

fn mutated(field: &str, value: Option<Value>) -> String {
    let mut document = fixture_json(RECEIVED);
    match value {
        Some(value) => document[field] = value,
        None => {
            document.as_object_mut().unwrap().remove(field);
        }
    }
    document.to_string()
}

fn retry_policy(attempts: usize, timeout: Duration) -> SendPolicy {
    SendPolicy::default()
        .with_retry(concord_retry::Policy {
            timeout: Some(timeout),
            initial_delay: Duration::ZERO,
            max_delay: Duration::ZERO,
            ..concord_retry::Policy::new(NonZeroUsize::new(attempts).unwrap())
        })
        .unwrap()
}

// ---------------------------------------------------------------------------------------------
// Regression: the capture contract.
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn capture_wire_contract_returns_a_received_result_not_a_captured_one() {
    let server = TestServer::start(|r: &Recorded| {
        assert_eq!(r.method, "POST");
        assert_eq!(r.target, CAPTURE_PATH);
        assert_eq!(r.header("x-api-key"), Some("synthetic-api-secret"));
        assert_eq!(r.header("idempotency-key"), Some("synthetic-key-secret"));
        assert_eq!(r.header("content-type"), Some("application/json"));
        assert_eq!(r.header("accept"), Some("application/json"));
        assert!(!r.chunked);
        assert_body(r, include_str!("../fixtures/capture-request.json"));
        received(r, RECEIVED)
    })
    .await;
    let api = api(&server, SendPolicy::default());
    let scope = api.v71();
    let payment = scope.payment(PAYMENT_PSP);
    let operation = payment.captures("synthetic-key-secret", capture());
    let prepared = operation.clone().prepare().unwrap();
    assert_eq!(prepared.request().url().path(), CAPTURE_PATH);
    assert!(prepared.request().headers()["idempotency-key"].is_sensitive());
    let shown = format!("{api:?} {scope:?} {payment:?} {operation:?} {prepared:?}");
    assert_hidden(&shown);
    assert!(
        !shown.contains(PAYMENT_PSP),
        "payment reference leaked: {shown}"
    );

    let response = operation.send().await.unwrap();
    assert_eq!(response.meta.status().as_u16(), 201);
    assert_eq!(response.retry.attempts, 1);
    let receipt = &response.value;
    assert_eq!(receipt.psp_reference, CAPTURE_PSP);
    assert_eq!(receipt.payment_psp_reference, PAYMENT_PSP);
    assert_eq!(receipt.requested, capture().amount);
    assert_eq!(receipt.merchant_account, "TestMerchant");
    assert_eq!(receipt.reference.as_deref(), Some("order-sdk31-capture-1"));
    // `received` is the type itself; no status string or outcome is carried as data.
    assert!(receipt.fields.is_empty());
    let shown = format!("{response:?} {receipt:?} {:?}", capture());
    for value in [
        CAPTURE_PSP,
        PAYMENT_PSP,
        "order-sdk31-capture-1",
        "TestMerchant",
    ] {
        assert!(!shown.contains(value), "{value} leaked: {shown}");
    }
    assert_eq!(server.requests().len(), 1);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn capture_documents_for_another_request_require_reconciliation_and_revoke_the_key() {
    let cases = [
        ("paymentPspReference", Some(json!("8812345678909999"))),
        // The full authorisation instead of the partial amount that was requested.
        ("amount", Some(json!({"currency": "EUR", "value": 1000}))),
        ("amount", Some(json!({"currency": "USD", "value": 600}))),
        ("merchantAccount", Some(json!("OtherMerchant"))),
        ("reference", Some(json!("order-sdk31-capture-2"))),
        ("reference", None),
    ];
    for (field, value) in cases {
        let body = mutated(field, value);
        let server = TestServer::start(move |r: &Recorded| received(r, &body)).await;
        let operation = api(&server, SendPolicy::default())
            .v71()
            .payment(PAYMENT_PSP)
            .captures("key", capture());
        let error = operation.clone().send().await.unwrap_err();
        let evidence = error.reconciliation().unwrap();
        assert_eq!(
            evidence.reason,
            ReconciliationReason::GuaranteeNotConfirmed,
            "{field}"
        );
        assert_eq!(evidence.meta.as_ref().unwrap().status().as_u16(), 201);
        assert_eq!(evidence.retry.as_ref().unwrap().attempts, 1);
        assert_hidden(&format!("{error:?} {error}"));
        let again = operation.send().await.unwrap_err();
        assert_eq!(
            again.reconciliation().unwrap().reason,
            ReconciliationReason::GuaranteeRevoked,
            "{field}"
        );
        assert_eq!(server.requests().len(), 1, "{field}");
        server.shutdown().await;
    }

    // A capture sent without a reference must not accept a document that carries one.
    let server = TestServer::start(|r: &Recorded| received(r, RECEIVED)).await;
    let mut unreferenced = capture();
    unreferenced.reference = None;
    let error = api(&server, SendPolicy::default())
        .v71()
        .payment(PAYMENT_PSP)
        .captures("key", unreferenced)
        .send()
        .await
        .unwrap_err();
    assert_eq!(
        error.reconciliation().unwrap().reason,
        ReconciliationReason::GuaranteeNotConfirmed
    );
    server.shutdown().await;
}

#[tokio::test]
async fn capture_documents_without_received_status_or_references_are_not_receipts() {
    let cases = [
        ("status", Some(json!("captured"))),
        ("status", Some(json!("Received"))),
        ("status", None),
        ("pspReference", None),
        ("paymentPspReference", None),
        ("amount", None),
        ("merchantAccount", None),
    ];
    for (field, value) in cases {
        let body = mutated(field, value);
        let server = TestServer::start(move |r: &Recorded| received(r, &body)).await;
        let error = api(&server, SendPolicy::default())
            .v71()
            .payment(PAYMENT_PSP)
            .captures("key", capture())
            .send()
            .await
            .unwrap_err();
        let evidence = error.reconciliation().unwrap();
        assert_eq!(
            evidence.reason,
            ReconciliationReason::UncertainDelivery,
            "{field}"
        );
        assert_eq!(evidence.cause.as_ref().unwrap().kind(), ErrorKind::Decode);
        assert_hidden(&format!("{error:?} {error}"));
        assert_eq!(server.requests().len(), 1, "{field}");
        server.shutdown().await;
    }
}

#[tokio::test]
async fn capture_reference_stays_one_path_segment_and_unencodable_references_fail_closed() {
    let server = TestServer::start(|r: &Recorded| {
        assert_eq!(r.target, "/v71/payments/a%2Fb/captures");
        received(r, &mutated("paymentPspReference", Some(json!("a/b"))))
    })
    .await;
    let scope = api(&server, SendPolicy::default()).v71();
    for bad in ["", ".", ".."] {
        let error = scope
            .payment(bad)
            .captures("key", capture())
            .send()
            .await
            .unwrap_err();
        assert!(
            matches!(error, Error::Concord(ref e) if e.kind() == ErrorKind::Build),
            "{bad:?}: {error}"
        );
    }
    assert!(server.requests().is_empty());
    // Encoded as one segment; the provider's raw echo cannot match it, so nothing is confirmed.
    let error = scope
        .payment("a/b")
        .captures("key", capture())
        .send()
        .await
        .unwrap_err();
    assert_eq!(
        error.reconciliation().unwrap().reason,
        ReconciliationReason::GuaranteeNotConfirmed
    );
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

#[tokio::test]
async fn capture_replay_follows_the_existing_guarantee_rules() {
    // No head arrived: the same key and bytes are replayed automatically, as for a payment.
    let calls = AtomicUsize::new(0);
    let server = TestServer::start(move |r: &Recorded| {
        if calls.fetch_add(1, Ordering::SeqCst) == 0 {
            Reply::hang_up()
        } else {
            received(r, RECEIVED)
        }
    })
    .await;
    let response = api(&server, retry_policy(2, Duration::from_secs(2)))
        .v71()
        .payment(PAYMENT_PSP)
        .captures("key", capture())
        .send()
        .await
        .unwrap();
    assert_eq!(response.retry.attempts, 2);
    let wire = server.requests();
    assert_eq!(wire.len(), 2);
    assert_eq!(wire[0].target, wire[1].target);
    assert_eq!(wire[0].body, wire[1].body);
    assert_eq!(wire[0].headers, wire[1].headers);
    server.shutdown().await;

    // Every attempt dropped: uncertain, with the key binding kept for an identical replay.
    let server = TestServer::start(|_: &Recorded| Reply::hang_up()).await;
    let error = api(&server, retry_policy(2, Duration::from_secs(2)))
        .v71()
        .payment(PAYMENT_PSP)
        .captures("key", capture())
        .send()
        .await
        .unwrap_err();
    let evidence = error.reconciliation().unwrap();
    assert_eq!(evidence.reason, ReconciliationReason::UncertainDelivery);
    assert_eq!(
        evidence.cause.as_ref().unwrap().kind(),
        ErrorKind::Transport
    );
    let shown = format!("{error:?} {error}");
    assert_hidden(&shown);
    assert!(
        !shown.contains(PAYMENT_PSP),
        "payment reference leaked: {shown}"
    );
    server.shutdown().await;

    // A provider error stays a provider failure and is not retried without permission.
    let server = TestServer::start(|r: &Recorded| {
        Reply::json(422, PROBLEM).header("idempotency-key", r.header("idempotency-key").unwrap())
    })
    .await;
    let error = api(&server, SendPolicy::default())
        .v71()
        .payment(PAYMENT_PSP)
        .captures("key", capture())
        .send()
        .await
        .unwrap_err();
    let Error::Provider(failure) = error else {
        panic!("expected provider failure")
    };
    assert_eq!(failure.class, HttpClass::Validation);
    assert_eq!(failure.retry.attempts, 1);
    server.shutdown().await;

    // An expired guarantee refuses the replay before the wire.
    let server = TestServer::start(|r: &Recorded| received(r, RECEIVED)).await;
    let policy = SendPolicy::default()
        .with_guarantee_window(Duration::from_secs(1))
        .unwrap();
    let operation = api(&server, policy)
        .v71()
        .payment(PAYMENT_PSP)
        .captures("key", capture());
    operation.clone().send().await.unwrap();
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(1)).await;
    assert_eq!(
        operation
            .send()
            .await
            .unwrap_err()
            .reconciliation()
            .unwrap()
            .reason,
        ReconciliationReason::GuaranteeExpired
    );
    tokio::time::resume();
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Consumer: an application authorises a payment, then captures part of it later.
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn consumer_partial_capture_after_timeout_is_retried_identically_and_never_changed() {
    let captures = AtomicUsize::new(0);
    let server = TestServer::start(move |r: &Recorded| match r.path() {
        "/v71/payments" => success(r, AUTHORISED),
        CAPTURE_PATH if captures.fetch_add(1, Ordering::SeqCst) == 0 => {
            // The head is late: the application's deadline passes first.
            received(r, RECEIVED).delay(Duration::from_secs(2))
        }
        CAPTURE_PATH => received(r, RECEIVED),
        other => panic!("unexpected route {other}"),
    })
    .await;
    let checkout = api(&server, retry_policy(3, Duration::from_millis(300))).v71();

    // Manual capture is configured for the merchant account; the payment request is unchanged.
    let authorised = checkout
        .payments("payment-key", payment())
        .send()
        .await
        .unwrap();
    assert_eq!(authorised.value.state(), PaymentState::Authorised);
    let psp = authorised.value.psp_reference.clone().unwrap();

    // Capture 6.00 of the 10.00 authorised.
    let request = CaptureRequest {
        merchant_account: "TestMerchant".into(),
        amount: Amount {
            currency: "EUR".into(),
            value: 600,
        },
        reference: Some("order-sdk31-capture-1".into()),
    };
    let capture_op = checkout
        .payment(psp.clone())
        .captures("capture-key", request.clone());
    let timed_out = capture_op.clone().send().await.unwrap_err();
    let evidence = timed_out.reconciliation().unwrap();
    assert_eq!(evidence.reason, ReconciliationReason::Deadline);
    assert_eq!(
        evidence.retry.as_ref().unwrap().stop,
        concord_retry::StopReason::Deadline
    );
    assert_eq!(
        server.requests().len(),
        2,
        "a timed-out capture is not replayed automatically"
    );

    // The application decides to retry: the same operation, key and bytes.
    let receipt = capture_op.send().await.unwrap();
    assert_eq!(receipt.meta.status().as_u16(), 201);
    assert_eq!(receipt.value.psp_reference, CAPTURE_PSP);
    assert_eq!(receipt.value.payment_psp_reference, psp);
    assert_eq!(receipt.value.requested, request.amount);
    let wire = server.requests();
    assert_eq!(wire.len(), 3);
    assert_eq!(wire[1].target, wire[2].target);
    assert_eq!(wire[1].body, wire[2].body);
    assert_eq!(wire[1].headers, wire[2].headers);
    assert_eq!(wire[1].header("idempotency-key"), Some("capture-key"));

    // Capturing a different amount under the same key is refused before sending.
    let mut full = request.clone();
    full.amount.value = 1000;
    let changed = checkout
        .payment(psp.clone())
        .captures("capture-key", full)
        .send()
        .await
        .unwrap_err();
    assert_eq!(
        changed.reconciliation().unwrap().reason,
        ReconciliationReason::ChangedRequest
    );
    // So is reusing the payment's own key for its capture.
    let reused = checkout
        .payment(psp)
        .captures("payment-key", request)
        .send()
        .await
        .unwrap_err();
    assert_eq!(
        reused.reconciliation().unwrap().reason,
        ReconciliationReason::ChangedRequest
    );
    assert_eq!(server.requests().len(), 3);
    server.shutdown().await;
}

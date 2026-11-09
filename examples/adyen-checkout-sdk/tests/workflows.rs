//! Connected local-only payment workflows and idempotency failure exercises.
mod common;
use common::*;
use concord_example_testserver::{Gate, Recorded, Reply, TestServer, paused};
use concord_sdk_adyen_checkout::*;
use std::{
    num::NonZeroUsize,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

fn retry_policy(attempts: usize, delay: Duration, timeout: Duration) -> SendPolicy {
    SendPolicy::default()
        .with_retry(concord_retry::Policy {
            timeout: Some(timeout),
            initial_delay: delay,
            max_delay: delay,
            ..concord_retry::Policy::new(NonZeroUsize::new(attempts).unwrap())
        })
        .unwrap()
}

#[tokio::test]
async fn list_select_submit_follow_action_and_submit_details_for_existing_flow() {
    let server = TestServer::start(|r: &Recorded| match r.path() {
        "/v71/paymentMethods" => success(r, METHODS),
        "/v71/payments" => success(r, ACTION),
        "/v71/payments/details" => {
            assert_body(r, include_str!("../fixtures/details-request.json"));
            success(r, AUTHORISED)
        }
        _ => panic!("no GET status lookup, redirect visit or extra endpoint exists in this slice"),
    })
    .await;
    let scope = api(&server, SendPolicy::default()).v71();
    let available = scope
        .payment_methods("methods-key", methods())
        .send()
        .await
        .unwrap();
    let method = available
        .value
        .payment_methods
        .iter()
        .find(|m| m.kind == "ideal")
        .unwrap();
    let issuer = method.attributes["issuers"][0]["id"].as_str().unwrap();
    let mut request = payment();
    request.payment_method = SandboxMethod::Ideal {
        issuer: issuer.into(),
    };
    let payment = scope.payments("payment-key", request).send().await.unwrap();
    assert_eq!(payment.value.state(), PaymentState::ActionRequired);
    assert!(payment.value.psp_reference.is_none());
    let action = payment.value.action.unwrap();
    assert_eq!(action.kind, "redirect");
    assert_eq!(action.fields["method"], "GET");
    assert!(
        action.fields["url"]
            .as_str()
            .unwrap()
            .starts_with("https://redirect.example/")
    );
    // Application hands the action to its UI and obtains a redirectResult. The fixture models that return.
    let mut continuation = DetailsRequest::redirect("synthetic-redirect-secret");
    continuation.payment_data = action.payment_data;
    let complete = scope
        .details("details-key", continuation)
        .send()
        .await
        .unwrap();
    assert_eq!(complete.value.state(), PaymentState::Authorised);
    assert_eq!(
        complete.value.psp_reference.as_deref(),
        Some("8812345678901234")
    );
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(
        requests
            .iter()
            .map(|r| r.header("idempotency-key").unwrap())
            .collect::<Vec<_>>(),
        ["methods-key", "payment-key", "details-key"]
    );
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn details_can_return_another_action_then_pending_without_inventing_authorisation() {
    let server = TestServer::start(|r: &Recorded| match r.header("idempotency-key").unwrap() {
        "start" => success(r, ACTION),
        "redirect-details" => success(r, NEXT_ACTION),
        "challenge-details" => {
            let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
            assert_eq!(
                body["details"]["threeds2.challengeResult"],
                "synthetic-component-result"
            );
            assert_eq!(body["paymentData"], "synthetic-next-flow-secret");
            success(r, include_str!("../fixtures/pending.json"))
        }
        _ => panic!("unexpected request"),
    })
    .await;
    let scope = api(&server, SendPolicy::default()).v71();
    let first = scope.payments("start", payment()).send().await.unwrap();
    let mut redirect = DetailsRequest::redirect("synthetic-redirect-secret");
    redirect.payment_data = first.value.action.unwrap().payment_data;
    let next = scope
        .details("redirect-details", redirect)
        .send()
        .await
        .unwrap();
    assert_eq!(next.value.state(), PaymentState::ActionRequired);
    let action = next.value.action.unwrap();
    assert_eq!(action.kind, "threeDS2");
    assert_eq!(action.fields["token"], "synthetic-challenge-secret");
    // External component supplies the result. The SDK doesn't orchestrate 3DS or verify webhooks.
    let continuation = DetailsRequest {
        details: [(
            "threeds2.challengeResult".into(),
            "synthetic-component-result".into(),
        )]
        .into(),
        payment_data: action.payment_data,
    };
    let pending = scope
        .details("challenge-details", continuation)
        .send()
        .await
        .unwrap();
    assert_eq!(pending.value.state(), PaymentState::Pending);
    assert_eq!(server.requests().len(), 3);
    server.shutdown().await;
}

#[tokio::test]
async fn automatic_and_caller_retries_reuse_identical_key_and_bytes_changed_body_reconciles() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let server = TestServer::start(move |r: &Recorded| {
        if r.path() == "/v71/paymentMethods" {
            return success(r, METHODS);
        }
        if counter.fetch_add(1, Ordering::SeqCst) == 0 {
            Reply::json(503, UNAVAILABLE)
                .header("transient-error", "true")
                .header("retry-after", "0")
        } else {
            success(r, AUTHORISED)
        }
    })
    .await;
    let scope = api(
        &server,
        retry_policy(3, Duration::from_millis(1), Duration::from_secs(2)),
    )
    .v71();
    scope
        .payment_methods("list", methods())
        .send()
        .await
        .unwrap();
    let operation = scope.payments("same-payment-key", payment());
    let done = operation.clone().send().await.unwrap();
    assert_eq!(done.value.state(), PaymentState::Authorised);
    assert_eq!(done.retry.attempts, 2);
    assert_eq!(done.retry.waits, [Duration::from_millis(1)]);
    operation.send().await.unwrap();
    let mut changed = payment();
    changed.amount.value += 1;
    let error = scope
        .payments("same-payment-key", changed)
        .send()
        .await
        .unwrap_err();
    assert_eq!(
        error.reconciliation().unwrap().reason,
        ReconciliationReason::ChangedRequest
    );
    let requests = server.requests();
    assert_eq!(requests.len(), 4); // listing, transient, automatic retry, caller retry
    let attempts = &requests[1..];
    for request in attempts {
        assert_eq!(request.header("idempotency-key"), Some("same-payment-key"));
        assert_eq!(request.body, attempts[0].body);
        assert_eq!(request.target, attempts[0].target);
        assert_eq!(request.headers, attempts[0].headers);
    }
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    server.shutdown().await;
}

#[tokio::test]
async fn transport_ambiguity_replays_the_original_request_only_and_retains_failure_evidence() {
    let calls = AtomicUsize::new(0);
    let server = TestServer::start(move |r: &Recorded| {
        if calls.fetch_add(1, Ordering::SeqCst) == 0 {
            Reply::hang_up()
        } else {
            success(r, ACTION)
        }
    })
    .await;
    let scope = api(
        &server,
        retry_policy(2, Duration::ZERO, Duration::from_secs(2)),
    )
    .v71();
    let response = scope.payments("key", payment()).send().await.unwrap();
    assert_eq!(response.retry.attempts, 2);
    assert_eq!(response.value.state(), PaymentState::ActionRequired);
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].body, requests[1].body);
    assert_eq!(requests[0].headers, requests[1].headers);
    server.shutdown().await;

    let server = TestServer::start(|_: &Recorded| Reply::hang_up()).await;
    let error = api(
        &server,
        retry_policy(2, Duration::ZERO, Duration::from_secs(2)),
    )
    .v71()
    .details("details", details())
    .send()
    .await
    .unwrap_err();
    let evidence = error.reconciliation().unwrap();
    assert_eq!(evidence.reason, ReconciliationReason::UncertainDelivery);
    assert_eq!(
        evidence.cause.as_ref().unwrap().kind(),
        concord::ErrorKind::Transport
    );
    assert_eq!(evidence.retry.as_ref().unwrap().attempts, 2);
    assert_hidden(&format!("{error:?} {error}"));
    server.shutdown().await;
}

#[tokio::test]
async fn expired_or_explicitly_revoked_guarantees_never_start_a_fresh_payment() {
    let server = TestServer::start(|r: &Recorded| success(r, AUTHORISED)).await;
    let policy = SendPolicy::default()
        .with_guarantee_window(Duration::from_secs(1))
        .unwrap();
    let scope = api(&server, policy.clone()).v71();
    let expired = scope.payments("expired", payment());
    let revoked = scope.payments("revoked", payment());
    expired.clone().send().await.unwrap();
    revoked.clone().send().await.unwrap();
    policy.revoke("revoked").unwrap();
    assert_eq!(
        revoked
            .send()
            .await
            .unwrap_err()
            .reconciliation()
            .unwrap()
            .reason,
        ReconciliationReason::GuaranteeRevoked
    );
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(1)).await;
    assert_eq!(
        expired
            .send()
            .await
            .unwrap_err()
            .reconciliation()
            .unwrap()
            .reason,
        ReconciliationReason::GuaranteeExpired
    );
    tokio::time::resume();
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

#[tokio::test]
async fn guarantee_expiring_during_backoff_prevents_the_next_wire_attempt() {
    let server = TestServer::start(|_: &Recorded| {
        Reply::json(503, UNAVAILABLE).header("transient-error", "true")
    })
    .await;
    let policy = retry_policy(3, Duration::from_millis(30), Duration::from_secs(2))
        .with_guarantee_window(Duration::from_millis(20))
        .unwrap();
    let error = api(&server, policy)
        .v71()
        .payments("key", payment())
        .send()
        .await
        .unwrap_err();
    assert_eq!(
        error.reconciliation().unwrap().reason,
        ReconciliationReason::GuaranteeExpired
    );
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

#[tokio::test]
async fn exhausted_703_and_704_surface_reconciliation_without_removing_the_key() {
    for (status, fixture, reason) in [
        (503, UNAVAILABLE, ReconciliationReason::GuaranteeUnavailable),
        (409, IN_PROGRESS, ReconciliationReason::InProgress),
        (422, IN_PROGRESS, ReconciliationReason::InProgress),
    ] {
        let server = TestServer::start(move |_: &Recorded| {
            Reply::json(status, fixture).header("transient-error", "true")
        })
        .await;
        let scope = api(
            &server,
            retry_policy(2, Duration::ZERO, Duration::from_secs(2)),
        )
        .v71();
        let operation = scope.payments("key", payment());
        let error = operation.clone().send().await.unwrap_err();
        let evidence = error.reconciliation().unwrap();
        assert_eq!(evidence.reason, reason);
        let provider = evidence.provider.as_ref().unwrap();
        assert_eq!(provider.meta.status().as_u16(), status);
        assert_eq!(provider.retry.stop, concord_retry::StopReason::Attempts);
        assert_eq!(provider.retry.attempts, 2);
        if status == 503 {
            assert_eq!(
                operation
                    .send()
                    .await
                    .unwrap_err()
                    .reconciliation()
                    .unwrap()
                    .reason,
                ReconciliationReason::GuaranteeRevoked
            );
        }
        assert_eq!(server.requests().len(), 2);
        server.shutdown().await;
    }
}

// Paused clock (the SDK's deadline is a Tokio timer): the 429 arrives while time stands still, so
// the whole 50 ms budget remains when the policy weighs `Retry-After: 60` against it.
#[tokio::test(start_paused = true)]
async fn retry_after_minimum_above_budget_stops_without_early_retry_and_preserves_http_failure() {
    let server = TestServer::start(|_: &Recorded| {
        Reply::json(429, PROBLEM)
            .header("transient-error", "true")
            .header("retry-after", "60")
    })
    .await;
    let checkout = api(
        &server,
        retry_policy(3, Duration::ZERO, Duration::from_millis(50)),
    );
    let error = paused::drive(checkout.v71().payments("key", payment()).send())
        .await
        .unwrap_err();
    let evidence = error.reconciliation().unwrap();
    assert_eq!(evidence.reason, ReconciliationReason::Deadline);
    assert_eq!(
        evidence.retry.as_ref().unwrap().stop,
        concord_retry::StopReason::Deadline
    );
    assert_eq!(evidence.meta.as_ref().unwrap().status().as_u16(), 429);
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

#[tokio::test]
async fn simultaneous_same_key_and_cancelled_send_keep_the_original_binding() {
    let gate = Gate::new();
    let reply_gate = gate.clone();
    let server =
        TestServer::start(move |r: &Recorded| success(r, AUTHORISED).after(reply_gate.clone()))
            .await;
    let scope = api(&server, SendPolicy::default()).v71();
    let operation = scope.payments("key", payment());
    let active = tokio::spawn(operation.clone().send());
    assert!(
        server
            .wait_until(Duration::from_secs(2), |s| s.requests().len() == 1)
            .await
    );
    let busy = operation.clone().send().await.unwrap_err();
    assert_eq!(
        busy.reconciliation().unwrap().reason,
        ReconciliationReason::InFlight
    );
    let mut changed = payment();
    changed.reference = "different".into();
    let changed = scope.payments("key", changed).send().await.unwrap_err();
    assert_eq!(
        changed.reconciliation().unwrap().reason,
        ReconciliationReason::ChangedRequest
    );
    active.abort();
    assert!(active.await.unwrap_err().is_cancelled());
    gate.open();
    operation.send().await.unwrap();
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

#[tokio::test]
async fn changing_route_or_destination_under_the_same_key_is_reconciliation() {
    let server = TestServer::start(|r: &Recorded| success(r, AUTHORISED)).await;
    let other = TestServer::start(|_: &Recorded| {
        panic!("different destination must not receive the request")
    })
    .await;
    let api = api(&server, SendPolicy::default());
    api.v71().payments("key", payment()).send().await.unwrap();
    let route = api
        .v71()
        .details("key", details())
        .send()
        .await
        .unwrap_err();
    assert_eq!(
        route.reconciliation().unwrap().reason,
        ReconciliationReason::ChangedRequest
    );
    let destination = api
        .with_base_url(other.url())
        .unwrap()
        .v71()
        .payments("key", payment())
        .send()
        .await
        .unwrap_err();
    assert_eq!(
        destination.reconciliation().unwrap().reason,
        ReconciliationReason::ChangedRequest
    );
    assert_eq!(server.requests().len(), 1);
    assert!(other.requests().is_empty());
    server.shutdown().await;
    other.shutdown().await;
}

#[tokio::test]
async fn stalled_attempt_hits_total_deadline_and_http_success_can_still_be_refused() {
    // Delay the head: Retry's total deadline covers attempts through receipt of the head.
    let delayed = TestServer::start(|_: &Recorded| {
        Reply::json(200, AUTHORISED).delay(Duration::from_secs(2))
    })
    .await;
    // Paused clock (the SDK's deadline is a Tokio timer): the 20 ms deadline fires only once the
    // server holds the attempt.
    let deadline = Duration::from_millis(20);
    let checkout = api(&delayed, retry_policy(2, Duration::ZERO, deadline));
    tokio::time::pause();
    let send = checkout.v71().payments("key", payment()).send();
    let error = paused::time_out(&delayed, |_| true, deadline, send)
        .await
        .unwrap_err();
    tokio::time::resume();
    assert_eq!(
        error.reconciliation().unwrap().reason,
        ReconciliationReason::Deadline
    );
    assert_eq!(delayed.requests().len(), 1);
    delayed.shutdown().await;

    let server =
        TestServer::start(|r: &Recorded| success(r, include_str!("../fixtures/refused.json")))
            .await;
    let response = api(&server, SendPolicy::default())
        .v71()
        .payments("key", payment())
        .send()
        .await
        .unwrap();
    assert_eq!(response.value.state(), PaymentState::Unsuccessful);
    assert_eq!(response.retry.attempts, 1);
    assert_hidden(&format!("{response:?}"));
    server.shutdown().await;
}

#[tokio::test]
async fn reconstructed_key_rotation_retains_the_company_ledger() {
    let server = TestServer::start(|r: &Recorded| success(r, AUTHORISED)).await;
    let policy = SendPolicy::default();
    let first = api(&server, policy.clone());
    first
        .v71()
        .payments("before-rotation", payment())
        .send()
        .await
        .unwrap();
    let rotated = Checkout::sandbox("synthetic-rotated-key", policy)
        .unwrap()
        .with_base_url(server.url())
        .unwrap();
    let error = rotated
        .v71()
        .payments("before-rotation", payment())
        .send()
        .await
        .unwrap_err();
    assert_eq!(
        error.reconciliation().unwrap().reason,
        ReconciliationReason::ChangedRequest
    );
    rotated
        .v71()
        .payments("after-rotation", payment())
        .send()
        .await
        .unwrap();
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0].header("x-api-key"),
        Some("synthetic-api-secret")
    );
    assert_eq!(
        requests[1].header("x-api-key"),
        Some("synthetic-rotated-key")
    );
    server.shutdown().await;
}

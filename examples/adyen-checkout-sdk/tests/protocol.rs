//! Wire-level Checkout contracts and hostile response fixtures.
mod common;
use common::*;
use concord_example_testserver::{Body, Recorded, Reply, TestServer, paused};
use concord_sdk_adyen_checkout::{concord::ErrorKind, *};
use std::{num::NonZeroUsize, time::Duration};

#[tokio::test]
async fn wire_contract_for_all_three_routes_and_encrypted_test_card() {
    let server = TestServer::start(|r: &Recorded| {
        assert_eq!(r.method, "POST");
        assert_eq!(r.query(), None);
        assert_eq!(r.header("x-api-key"), Some("synthetic-api-secret"));
        assert_eq!(r.header("content-type"), Some("application/json"));
        assert_eq!(r.header("accept"), Some("application/json"));
        assert!(!r.chunked);
        match r.path() {
            "/v71/paymentMethods" => {
                assert_body(r, include_str!("../fixtures/methods-request.json"));
                success(r, METHODS)
            }
            "/v71/payments" if r.header("idempotency-key") == Some("card") => {
                assert_body(r, include_str!("../fixtures/card-request.json"));
                success(r, AUTHORISED)
            }
            "/v71/payments" => {
                assert_body(r, include_str!("../fixtures/payment-request.json"));
                success(r, ACTION)
            }
            "/v71/payments/details" => {
                assert_body(r, include_str!("../fixtures/details-request.json"));
                success(r, AUTHORISED)
            }
            _ => panic!("unexpected route"),
        }
    })
    .await;
    Checkout::validate_header_names().unwrap();
    let scope = api(&server, SendPolicy::default()).v71();
    assert_eq!(
        scope
            .payment_methods("methods", methods())
            .send()
            .await
            .unwrap()
            .value
            .payment_methods
            .len(),
        2
    );
    assert_eq!(
        scope
            .payments("payment", payment())
            .send()
            .await
            .unwrap()
            .value
            .state(),
        PaymentState::ActionRequired
    );
    assert_eq!(
        scope
            .details("details", details())
            .send()
            .await
            .unwrap()
            .value
            .state(),
        PaymentState::Authorised
    );
    let mut card = payment();
    card.payment_method =
        SandboxMethod::test_card("test_4111111111111111", "test_03", "test_2030", "test_737")
            .unwrap();
    scope.payments("card", card).send().await.unwrap();
    assert_eq!(server.requests().len(), 4);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn every_documented_http_failure_class_keeps_actual_status_and_document() {
    let cases: Vec<(u16, String)> =
        serde_json::from_str(include_str!("../fixtures/status-cases.json")).unwrap();
    for (status, expected_class) in cases {
        let server = TestServer::start(move |_: &Recorded| Reply::json(status, PROBLEM)).await;
        let error = api(&server, SendPolicy::default())
            .v71()
            .payments("synthetic-key-secret", payment())
            .send()
            .await
            .unwrap_err();
        assert_hidden(&format!("{error:?} {error}"));
        let Error::Provider(failure) = error else {
            panic!("expected authoritative HTTP failure")
        };
        assert_eq!(failure.meta.status().as_u16(), status);
        assert_eq!(format!("{:?}", failure.class), expected_class);
        // Fixture deliberately reports 422 regardless of actual status; it cannot replace it.
        let problem = failure.document.unwrap();
        assert_eq!(problem.status, 422);
        assert_eq!(problem.error_code, "14_001");
        assert_hidden(&format!("{problem:?} {problem}"));
        assert_eq!(failure.retry.attempts, 1);
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

fn hostile(case: usize, status: u16) -> Reply {
    match case {
        0 => Reply::json(status, include_str!("../fixtures/malformed.txt")),
        1 => Reply::json(status, include_str!("../fixtures/truncated.txt")),
        2 => Reply::json(status, vec![b' '; RESPONSE_LIMIT + 1]),
        3 => Reply::text(status, "text/html", "synthetic-body-secret"),
        4 => Reply::new(status).body("synthetic-body-secret"),
        5 => Reply::json(status, AUTHORISED).header("content-type", "application/json"),
        6 => Reply::json(status, "{}").with_body(Body::FailAfter(vec![
            concord::bytes::Bytes::from_static(b"{\"resultCode\":"),
        ])),
        7 => Reply::json(status, "{}"),
        _ => unreachable!(),
    }
}

#[tokio::test]
async fn malformed_truncated_oversized_wrong_media_and_interrupted_success_are_uncertain() {
    let kinds = [
        ErrorKind::Decode,
        ErrorKind::Decode,
        ErrorKind::Limit,
        ErrorKind::Metadata,
        ErrorKind::Metadata,
        ErrorKind::Metadata,
        ErrorKind::Transport,
        ErrorKind::Decode,
    ];
    for (case, kind) in kinds.into_iter().enumerate() {
        let server = TestServer::start(move |r: &Recorded| {
            hostile(case, 200).header("idempotency-key", r.header("idempotency-key").unwrap())
        })
        .await;
        let error = api(&server, SendPolicy::default())
            .v71()
            .payments("synthetic-key-secret", payment())
            .send()
            .await
            .unwrap_err();
        assert_hidden(&format!("{error:?} {error}"));
        let evidence = error.reconciliation().unwrap();
        assert_eq!(evidence.reason, ReconciliationReason::UncertainDelivery);
        let cause = evidence.cause.as_ref().unwrap();
        assert_eq!(cause.kind(), kind);
        assert_eq!(cause.head().unwrap().status().as_u16(), 200);
        assert_eq!(evidence.retry.as_ref().unwrap().attempts, 1);
        assert_eq!(
            server.requests().len(),
            1,
            "body failures must not restart a payment"
        );
        server.shutdown().await;
    }
}

#[tokio::test]
async fn hostile_provider_error_bodies_do_not_hide_http_failure() {
    let kinds = [
        ErrorKind::Decode,
        ErrorKind::Decode,
        ErrorKind::Limit,
        ErrorKind::Metadata,
        ErrorKind::Metadata,
        ErrorKind::Metadata,
        ErrorKind::Transport,
        ErrorKind::Decode,
    ];
    for (case, kind) in kinds.into_iter().enumerate() {
        let server = TestServer::start(move |_: &Recorded| hostile(case, 422)).await;
        let error = api(&server, SendPolicy::default())
            .v71()
            .details("synthetic-key-secret", details())
            .send()
            .await
            .unwrap_err();
        assert_hidden(&format!("{error:?} {error}"));
        let Error::Provider(failure) = error else {
            panic!("expected provider failure")
        };
        assert_eq!(failure.meta.status().as_u16(), 422);
        assert_eq!(failure.class, HttpClass::Validation);
        assert_eq!(failure.document.unwrap_err().kind(), kind);
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn absent_false_repeated_or_invalid_retry_permission_never_repeats() {
    for case in 0..6 {
        let server = TestServer::start(move |_: &Recorded| {
            let reply = Reply::json(429, PROBLEM);
            match case {
                0 => reply,
                1 => reply.header("transient-error", "false"),
                2 => reply
                    .header("transient-error", "true")
                    .header("transient-error", "true"),
                3 => reply.header("transient-error", "TRUE"),
                4 => reply
                    .header("transient-error", "true")
                    .header("retry-after", "invalid-secret"),
                5 => reply
                    .header("transient-error", "true")
                    .header("retry-after", "0")
                    .header("retry-after", "0"),
                _ => unreachable!(),
            }
        })
        .await;
        let error = api(&server, SendPolicy::default())
            .v71()
            .payments("key", payment())
            .send()
            .await
            .unwrap_err();
        let Error::Provider(failure) = error else {
            panic!("expected provider failure")
        };
        assert!(!failure.transient);
        assert_eq!(failure.retry.attempts, 1);
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn success_must_confirm_the_same_key_or_require_reconciliation_and_revoke_it() {
    for case in 0..3 {
        let server = TestServer::start(move |r: &Recorded| match case {
            0 => Reply::json(200, AUTHORISED),
            1 => Reply::json(200, AUTHORISED).header("idempotency-key", "different-secret"),
            2 => success(r, AUTHORISED)
                .header("idempotency-key", r.header("idempotency-key").unwrap()),
            _ => unreachable!(),
        })
        .await;
        let scope = api(&server, SendPolicy::default()).v71();
        let operation = scope.payments("key", payment());
        let error = operation.clone().send().await.unwrap_err();
        assert_eq!(
            error.reconciliation().unwrap().reason,
            ReconciliationReason::GuaranteeNotConfirmed
        );
        assert_eq!(
            error
                .reconciliation()
                .unwrap()
                .meta
                .as_ref()
                .unwrap()
                .status()
                .as_u16(),
            200
        );
        let again = operation.send().await.unwrap_err();
        assert_eq!(
            again.reconciliation().unwrap().reason,
            ReconciliationReason::GuaranteeRevoked
        );
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn all_diagnostic_boundaries_withhold_secrets() {
    let server = TestServer::start(|r: &Recorded| {
        success(r, ACTION).header("x-request-id", "synthetic-provider-secret")
    })
    .await;
    let api = api(&server, SendPolicy::default());
    let scope = api.v71();
    let operation = scope.payments("synthetic-key-secret", payment());
    let prepared = operation.clone().prepare().unwrap();
    assert!(prepared.request().headers()["idempotency-key"].is_sensitive());
    assert!(prepared.request().headers()["x-api-key"].is_sensitive());
    assert_hidden(&format!(
        "{api:?} {scope:?} {operation:?} {prepared:?} {:?}",
        SendPolicy::default()
    ));
    let response = operation.send().await.unwrap();
    assert_hidden(&format!(
        "{response:?} {:?} {:?}",
        response.value,
        response.value.action.as_ref().unwrap()
    ));
    assert_hidden(&format!("{:?} {:?} {:?}", payment(), details(), methods()));
    let card =
        SandboxMethod::test_card("test_4111111111111111", "test_03", "test_2030", "test_737")
            .unwrap();
    assert_hidden(&format!("{card:?}"));
    for fixture in [
        include_str!("../fixtures/refused.json"),
        include_str!("../fixtures/next-action.json"),
    ] {
        let value: PaymentResponse = serde_json::from_str(fixture).unwrap();
        assert_hidden(&format!("{value:?}"));
    }
    server.shutdown().await;
}

#[tokio::test]
async fn input_failures_and_ledger_capacity_dispatch_nothing_extra() {
    let server = TestServer::start(|r: &Recorded| success(r, AUTHORISED)).await;
    let policy = SendPolicy::new(NonZeroUsize::new(1).unwrap());
    let api = api(&server, policy.clone());
    for key in ["", "a b", &"x".repeat(65)] {
        assert!(matches!(
            api.v71().payments(key, payment()).send().await,
            Err(Error::InvalidInput(_))
        ));
    }
    let error = api
        .v71()
        .payments("a\nb", payment())
        .send()
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Concord(ref e) if e.kind() == ErrorKind::Build));
    let mut large = payment();
    large.reference = "x".repeat(REQUEST_LIMIT);
    assert!(matches!(
        api.v71().payments("large", large).send().await,
        Err(Error::InvalidInput(_))
    ));
    assert!(server.requests().is_empty());
    api.v71()
        .payments("original", payment())
        .send()
        .await
        .unwrap();
    assert!(matches!(
        api.v71().payments("another", payment()).send().await,
        Err(Error::LedgerFull)
    ));
    api.v71()
        .payments("original", payment())
        .send()
        .await
        .unwrap();
    assert_eq!(server.requests().len(), 2);
    assert!(policy.revoke("unknown").is_err());
    assert!(
        policy
            .clone()
            .with_guarantee_window(Duration::ZERO)
            .is_err()
    );
    assert!(
        policy
            .clone()
            .with_guarantee_window(GUARANTEE_WINDOW + Duration::from_secs(1))
            .is_err()
    );
    assert!(
        policy
            .with_retry(concord_retry::Policy::new(NonZeroUsize::new(1).unwrap()))
            .is_err()
    );
    assert!(Checkout::sandbox("synthetic-api-secret\n", SendPolicy::default()).is_err());
    assert!(SandboxMethod::test_card("4111111111111111", "03", "2030", "737").is_err());
    server.shutdown().await;
}

#[tokio::test]
async fn live_destinations_are_rejected_and_redirects_are_not_followed() {
    let destination =
        TestServer::start(|_: &Recorded| panic!("redirect must not be followed")).await;
    let location = destination.url();
    let server =
        TestServer::start(move |_: &Recorded| Reply::new(302).header("location", &location)).await;
    let api = api(&server, SendPolicy::default());
    let error = api
        .v71()
        .payments("key", payment())
        .send()
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Provider(ref e) if e.meta.status().as_u16() == 302));
    let live = api
        .with_base_url("https://checkout-live.adyenpayments.com")
        .unwrap();
    assert!(matches!(
        live.v71().payments("live-key", payment()).send().await,
        Err(Error::InvalidInput(_))
    ));
    assert!(destination.requests().is_empty());
    server.shutdown().await;
    destination.shutdown().await;
}

#[tokio::test]
async fn asynchronous_partial_unknown_and_action_states_never_invent_authorisation() {
    let cases: Vec<(serde_json::Value, String)> =
        serde_json::from_str(include_str!("../fixtures/state-cases.json")).unwrap();
    for (document, expected) in cases {
        let server = TestServer::start(move |r: &Recorded| success(r, &document.to_string())).await;
        let response = api(&server, SendPolicy::default())
            .v71()
            .payments("key", payment())
            .send()
            .await
            .unwrap();
        assert_eq!(format!("{:?}", response.value.state()), expected);
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn lost_assurance_on_a_transient_error_prevents_replay_before_body_consumption() {
    let server = TestServer::start(|_: &Recorded| {
        Reply::json(503, UNAVAILABLE)
            .header("transient-error", "true")
            .header("idempotency-key", "different-key")
    })
    .await;
    let error = api(&server, SendPolicy::default())
        .v71()
        .payments("key", payment())
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

// Paused clock: the 50 ms timeout fires only once the server has written the head, so it ends the
// stalled body and never the wait for the head (which the policy would retry).
#[tokio::test(start_paused = true)]
async fn stalled_final_bodies_are_bounded_by_transport_timeout_without_restarting() {
    for status in [200, 422] {
        let server = TestServer::start(move |r: &Recorded| {
            Reply::json(status, "{}")
                .header("idempotency-key", r.header("idempotency-key").unwrap())
                .with_body(Body::Stall)
        })
        .await;
        let timeout = Duration::from_millis(50);
        let http = concord::reqwest::Client::builder()
            .redirect(concord::reqwest::redirect::Policy::none())
            .retry(concord::reqwest::retry::never())
            .timeout(timeout)
            .build()
            .unwrap();
        let api = Checkout::new(
            http,
            CheckoutConfig {
                api_key: "synthetic-api-secret".into(),
                policy: SendPolicy::default(),
            },
        )
        .with_base_url(server.url())
        .unwrap();
        let v71 = api.v71();
        let send = v71.payments("key", payment()).send();
        let error = paused::time_out_in_body(&server, |_| true, timeout, send)
            .await
            .unwrap_err();
        match error {
            Error::Provider(e) => assert_eq!(e.document.unwrap_err().kind(), ErrorKind::Transport),
            Error::Reconciliation(e) => assert_eq!(e.cause.unwrap().kind(), ErrorKind::Transport),
            _ => panic!("expected final-body failure"),
        }
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn all_documented_error_types_and_future_types_are_preserved_and_redacted() {
    let documents: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../fixtures/error-types.json")).unwrap();
    for document in documents {
        let status = document["status"].as_u64().unwrap() as u16;
        let expected_type = document["errorType"].as_str().unwrap().to_owned();
        let expected_code = document["errorCode"].as_str().unwrap().to_owned();
        let server =
            TestServer::start(move |_: &Recorded| Reply::json(status, document.to_string())).await;
        let error = api(&server, SendPolicy::default())
            .v71()
            .payments("key", payment())
            .send()
            .await
            .unwrap_err();
        assert_hidden(&format!("{error:?} {error}"));
        let Error::Provider(failure) = error else {
            panic!("expected provider failure")
        };
        let problem = failure.document.unwrap();
        assert_eq!(problem.error_type, expected_type);
        assert_eq!(problem.error_code, expected_code);
        assert!(problem.psp_reference.is_none());
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

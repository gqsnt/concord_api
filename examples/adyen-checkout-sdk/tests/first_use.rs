//! Fresh-consumer application tests (see FIRST_USE.md). Loopback fixtures only; the real
//! Adyen API is never called. Fixture bodies are written from Adyen's public documentation
//! (read 2026-11-07), not copied from this SDK's own fixtures.

use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use concord_example_testserver::{Gate, Recorded, Reply, Script, TestServer};
use concord_retry::{Policy, StopReason};
use concord_sdk_adyen_checkout::{
    Amount, Checkout, CheckoutV71, DetailsRequest, Error, HttpClass, MethodsRequest,
    PaymentRequest, PaymentState, ReconciliationReason, SandboxMethod, SendPolicy, Web,
};

const API_KEY: &str = "AQEyhmfxK4-synthetic-test-key-not-real";
const MERCHANT: &str = "ShopExampleNL_ECOM_TEST";

const METHODS_JSON: &str = r#"{
  "paymentMethods": [
    {"type":"scheme","name":"Cards","brands":["visa","mc","amex"]},
    {"type":"ideal","name":"iDEAL","issuers":[
      {"id":"1121","name":"Test Issuer"},
      {"id":"1154","name":"Test Issuer 5"}
    ]},
    {"type":"klarna_paynow","name":"Pay now with Klarna."}
  ]
}"#;

const REDIRECT_JSON: &str = r#"{
  "resultCode":"RedirectShopper",
  "action":{
    "paymentMethodType":"ideal",
    "url":"https://checkoutshopper-test.adyen.com/checkoutshopper/checkoutPaymentRedirect?redirectData=X6Xtf",
    "method":"GET",
    "type":"redirect"
  }
}"#;

const ERR_703: &str = r#"{"status":503,"errorCode":"703","message":"Required resource temporarily unavailable","errorType":"internal","pspReference":"QFQTPCQ8HXSKGK82"}"#;
const ERR_704: &str = r#"{"status":422,"errorCode":"704","message":"request already processed or in progress","errorType":"validation","pspReference":"PSP704REF"}"#;
const ERR_401: &str = r#"{"status":401,"errorCode":"000","message":"HTTP Status Response - Unauthorized","errorType":"security"}"#;

/// A success or error reply that echoes the request's idempotency key, as Adyen documents.
fn echo(request: &Recorded, status: u16, body: &str) -> Reply {
    let key = request
        .header("idempotency-key")
        .unwrap_or_default()
        .to_string();
    Reply::json(status, body).header("idempotency-key", key)
}

fn checkout(server: &TestServer, policy: SendPolicy) -> CheckoutV71 {
    Checkout::sandbox(API_KEY, policy)
        .expect("sandbox client")
        .with_base_url(server.url())
        .expect("loopback base url")
        .v71()
}

fn amount() -> Amount {
    Amount {
        currency: "EUR".into(),
        value: 2495,
    }
}

fn methods_request() -> MethodsRequest {
    MethodsRequest {
        merchant_account: MERCHANT.into(),
        amount: amount(),
        country_code: Some("NL".into()),
        shopper_locale: Some("nl-NL".into()),
        channel: Web::Web,
    }
}

fn payment_request(method: SandboxMethod) -> PaymentRequest {
    PaymentRequest {
        merchant_account: MERCHANT.into(),
        amount: amount(),
        reference: "order-7731".into(),
        return_url: "https://shop.example/checkout/return?order=7731".into(),
        payment_method: method,
        channel: Web::Web,
    }
}

fn json(r: &Recorded) -> serde_json::Value {
    serde_json::from_slice(&r.body).expect("request body is JSON")
}

/// Fast, bounded retries so failure tests do not spend real seconds in backoff.
fn fast_retry() -> Policy {
    let mut policy = Policy::new(NonZeroUsize::new(3).unwrap());
    policy.timeout = Some(Duration::from_secs(5));
    policy.initial_delay = Duration::from_millis(5);
    policy.max_delay = Duration::from_millis(20);
    policy
}

fn assert_redacted(text: &str, secrets: &[&str]) {
    for secret in secrets {
        assert!(
            !text.contains(secret),
            "formatted output leaked {secret:?}: {text}"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Workflow 1: list available payment methods, then submit a sandbox payment.
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn workflow1_methods_then_ideal_payment_survives_transient_703() {
    let payments = Script::new([
        // The idempotency store is briefly unavailable: transient, no side effects.
        Reply::new(503)
            .header("content-type", "application/json")
            .body(ERR_703),
        Reply::json(200, REDIRECT_JSON),
        // Deliberate caller retry: Adyen replays the original response for the same key.
        Reply::json(200, REDIRECT_JSON),
    ]);
    let server = TestServer::start(move |r: &Recorded| {
        let key = r.header("idempotency-key").unwrap_or_default().to_string();
        match r.path() {
            "/v71/paymentMethods" => echo(r, 200, METHODS_JSON),
            "/v71/payments" => {
                let reply = payments.next().header("idempotency-key", key);
                if reply.status == 503 {
                    reply.header("transient-error", "true")
                } else {
                    reply
                }
            }
            other => Reply::text(404, "text/plain", format!("unexpected {other}")),
        }
    })
    .await;
    let checkout = checkout(&server, SendPolicy::default());

    // 1. Available methods.
    let methods = checkout
        .payment_methods(
            "methods-3f1c2a9e-0b7d-4e55-a1f2-5d7e9c0b1a11",
            methods_request(),
        )
        .send()
        .await
        .expect("methods listed");
    assert_eq!(methods.meta.status().as_u16(), 200);
    assert_eq!(methods.retry.attempts, 1);
    let kinds: Vec<&str> = methods
        .value
        .payment_methods
        .iter()
        .map(|m| m.kind.as_str())
        .collect();
    assert_eq!(kinds, ["scheme", "ideal", "klarna_paynow"]);
    let ideal = methods
        .value
        .payment_methods
        .iter()
        .find(|m| m.kind == "ideal")
        .unwrap();
    assert_eq!(ideal.name.as_deref(), Some("iDEAL"));
    let issuer = ideal.attributes["issuers"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(issuer, "1121");
    let cards = &methods.value.payment_methods[0];
    assert_eq!(
        cards.attributes["brands"],
        serde_json::json!(["visa", "mc", "amex"])
    );

    // 2. Submit the payment; the first attempt hits a transient 703.
    let payment_key = "payment-8a2b6c4d-91e3-4f0a-b7c5-2e6d8f1a3b57";
    let submission = checkout.payments(
        payment_key,
        payment_request(SandboxMethod::Ideal {
            issuer: issuer.clone(),
        }),
    );
    let payment = submission
        .clone()
        .send()
        .await
        .expect("payment accepted after a retry");
    assert_eq!(
        payment.retry.attempts, 2,
        "one automatic retry after the transient 703"
    );
    assert_eq!(payment.retry.stop, StopReason::Done);
    assert!(
        payment.retry.waits[0] >= Duration::from_millis(100),
        "{:?}",
        payment.retry.waits
    );
    assert_eq!(
        payment
            .meta
            .headers()
            .get("idempotency-key")
            .unwrap()
            .to_str()
            .unwrap(),
        payment_key
    );
    assert_eq!(payment.value.result_code, "RedirectShopper");
    assert_eq!(payment.value.state(), PaymentState::ActionRequired);
    assert_eq!(
        payment.value.psp_reference, None,
        "no pspReference before the redirect completes"
    );
    let action = payment.value.action.as_ref().expect("redirect action");
    assert_eq!(action.kind, "redirect");
    assert_eq!(action.fields["method"], "GET");
    assert_eq!(action.fields["paymentMethodType"], "ideal");
    assert!(
        action.fields["url"]
            .as_str()
            .unwrap()
            .starts_with("https://checkoutshopper-test")
    );
    assert_eq!(action.payment_data, None);

    // Wire evidence: methods once, then two byte-identical payment attempts with one key.
    let wire = server.requests();
    assert_eq!(wire.len(), 3);
    let m = &wire[0];
    assert_eq!(
        (m.method.as_str(), m.target.as_str()),
        ("POST", "/v71/paymentMethods")
    );
    assert_eq!(m.query(), None);
    assert_eq!(m.header("x-api-key"), Some(API_KEY));
    assert_eq!(
        m.header("idempotency-key"),
        Some("methods-3f1c2a9e-0b7d-4e55-a1f2-5d7e9c0b1a11")
    );
    assert_eq!(m.header("content-type"), Some("application/json"));
    assert_eq!(m.header("accept"), Some("application/json"));
    assert_eq!(
        json(m),
        serde_json::json!({
            "merchantAccount": MERCHANT,
            "amount": {"currency": "EUR", "value": 2495},
            "countryCode": "NL",
            "shopperLocale": "nl-NL",
            "channel": "Web"
        })
    );
    let (first, second) = (&wire[1], &wire[2]);
    assert_eq!(
        (first.method.as_str(), first.target.as_str()),
        ("POST", "/v71/payments")
    );
    assert_eq!(
        first.body, second.body,
        "automatic retry resends identical bytes"
    );
    assert_eq!(
        first.headers, second.headers,
        "automatic retry resends identical headers"
    );
    assert_eq!(first.header("idempotency-key"), Some(payment_key));
    assert_eq!(
        json(first),
        serde_json::json!({
            "merchantAccount": MERCHANT,
            "amount": {"currency": "EUR", "value": 2495},
            "reference": "order-7731",
            "returnUrl": "https://shop.example/checkout/return?order=7731",
            "paymentMethod": {"type": "ideal", "issuer": "1121"},
            "channel": "Web"
        })
    );

    // 3. A deliberate caller retry of the same operation is allowed and identical on the wire.
    let replayed = submission
        .clone()
        .send()
        .await
        .expect("identical replay is allowed");
    assert_eq!(replayed.value.result_code, "RedirectShopper");
    let wire = server.requests();
    assert_eq!(wire.len(), 4);
    assert_eq!(wire[3].body, wire[1].body);
    assert_eq!(wire[3].header("idempotency-key"), Some(payment_key));

    // 4. Changing the body under the same key must not reach the wire.
    let mut changed = payment_request(SandboxMethod::Ideal { issuer });
    changed.amount.value = 2595;
    let err = checkout
        .payments(payment_key, changed)
        .send()
        .await
        .unwrap_err();
    let rec = err
        .reconciliation()
        .expect("changed request needs reconciliation");
    assert_eq!(rec.reason, ReconciliationReason::ChangedRequest);
    assert!(rec.provider.is_none());
    assert_eq!(
        server.requests().len(),
        4,
        "changed request was refused locally"
    );
    assert_redacted(
        &format!("{err:?} {err}"),
        &[API_KEY, payment_key, "order-7731"],
    );

    // Response Debug output withholds credentials, keys and provider URLs.
    assert_redacted(
        &format!("{payment:?} {methods:?}"),
        &[API_KEY, payment_key, "redirectData=X6Xtf"],
    );
}

#[tokio::test]
async fn workflow1_card_tokens_refusal_in_200_and_input_guards() {
    let server = TestServer::start(|r: &Recorded| {
        echo(
            r,
            200,
            r#"{"resultCode":"Refused","refusalReason":"CVC Declined","refusalReasonCode":"24","pspReference":"V4HZ4RBFJGXXGN82","merchantReference":"order-7731"}"#,
        )
    })
    .await;
    let checkout = checkout(&server, SendPolicy::default());

    // Raw card data is refused before any I/O.
    let raw = SandboxMethod::test_card("4111111111111111", "03", "2030", "737");
    assert!(
        matches!(raw, Err(Error::InvalidInput(_))),
        "raw PAN must be rejected"
    );
    assert!(server.requests().is_empty());

    let card =
        SandboxMethod::test_card("test_4111111111111111", "test_03", "test_2030", "test_737")
            .expect("Adyen test_ tokens");
    let key = "payment-card-5c7e1d2a-6b3f-4a8e-9d0c-7f1e2a3b4c5d";
    let refused = checkout
        .payments(key, payment_request(card))
        .send()
        .await
        .expect("a refusal in HTTP 200 is a normal response");
    assert_eq!(refused.value.state(), PaymentState::Unsuccessful);
    assert_eq!(refused.value.result_code, "Refused");
    assert_eq!(
        refused.value.refusal_reason.as_deref(),
        Some("CVC Declined")
    );
    assert_eq!(
        refused.value.psp_reference.as_deref(),
        Some("V4HZ4RBFJGXXGN82")
    );
    assert_eq!(
        refused.value.merchant_reference.as_deref(),
        Some("order-7731")
    );
    assert_eq!(refused.value.fields["refusalReasonCode"], "24");
    assert_redacted(&format!("{refused:?}"), &["CVC Declined", API_KEY, key]);

    let wire = server.single();
    assert_eq!(
        json(&wire)["paymentMethod"],
        serde_json::json!({
            "type": "scheme",
            "encryptedCardNumber": "test_4111111111111111",
            "encryptedExpiryMonth": "test_03",
            "encryptedExpiryYear": "test_2030",
            "encryptedSecurityCode": "test_737"
        })
    );
}

#[tokio::test]
async fn workflow1_methods_failures_are_classified() {
    let replies = Script::new([
        Reply::json(401, ERR_401),
        // Success without the idempotency-key echo.
        Reply::json(200, METHODS_JSON),
        // Success head, but the body is not JSON.
        Reply::json(200, "<html>gateway</html>"),
        // Provider omits the list when nothing is available.
        Reply::json(200, "{}"),
    ]);
    let server = TestServer::start(move |r: &Recorded| {
        let reply = replies.next();
        let echo_it = reply.status == 401 || replies.remaining() < 2;
        if echo_it {
            reply.header(
                "idempotency-key",
                r.header("idempotency-key").unwrap_or_default(),
            )
        } else {
            reply
        }
    })
    .await;
    let checkout = checkout(&server, SendPolicy::default());

    // 401: a provider failure with the parsed document; not retried.
    let err = checkout
        .payment_methods("methods-a1", methods_request())
        .send()
        .await
        .unwrap_err();
    let Error::Provider(failure) = &err else {
        panic!("expected Provider, got {err:?}")
    };
    assert_eq!(failure.class, HttpClass::Authentication);
    assert_eq!(failure.meta.status().as_u16(), 401);
    assert!(!failure.transient);
    assert_eq!(failure.retry.attempts, 1);
    let problem = failure.document.as_ref().expect("error document parsed");
    assert_eq!((problem.status, problem.error_code.as_str()), (401, "000"));
    assert_eq!(problem.error_type, "security");
    assert!(err.reconciliation().is_none());
    assert_redacted(
        &format!("{err:?} {err}"),
        &[API_KEY, "methods-a1", "Unauthorized"],
    );

    // Missing echo on a 2xx: the guarantee is not confirmed.
    let err = checkout
        .payment_methods("methods-a2", methods_request())
        .send()
        .await
        .unwrap_err();
    let rec = err.reconciliation().expect("unconfirmed guarantee");
    assert_eq!(rec.reason, ReconciliationReason::GuaranteeNotConfirmed);
    assert_eq!(rec.meta.as_ref().map(|m| m.status().as_u16()), Some(200));

    // Malformed success body.
    let err = checkout
        .payment_methods("methods-a3", methods_request())
        .send()
        .await
        .unwrap_err();
    let rec = err.reconciliation().expect("unreadable success document");
    assert_eq!(rec.reason, ReconciliationReason::UncertainDelivery);
    assert!(rec.cause.is_some(), "the decode failure is retained");

    // An absent list is an empty, complete result.
    let empty = checkout
        .payment_methods("methods-a4", methods_request())
        .send()
        .await
        .unwrap();
    assert!(empty.value.payment_methods.is_empty());
    assert_eq!(
        server.requests().len(),
        4,
        "no automatic retries for any of these"
    );
}

// ---------------------------------------------------------------------------------------------
// Workflow 2: continue an existing payment flow through /payments/details.
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn workflow2_redirect_return_is_submitted_after_a_dropped_connection() {
    let details = Script::new([
        Reply::hang_up(), // transport failure before any response head
        Reply::json(
            200,
            r#"{"resultCode":"Received","pspReference":"NC6HT9CRT65ZGN82","merchantReference":"order-7731","paymentMethod":{"type":"ideal","brand":"ideal"}}"#,
        ),
    ]);
    let server = TestServer::start(move |r: &Recorded| match r.path() {
        "/v71/payments" => echo(r, 200, REDIRECT_JSON),
        "/v71/payments/details" => {
            let reply = details.next();
            if reply.hang_up {
                reply
            } else {
                reply.header(
                    "idempotency-key",
                    r.header("idempotency-key").unwrap_or_default(),
                )
            }
        }
        other => Reply::text(404, "text/plain", format!("unexpected {other}")),
    })
    .await;
    let checkout = checkout(
        &server,
        SendPolicy::default().with_retry(fast_retry()).unwrap(),
    );

    let payment = checkout
        .payments(
            "payment-wf2-1",
            payment_request(SandboxMethod::Ideal {
                issuer: "1121".into(),
            }),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(payment.value.state(), PaymentState::ActionRequired);
    let action = payment.value.action.unwrap();

    // The shopper returns to returnUrl?redirectResult=...; the application decodes it once.
    let redirect_result = "X6XtfGC3!Y1dHJfMjM0NTY3OA==";
    let mut request = DetailsRequest::redirect(redirect_result);
    request.payment_data = action.payment_data; // None for this redirect
    let details_key = "details-wf2-1-2f9b";
    let continuation = checkout
        .details(details_key, request)
        .send()
        .await
        .expect("details");
    assert_eq!(
        continuation.retry.attempts, 2,
        "one automatic retry after the hang-up"
    );
    assert_eq!(continuation.value.result_code, "Received");
    assert_eq!(
        continuation.value.state(),
        PaymentState::Pending,
        "asynchronous outcome"
    );
    assert_eq!(
        continuation.value.psp_reference.as_deref(),
        Some("NC6HT9CRT65ZGN82")
    );
    assert_eq!(continuation.value.fields["paymentMethod"]["type"], "ideal");
    assert!(continuation.value.action.is_none());

    let wire = server.requests();
    assert_eq!(wire.len(), 3);
    let (a, b) = (&wire[1], &wire[2]);
    assert_eq!(
        (a.method.as_str(), a.target.as_str()),
        ("POST", "/v71/payments/details")
    );
    assert_eq!(a.body, b.body);
    assert_eq!(a.header("idempotency-key"), Some(details_key));
    assert_ne!(
        a.header("idempotency-key"),
        wire[0].header("idempotency-key")
    );
    assert_eq!(a.header("x-api-key"), Some(API_KEY));
    assert_eq!(
        json(a),
        serde_json::json!({"details": {"redirectResult": redirect_result}})
    );
}

#[tokio::test]
async fn workflow2_payment_data_is_forwarded_and_a_chained_action_is_surfaced() {
    let details = Script::new([
        // The continuation itself asks for another action (Pending + await).
        Reply::json(
            200,
            r#"{"resultCode":"Pending","action":{"type":"await","paymentMethodType":"mbway","paymentData":"Ab02b4c0!step2"}}"#,
        ),
        Reply::json(
            200,
            r#"{"resultCode":"Authorised","pspReference":"ZZ7GH8KQ2ABCDE82"}"#,
        ),
    ]);
    let server = TestServer::start(move |r: &Recorded| match r.path() {
        "/v71/payments" => echo(
            r,
            200,
            r#"{"resultCode":"RedirectShopper","action":{"type":"redirect","method":"POST","url":"https://test.adyen.com/hpp/3d/validate.shtml","data":{"MD":"M2RzMi","PaReq":"BQABAg","TermUrl":"https://shop.example/checkout/return"},"paymentData":"Ab02b4c0!step1","paymentMethodType":"scheme"}}"#,
        ),
        _ => {
            let key = r.header("idempotency-key").unwrap_or_default().to_string();
            details.next().header("idempotency-key", key)
        }
    })
    .await;
    let checkout = checkout(&server, SendPolicy::default());
    let card =
        SandboxMethod::test_card("test_5555444433331111", "test_03", "test_2030", "test_737")
            .unwrap();
    let payment = checkout
        .payments("payment-wf2-2", payment_request(card))
        .send()
        .await
        .unwrap();
    let action = payment.value.action.expect("redirect");
    assert_eq!(action.fields["method"], "POST");
    assert_eq!(action.fields["data"]["MD"], "M2RzMi");
    assert_eq!(action.payment_data.as_deref(), Some("Ab02b4c0!step1"));

    let mut step1 = DetailsRequest::redirect("Y2hhbGxlbmdlLXJlc3VsdA");
    step1.payment_data = action.payment_data;
    let next = checkout
        .details("details-wf2-2-a", step1)
        .send()
        .await
        .unwrap();
    assert_eq!(next.value.result_code, "Pending");
    assert_eq!(
        next.value.state(),
        PaymentState::ActionRequired,
        "an action takes precedence over Pending"
    );
    let await_action = next.value.action.expect("await action");
    assert_eq!(await_action.kind, "await");

    // The application obtains the next opaque result from its own UI/component.
    let mut step2 = DetailsRequest {
        details: Default::default(),
        payment_data: None,
    };
    step2
        .details
        .insert("payload".into(), "c3RlcDItcmVzdWx0".into());
    step2.payment_data = await_action.payment_data;
    let done = checkout
        .details("details-wf2-2-b", step2)
        .send()
        .await
        .unwrap();
    assert_eq!(done.value.state(), PaymentState::Authorised);
    assert_eq!(
        done.value.psp_reference.as_deref(),
        Some("ZZ7GH8KQ2ABCDE82")
    );

    let wire = server.requests();
    assert_eq!(wire.len(), 3);
    assert_eq!(
        json(&wire[1]),
        serde_json::json!({"details": {"redirectResult": "Y2hhbGxlbmdlLXJlc3VsdA"}, "paymentData": "Ab02b4c0!step1"})
    );
    assert_eq!(
        json(&wire[2]),
        serde_json::json!({"details": {"payload": "c3RlcDItcmVzdWx0"}, "paymentData": "Ab02b4c0!step2"})
    );
}

#[tokio::test]
async fn workflow2_704_in_progress_requires_reconciliation() {
    let server = TestServer::start(|r: &Recorded| echo(r, 422, ERR_704)).await;
    let checkout = checkout(&server, SendPolicy::default());
    let err = checkout
        .details("details-704", DetailsRequest::redirect("cmVzdWx0"))
        .send()
        .await
        .unwrap_err();
    let rec = err
        .reconciliation()
        .expect("704 is promoted to reconciliation");
    assert_eq!(rec.reason, ReconciliationReason::InProgress);
    let provider = rec.provider.as_ref().expect("provider failure retained");
    assert_eq!(provider.class, HttpClass::Validation);
    assert_eq!(provider.document.as_ref().unwrap().error_code, "704");
    assert_eq!(
        provider.document.as_ref().unwrap().psp_reference.as_deref(),
        Some("PSP704REF")
    );
    assert_eq!(
        server.requests().len(),
        1,
        "704 without transient-error is not retried"
    );
    assert_redacted(
        &format!("{err:?} {err}"),
        &["already processed", API_KEY, "details-704"],
    );
}

#[tokio::test]
async fn workflow2_704_as_http_409_is_a_conflict_in_progress() {
    // Adyen documents 704 with HTTP 422 or 409; the class follows the real status.
    let server = TestServer::start(|r: &Recorded| echo(r, 409, ERR_704)).await;
    let checkout = checkout(&server, SendPolicy::default());
    let err = checkout
        .details("details-704-409", DetailsRequest::redirect("cmVzdWx0"))
        .send()
        .await
        .unwrap_err();
    let rec = err
        .reconciliation()
        .expect("704 is promoted to reconciliation");
    assert_eq!(rec.reason, ReconciliationReason::InProgress);
    let provider = rec.provider.as_ref().unwrap();
    assert_eq!(provider.class, HttpClass::Conflict);
    assert_eq!(provider.meta.status().as_u16(), 409);
    assert_eq!(
        provider.document.as_ref().unwrap().status,
        422,
        "body status is not authoritative"
    );
}

#[tokio::test]
async fn workflow2_uncertain_delivery_allows_only_identical_replay() {
    let answer = Arc::new(AtomicUsize::new(0));
    let flag = answer.clone();
    let server = TestServer::start(move |r: &Recorded| {
        if flag.load(Ordering::SeqCst) == 0 {
            Reply::hang_up()
        } else {
            echo(
                r,
                200,
                r#"{"resultCode":"Authorised","pspReference":"AUTH000001"}"#,
            )
        }
    })
    .await;
    let checkout = checkout(
        &server,
        SendPolicy::default().with_retry(fast_retry()).unwrap(),
    );
    let key = "details-uncertain-77";
    let operation = checkout.details(key, DetailsRequest::redirect("dW5jZXJ0YWlu"));

    let err = operation.clone().send().await.unwrap_err();
    let rec = err.reconciliation().expect("no trustworthy response");
    assert_eq!(rec.reason, ReconciliationReason::UncertainDelivery);
    assert!(rec.meta.is_none(), "no head ever arrived");
    assert!(rec.cause.is_some());
    let report = rec.retry.as_ref().expect("attempt loop ran");
    assert_eq!(report.attempts, 3);
    assert_eq!(report.stop, StopReason::Attempts);
    assert_eq!(server.requests().len(), 3);

    // A changed continuation under the uncertain key is refused locally.
    let err = checkout
        .details(key, DetailsRequest::redirect("ZGlmZmVyZW50"))
        .send()
        .await
        .unwrap_err();
    assert_eq!(
        err.reconciliation().unwrap().reason,
        ReconciliationReason::ChangedRequest
    );
    assert_eq!(server.requests().len(), 3);

    // The provider is reachable again: the identical replay is permitted and completes.
    answer.store(1, Ordering::SeqCst);
    let ok = operation.clone().send().await.expect("identical replay");
    assert_eq!(ok.value.state(), PaymentState::Authorised);
    let wire = server.requests();
    assert_eq!(wire.len(), 4);
    assert!(
        wire.iter()
            .all(|r| r.body == wire[0].body && r.headers == wire[0].headers)
    );
}

#[tokio::test]
async fn workflow2_expired_guarantee_refuses_replay() {
    let server = TestServer::start(|_: &Recorded| Reply::hang_up()).await;
    let policy = SendPolicy::default()
        .with_retry(fast_retry())
        .unwrap()
        .with_guarantee_window(Duration::from_millis(400))
        .unwrap();
    let checkout = checkout(&server, policy);
    let operation = checkout.details("details-expiring", DetailsRequest::redirect("ZXhwaXJ5"));
    let err = operation.clone().send().await.unwrap_err();
    assert_eq!(
        err.reconciliation().unwrap().reason,
        ReconciliationReason::UncertainDelivery
    );
    let sent = server.requests().len();

    tokio::time::sleep(Duration::from_millis(500)).await;
    let err = operation.clone().send().await.unwrap_err();
    let rec = err.reconciliation().expect("expired");
    assert_eq!(rec.reason, ReconciliationReason::GuaranteeExpired);
    assert_eq!(
        server.requests().len(),
        sent,
        "an expired key is never replayed"
    );

    // Extending beyond Adyen's minimum retention is refused.
    assert!(
        SendPolicy::default()
            .with_guarantee_window(Duration::from_secs(30 * 86_400))
            .is_err()
    );
}

#[tokio::test]
async fn workflow2_concurrent_same_key_is_refused_while_in_flight() {
    let gate = Gate::new();
    let held = gate.clone();
    let server = TestServer::start(move |r: &Recorded| {
        echo(
            r,
            200,
            r#"{"resultCode":"Authorised","pspReference":"AUTH000002"}"#,
        )
        .after(held.clone())
    })
    .await;
    let checkout = checkout(&server, SendPolicy::default());
    let operation = checkout.details("details-inflight", DetailsRequest::redirect("aW5mbGlnaHQ"));
    let first = tokio::spawn(operation.clone().send());
    assert!(
        server
            .wait_until(Duration::from_secs(5), |s| s.requests().len() == 1)
            .await
    );

    let err = operation.clone().send().await.unwrap_err();
    assert_eq!(
        err.reconciliation().unwrap().reason,
        ReconciliationReason::InFlight
    );
    assert_eq!(server.requests().len(), 1);

    gate.open();
    let ok = first.await.unwrap().expect("first call completes");
    assert_eq!(ok.value.state(), PaymentState::Authorised);
}

#[tokio::test]
async fn workflow1_exhausted_703_revokes_the_key_and_other_inputs_are_refused() {
    let server = TestServer::start(|r: &Recorded| {
        if r.header("x-api-key") == Some(API_KEY) && r.path() == "/v71/payments" {
            echo(r, 503, ERR_703).header("transient-error", "true")
        } else {
            echo(r, 500, "{\"status\":500,\"errorCode\":\"9").header("transient-error", "true")
        }
    })
    .await;
    let policy = SendPolicy::default();
    let checkout = checkout(&server, policy.clone().with_retry(fast_retry()).unwrap());
    let request = || {
        payment_request(SandboxMethod::Ideal {
            issuer: "1154".into(),
        })
    };
    let key = "payment-703-1b2c3d4e";

    let err = checkout.payments(key, request()).send().await.unwrap_err();
    let rec = err.reconciliation().expect("idempotency store unavailable");
    assert_eq!(rec.reason, ReconciliationReason::GuaranteeUnavailable);
    let provider = rec.provider.as_ref().expect("final 703 retained");
    assert_eq!(provider.class, HttpClass::Server);
    assert!(provider.transient);
    assert_eq!(provider.document.as_ref().unwrap().error_code, "703");
    assert_eq!(provider.retry.attempts, 3);
    assert_eq!(provider.retry.stop, StopReason::Attempts);
    assert_eq!(server.requests().len(), 3);
    let first = &server.requests()[0];
    assert!(
        server
            .requests()
            .iter()
            .all(|r| r.body == first.body && r.headers == first.headers)
    );

    // Adyen suggests "pause and retry later" with the same key; this SDK has revoked it.
    let err = checkout.payments(key, request()).send().await.unwrap_err();
    assert_eq!(
        err.reconciliation().unwrap().reason,
        ReconciliationReason::GuaranteeRevoked
    );
    assert_eq!(
        server.requests().len(),
        3,
        "a revoked key is never replayed"
    );

    // A second credential of the same company, sharing the policy, cannot reuse the key.
    let other = Checkout::sandbox("second-credential-same-company", policy.clone())
        .unwrap()
        .with_base_url(server.url())
        .unwrap()
        .v71();
    let err = other.payments(key, request()).send().await.unwrap_err();
    assert_eq!(
        err.reconciliation().unwrap().reason,
        ReconciliationReason::ChangedRequest
    );
    assert_eq!(server.requests().len(), 3);

    // Keys must be 1..=64 visible ASCII characters; nothing is sent otherwise.
    for bad in ["", "has spaces", &"k".repeat(65)] {
        let err = checkout.payments(bad, request()).send().await.unwrap_err();
        assert!(matches!(err, Error::InvalidInput(_)), "{bad:?} -> {err}");
    }
    assert_eq!(server.requests().len(), 3);

    // A 64-character key is accepted; this transient 5xx has a truncated error document and
    // stays a provider failure after the bounded attempts.
    let err = other
        .payments("k".repeat(64), request())
        .send()
        .await
        .unwrap_err();
    let Error::Provider(failure) = &err else {
        panic!("expected Provider, got {err}")
    };
    assert_eq!(failure.class, HttpClass::Server);
    assert!(
        failure.document.is_err(),
        "truncated document is reported, not hidden"
    );
    assert_eq!(failure.retry.attempts, 3);
    assert!(err.reconciliation().is_none());
    assert_eq!(server.requests().len(), 6);
}

#[tokio::test]
async fn workflow2_guarantee_expiring_during_backoff_stops_before_dispatch() {
    let server =
        TestServer::start(|r: &Recorded| echo(r, 503, ERR_703).header("transient-error", "true"))
            .await;
    // Default backoff waits 100 ms after the first attempt; the window is shorter.
    let policy = SendPolicy::default()
        .with_guarantee_window(Duration::from_millis(50))
        .unwrap();
    let checkout = checkout(&server, policy);
    let err = checkout
        .details(
            "details-expiring-in-backoff",
            DetailsRequest::redirect("YmFja29mZg"),
        )
        .send()
        .await
        .unwrap_err();
    let rec = err.reconciliation().unwrap();
    assert_eq!(rec.reason, ReconciliationReason::GuaranteeExpired);
    assert_eq!(
        server.requests().len(),
        1,
        "the second attempt was refused before the wire"
    );
    assert_eq!(
        rec.retry.as_ref().unwrap().attempts,
        2,
        "attempts count factory starts"
    );
    // Observed: the first attempt's 503/703 is not retained in this reconciliation.
    assert!(rec.provider.is_none() && rec.meta.is_none());
}

#[tokio::test]
async fn workflow2_cancelled_call_keeps_the_binding_and_can_be_replayed() {
    let slow = Arc::new(AtomicUsize::new(1));
    let flag = slow.clone();
    let server = TestServer::start(move |r: &Recorded| {
        let reply = echo(
            r,
            200,
            r#"{"resultCode":"Authorised","pspReference":"AUTH000003"}"#,
        );
        if flag.load(Ordering::SeqCst) == 1 {
            reply.delay(Duration::from_secs(2))
        } else {
            reply
        }
    })
    .await;
    let checkout = checkout(&server, SendPolicy::default());
    let operation = checkout.details("details-cancelled", DetailsRequest::redirect("Y2FuY2Vs"));
    let cancelled =
        tokio::time::timeout(Duration::from_millis(100), operation.clone().send()).await;
    assert!(cancelled.is_err(), "the application gave up waiting");
    assert!(
        server
            .wait_until(Duration::from_secs(5), |s| s.requests().len() == 1)
            .await
    );

    let err = checkout
        .details("details-cancelled", DetailsRequest::redirect("b3RoZXI"))
        .send()
        .await
        .unwrap_err();
    assert_eq!(
        err.reconciliation().unwrap().reason,
        ReconciliationReason::ChangedRequest
    );

    slow.store(0, Ordering::SeqCst);
    let ok = operation
        .send()
        .await
        .expect("identical replay after cancellation");
    assert_eq!(ok.value.state(), PaymentState::Authorised);
    let wire = server.requests();
    assert_eq!(wire.len(), 2);
    assert_eq!(wire[0].body, wire[1].body);
}

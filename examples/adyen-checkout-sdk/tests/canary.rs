//! Secrets: API key, separate idempotency keys for methods/payments/details/capture, and
//! payment PSP reference -> sensitive headers/path. Each request's withheld string inputs
//! (merchant, currency, country, locale, merchant reference, return URL, encrypted card's four
//! test_ tokens, redirect result, flow data and capture reference) get distinct canaries.
//! Logical read (payment methods) and write (payments, details, captures) cover all seven cases;
//! this SDK has no GET. Provider models echo credentials in fields promised withheld.
//! Excludes native reqwest, *_unredacted and explicit clear field access. No wire exemptions.

use concord_check_canary::Canaries;
use concord_example_testserver::{Body, Reply, TestServer};
use std::{net::TcpListener, time::Duration};

fn http() -> concord::reqwest::Client {
    concord::reqwest::Client::builder()
        .no_proxy()
        .redirect(concord::reqwest::redirect::Policy::none())
        .retry(concord::reqwest::retry::never())
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap()
}

fn refused() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    url
}

// Echo credentials in headers and in diagnostics whose formatting is withheld.
fn reply(case: usize, success: &str, problem: &str, echo: &str) -> Reply {
    let response = match case {
        0 | 6 => Reply::json(200, success),
        1 => Reply::json(401, problem),
        2 => Reply::json(500, problem),
        3 => Reply::json(200, format!("{{invalid {echo}")),
        4 => Reply::text(200, "text/html", echo),
        5 => Reply::json(200, "{}").with_body(Body::FailAfter(vec![concord::bytes::Bytes::from(
            format!("{{\"incomplete\":\"{echo}"),
        )])),
        _ => unreachable!(),
    };
    response
        .header("x-request-id", echo)
        .header("set-cookie", echo)
}

async fn receive<T>(c: &mut Canaries, prepared: concord::Prepared<T>) -> Result<T, concord::Error> {
    let result = prepared.receive().await;
    c.assert_debug_absent("Received Result", &result);
    match result {
        Ok(received) => {
            c.assert_debug_absent("response head", &received.head());
            c.assert_debug_absent("response metadata", &received.meta());
            received.decode().await
        }
        Err(error) => Err(error),
    }
}

fn concord_error(c: &mut Canaries, error: &concord::Error) {
    c.assert_debug_absent("concord::Error", error);
    c.assert_error_absent("concord::Error and sources", error);
    if let Some(meta) = error.head_meta() {
        c.assert_debug_absent("error response metadata", &meta);
    }
}

use concord_sdk_adyen_checkout::{
    Amount, CaptureReceived, CaptureRequest, Checkout, CheckoutConfig, DetailsRequest, Error,
    MethodsRequest, MethodsResponse, PaymentRequest, PaymentResponse, SandboxMethod, SendPolicy,
    Web,
};

fn error(c: &mut Canaries, e: &Error) {
    c.assert_debug_absent("SDK error", e);
    c.assert_error_absent("SDK error and sources", e);
    match e {
        Error::Concord(e) => concord_error(c, e),
        Error::Provider(p) => {
            c.assert_debug_absent("provider failure", p);
            c.assert_debug_absent("metadata", &p.meta);
            c.assert_debug_absent("retry report", &p.retry);
            match &p.document {
                Ok(document) => {
                    c.assert_debug_absent("Problem", document);
                    c.assert_display_absent("Problem", document);
                }
                Err(e) => concord_error(c, e),
            }
        }
        Error::Reconciliation(r) => {
            c.assert_debug_absent("reconciliation evidence", r);
            c.assert_debug_absent("reconciliation metadata", &r.meta);
            c.assert_debug_absent("reconciliation retry report", &r.retry);
            if let Some(e) = &r.cause {
                concord_error(c, e);
            }
        }
        _ => {}
    }
}

fn methods_models(c: &mut Canaries, value: &MethodsResponse) {
    c.assert_debug_absent("available method models", &value.payment_methods);
}

fn payment_models(c: &mut Canaries, value: &PaymentResponse) {
    c.assert_debug_absent("action model", &value.action);
}

fn capture_models(c: &mut Canaries, value: &CaptureReceived) {
    c.assert_debug_absent("capture amount", &value.requested);
}

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("adyen-checkout");
    let api_key = c.issue();
    let methods_key = c.issue();
    let payments_key = c.issue();
    let details_key = c.issue();
    let capture_key = c.issue();
    let psp = c.issue();
    let methods = MethodsRequest {
        merchant_account: c.issue(),
        amount: Amount {
            currency: c.issue(),
            value: 100,
        },
        country_code: Some(c.issue()),
        shopper_locale: Some(c.issue()),
        channel: Web::Web,
    };
    c.assert_debug_absent("request amount", &methods.amount);
    let card = SandboxMethod::test_card(
        &format!("test_{}", c.issue()),
        &format!("test_{}", c.issue()),
        &format!("test_{}", c.issue()),
        &format!("test_{}", c.issue()),
    )
    .unwrap();
    c.assert_debug_absent("sandbox card", &card);
    if let SandboxMethod::TestCard {
        encrypted_card_number,
        encrypted_expiry_month,
        encrypted_expiry_year,
        encrypted_security_code,
    } = &card
    {
        for token in [
            encrypted_card_number,
            encrypted_expiry_month,
            encrypted_expiry_year,
            encrypted_security_code,
        ] {
            c.assert_debug_absent("TestToken", token);
        }
    }
    let payment = PaymentRequest {
        merchant_account: c.issue(),
        amount: Amount {
            currency: c.issue(),
            value: 100,
        },
        reference: c.issue(),
        return_url: format!("https://unused.invalid/{}", c.issue()),
        payment_method: card,
        channel: Web::Web,
    };
    let mut details = DetailsRequest::redirect(c.issue());
    details.payment_data = Some(c.issue());
    let capture = CaptureRequest {
        merchant_account: c.issue(),
        amount: Amount {
            currency: c.issue(),
            value: 100,
        },
        reference: Some(c.issue()),
    };
    let ideal_key = c.issue();
    let ideal = SandboxMethod::Ideal { issuer: c.issue() };
    c.assert_debug_absent("iDEAL method", &ideal);
    for case in 0..7 {
        let echo = api_key.clone();
        let server = TestServer::start(move |r: &concord_example_testserver::Recorded| {
            let success = if r.path().ends_with("paymentMethods") {
                serde_json::json!({"paymentMethods":[{"type":echo,"name":echo,"extra":echo}]})
            } else if r.path().ends_with("captures") {
                let request: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
                let psp = r.path().split('/').rev().nth(1).unwrap().to_owned();
                serde_json::json!({"status":"received","pspReference":echo,"paymentPspReference":psp,
                    "merchantAccount":request["merchantAccount"], "amount":request["amount"], "reference":request["reference"]})
            } else {
                serde_json::json!({"resultCode":"RedirectShopper","pspReference":echo,"refusalReason":echo,
                    "action":{"type":"redirect","url":echo,"paymentData":echo},"extra":echo})
            };
            let problem = serde_json::json!({"status":401,"errorCode":"000","errorType":"security","message":echo,"pspReference":echo});
            reply(case, &success.to_string(), &problem.to_string(), &echo)
                .header("idempotency-key", r.header("idempotency-key").unwrap())
        }).await;
        let policy = SendPolicy::default();
        c.assert_debug_absent("policy", &policy);
        let config = CheckoutConfig {
            api_key: api_key.clone(),
            policy: policy.clone(),
        };
        c.assert_debug_absent("config", &config);
        let api = Checkout::new(http(), config)
            .with_base_url(if case == 6 { refused() } else { server.url() })
            .unwrap();
        let scope = api.v71();
        let payment_scope = scope.payment(&psp);
        c.assert_debug_absent("client", &api);
        c.assert_debug_absent("version scope", &scope);
        c.assert_debug_absent("payment scope", &payment_scope);
        c.assert_debug_absent("methods request", &methods);
        c.assert_debug_absent("payment request", &payment);
        c.assert_debug_absent("details request", &details);
        c.assert_debug_absent("capture request", &capture);
        let read = scope.payment_methods(&methods_key, methods.clone());
        let write = scope.payments(&payments_key, payment.clone());
        let continuation = scope.details(&details_key, details.clone());
        let capture_op = payment_scope.captures(&capture_key, capture.clone());
        c.assert_debug_absent("methods endpoint", &read);
        c.assert_debug_absent("payments endpoint", &write);
        c.assert_debug_absent("details endpoint", &continuation);
        c.assert_debug_absent("capture endpoint", &capture_op);
        c.assert_debug_absent("methods Prepared", &read.clone().prepare().unwrap());
        c.assert_debug_absent("payments Prepared", &write.clone().prepare().unwrap());
        c.assert_debug_absent("details Prepared", &continuation.clone().prepare().unwrap());
        c.assert_debug_absent("capture Prepared", &capture_op.clone().prepare().unwrap());
        let native = receive(&mut c, read.clone().prepare().unwrap()).await;
        c.assert_debug_absent("native Result", &native);
        if let Err(e) = &native {
            concord_error(&mut c, e);
        }
        // Check each public result wrapper without changing its concrete value type.
        macro_rules! check {
            ($op:expr, $inspect:expr) => {{
                let result = $op.send().await;
                c.assert_debug_absent("SDK Result", &result);
                assert_eq!(result.is_ok(), case == 0);
                match result {
                    Ok(response) => {
                        c.assert_debug_absent("response", &response);
                        c.assert_debug_absent("provider value", &response.value);
                        $inspect(&mut c, &response.value);
                        c.assert_debug_absent("response metadata", &response.meta);
                        c.assert_debug_absent("retry report", &response.retry);
                    }
                    Err(e) => error(&mut c, &e),
                }
            }};
        }
        check!(read, methods_models);
        check!(write, payment_models);
        check!(continuation, payment_models);
        check!(capture_op, capture_models);
        c.assert_debug_absent("populated policy ledger", &policy);
        if case == 0 {
            let mut ideal_payment = payment.clone();
            ideal_payment.payment_method = ideal.clone();
            check!(scope.payments(&ideal_key, ideal_payment), payment_models);
            c.assert_on_wire(&server.requests());
        }
        server.shutdown().await;
    }
    assert!(c.checked() >= 300);
    println!(
        "adyen-checkout: {} renderings; positive control passed",
        c.checked()
    );
}

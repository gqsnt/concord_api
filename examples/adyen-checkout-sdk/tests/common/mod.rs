#![allow(dead_code)]
use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_adyen_checkout::{
    Amount, Checkout, DetailsRequest, MethodsRequest, PaymentRequest, SandboxMethod, SendPolicy,
    Web,
};

pub const METHODS: &str = include_str!("../../fixtures/methods-response.json");
pub const AUTHORISED: &str = include_str!("../../fixtures/authorised.json");
pub const ACTION: &str = include_str!("../../fixtures/action.json");
pub const NEXT_ACTION: &str = include_str!("../../fixtures/next-action.json");
pub const PROBLEM: &str = include_str!("../../fixtures/error.json");
pub const UNAVAILABLE: &str = include_str!("../../fixtures/unavailable.json");
pub const IN_PROGRESS: &str = include_str!("../../fixtures/in-progress.json");

pub fn api(server: &TestServer, policy: SendPolicy) -> Checkout {
    Checkout::sandbox("synthetic-api-secret", policy)
        .unwrap()
        .with_base_url(server.url())
        .unwrap()
}
pub fn amount() -> Amount {
    Amount {
        currency: "EUR".into(),
        value: 1000,
    }
}
pub fn methods() -> MethodsRequest {
    MethodsRequest {
        merchant_account: "TestMerchant".into(),
        amount: amount(),
        country_code: Some("NL".into()),
        shopper_locale: Some("nl-NL".into()),
        channel: Web::Web,
    }
}
pub fn payment() -> PaymentRequest {
    PaymentRequest {
        merchant_account: "TestMerchant".into(),
        amount: amount(),
        reference: "order-sdk31".into(),
        return_url: "https://merchant.example/return".into(),
        payment_method: SandboxMethod::Ideal {
            issuer: "1121".into(),
        },
        channel: Web::Web,
    }
}
pub fn details() -> DetailsRequest {
    let mut request = DetailsRequest::redirect("synthetic-redirect-secret");
    request.payment_data = Some("synthetic-flow-secret".into());
    request
}
pub fn success(request: &Recorded, body: &str) -> Reply {
    Reply::json(200, body).header(
        "Idempotency-Key",
        request.header("idempotency-key").unwrap(),
    )
}
pub fn fixture_json(s: &str) -> serde_json::Value {
    serde_json::from_str(s).unwrap()
}
pub fn assert_body(request: &Recorded, fixture: &str) {
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&request.body).unwrap(),
        fixture_json(fixture)
    );
}
pub fn assert_hidden(shown: &str) {
    for secret in [
        "synthetic-api-secret",
        "synthetic-key-secret",
        "synthetic-provider-secret",
        "synthetic-reference-secret",
        "synthetic-body-secret",
        "synthetic-flow-secret",
        "synthetic-url-secret",
        "synthetic-redirect-secret",
        "synthetic-challenge-secret",
        "synthetic-refusal-secret",
        "test_4111111111111111",
        "test_737",
    ] {
        assert!(!shown.contains(secret), "secret leaked: {shown}");
    }
}

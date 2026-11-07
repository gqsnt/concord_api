use std::time::Duration;

use concord_example_testserver::{Recorded, TestServer};
use concord_sdk_square_payments::{CreatePayment, Money, Square, SquareConfig, bearer, transport};

pub const REQUEST: &str = include_str!("../../fixtures/create-request.json");
pub const PAYMENT: &str = include_str!("../../fixtures/payment.json");
pub const PAGE_1: &str = include_str!("../../fixtures/page-1.json");
pub const PAGE_2: &str = include_str!("../../fixtures/page-2.json");
pub const MISMATCH: &str = include_str!("../../fixtures/mismatch.json");
pub const CURSOR: &str = "opaque+/= &?%cursor";
pub const TOKEN: &str = "synthetic-bearer-secret";

pub fn client(server: &TestServer) -> Square {
    Square::new(
        transport(Duration::from_secs(2)).unwrap(),
        SquareConfig {
            authorization: bearer(TOKEN).unwrap(),
        },
    )
    .with_base_url(server.url())
    .unwrap()
}

pub fn create(amount: i64) -> CreatePayment {
    CreatePayment::new(
        "caller-key-001",
        "cnon:synthetic-payment-token",
        Money {
            amount,
            currency: "USD".into(),
        },
    )
    .unwrap()
}

pub fn headers(request: &Recorded) {
    assert_eq!(
        request.header_all("authorization"),
        [format!("Bearer {TOKEN}")]
    );
    assert_eq!(request.header_all("square-version"), ["2026-09-16"]);
    assert_eq!(request.header_all("accept"), ["application/json"]);
}

//! Shared test support: an offline client, JSON replies and the wire assertions every request
//! of this SDK must satisfy. Each test crate uses a subset.
#![allow(dead_code)]

use std::time::Duration;

use concord::reqwest;
use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_rest_countries::{RestCountries, RestCountriesConfig, bearer};

/// A synthetic API key (never a real credential).
pub const KEY: &str = "rc_test_synthetic_7f3a91";

/// A fixture's bytes, embedded at compile time.
#[macro_export]
macro_rules! fixture {
    ($name:literal) => {
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/", $name)).as_slice()
    };
}

pub fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(10))
        .user_agent("concord-sdk-rest-countries-offline-test")
        .build()
        .unwrap()
}

pub fn client(server: &TestServer) -> RestCountries {
    let config = RestCountriesConfig {
        authorization: bearer(KEY).unwrap(),
    };
    RestCountries::new(http(), config)
        .with_base_url(server.url())
        .unwrap()
}

/// A JSON reply labelled as the provider documents ("Content-Type: application/json on every
/// response").
pub fn json(status: u16, body: impl AsRef<[u8]>) -> Reply {
    Reply::new(status)
        .header("content-type", "application/json")
        .header("x-fixture", "rest-countries-offline")
        .body(body)
}

/// The message of every entry of an error fixture, read from the fixture itself.
pub fn fixture_messages(body: &[u8]) -> Vec<String> {
    let document: serde_json::Value = serde_json::from_slice(body).unwrap();
    document["errors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["message"].as_str().unwrap().to_owned())
        .collect()
}

/// Every request of this SDK: `GET`, the given raw target, the bearer key exactly once, the
/// JSON `Accept`, no body, no cookie and no query credential.
pub fn assert_get(request: &Recorded, target: &str) {
    assert_eq!(request.method, "GET");
    assert_eq!(request.target, target);
    assert_eq!(
        request.header_all("authorization"),
        [format!("Bearer {KEY}")]
    );
    assert_eq!(request.header_all("accept"), ["application/json"]);
    assert_eq!(request.header("content-type"), None);
    assert_eq!(request.header("cookie"), None);
    assert_eq!(
        request.header("user-agent"),
        Some("concord-sdk-rest-countries-offline-test")
    );
    assert!(request.query_all("api-key").is_empty());
    assert!(request.body.is_empty());
    assert_eq!(request.body_len, 0);
    assert!(!request.chunked);
}

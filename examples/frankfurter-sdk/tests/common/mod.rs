use std::time::Duration;

use concord::reqwest;
use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_frankfurter::Frankfurter;

pub const LATEST: &[u8] = include_bytes!("../../fixtures/latest.json");
pub const HISTORICAL: &[u8] = include_bytes!("../../fixtures/historical.json");

pub fn client(server: &TestServer) -> Frankfurter {
    let http = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(3))
        .user_agent("concord-sdk-frankfurter-offline-test")
        .build()
        .unwrap();
    Frankfurter::new(http).with_base_url(server.url()).unwrap()
}

pub fn json(status: u16, body: impl AsRef<[u8]>) -> Reply {
    Reply::new(status)
        .header("content-type", "application/json")
        .header("x-fixture-id", "frankfurter-offline")
        .body(body)
}

pub fn assert_wire(request: &Recorded, target: &str) {
    assert_eq!(request.method, "GET");
    assert_eq!(request.target, target);
    assert_eq!(request.header_all("accept"), ["application/json"]);
    assert_eq!(request.header("content-type"), None);
    assert_eq!(request.header("authorization"), None);
    assert_eq!(request.header("cookie"), None);
    assert_eq!(
        request.header("user-agent"),
        Some("concord-sdk-frankfurter-offline-test")
    );
    assert_eq!(request.header("host"), Some(request.host.as_str()));
    assert_eq!(request.body.as_slice(), b"");
    assert_eq!(request.body_len, 0);
    assert!(!request.chunked);
}

use concord::reqwest;
use concord_example_testserver::{Recorded, TestServer};
use concord_sdk_nager_holidays::NagerHolidays;

pub const AUSTRIA: &[u8] = include_bytes!("../../fixtures/holidays-at-2026.json");
pub const GERMANY: &[u8] = include_bytes!("../../fixtures/holidays-de-2027-synthetic.json");
pub const PERTURBED: &[u8] = include_bytes!("../../fixtures/holidays-at-2026-perturbed.json");
pub const INVALID: &[u8] = include_bytes!("../../fixtures/invalid-request-synthetic.txt");

pub fn client(server: &TestServer) -> NagerHolidays {
    let http = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .unwrap();
    NagerHolidays::new(http)
        .with_base_url(server.url())
        .unwrap()
}

pub fn assert_read(request: &Recorded, server: &TestServer, target: &str) {
    assert_eq!(request.method, "GET");
    assert_eq!(request.target, target);
    assert_eq!(request.header_all("accept"), ["application/json"]);
    assert_eq!(request.header("authorization"), None);
    assert_eq!(request.header("content-type"), None);
    assert_eq!(request.header("transfer-encoding"), None);
    assert_eq!(request.host, server.addr().to_string());
    assert_eq!(request.body, b"");
    assert_eq!(request.body_len, 0);
    assert!(!request.chunked);
}

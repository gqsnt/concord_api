//! Pilot control oracle: one holiday request against a local receiver (public API only).
use concord_example_testserver::{Reply, TestServer};
use concord_sdk_nager_holidays::NagerHolidays;

#[tokio::test]
async fn holidays_request_and_result() {
    let server = TestServer::start(|_| Reply::json(200, "[]")).await;
    let api = NagerHolidays::new(concord::reqwest::Client::new())
        .with_base_url(server.url())
        .unwrap();
    let response = api.holidays("AT", 2026).send().await.unwrap();
    assert!(response.holidays.is_empty());
    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].target, "/api/v4/Holidays/AT/2026");
}

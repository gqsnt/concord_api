//! An application consuming the SDK against an independently scripted loopback provider.
use std::time::Duration;

use concord::reqwest::{self, StatusCode};
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use concord_sdk_nager_holidays::{ErrorKind, HolidayType, NagerHolidays};

const AT: &[u8] = include_bytes!("../fixtures/first-use-at.json");
const DE: &[u8] = include_bytes!("../fixtures/first-use-de-synthetic.json");
const REJECTION: &[u8] = include_bytes!("../fixtures/first-use-invalid-request.txt");
const TRUNCATED: &[u8] = include_bytes!("../fixtures/first-use-truncated.json");
const EMPTY: &[u8] = include_bytes!("../fixtures/first-use-empty.json");
const OUTAGE: &[u8] = include_bytes!("../fixtures/first-use-outage.txt");

fn client(server: &TestServer) -> NagerHolidays {
    let http = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    NagerHolidays::new(http)
        .with_base_url(server.url())
        .unwrap()
}

fn assert_request(request: &Recorded, target: &str) {
    assert_eq!(request.method, "GET");
    assert_eq!(request.target, target);
    assert_eq!(request.header("accept"), Some("application/json"));
    assert_eq!(request.header("authorization"), None);
    assert_eq!(request.header("content-type"), None);
    assert_eq!(request.body_len, 0);
    assert!(request.body.is_empty());
    assert!(!request.chunked);
}

#[tokio::test]
async fn reads_a_country_and_year() {
    let server = TestServer::start(|_| Reply::json(200, AT).header("x-example", "first")).await;
    let response = client(&server).holidays("AT", 2026).fetch().await.unwrap();
    assert_eq!(response.meta.status, StatusCode::OK);
    assert_eq!(response.meta.headers["x-example"], "first");
    assert_eq!(response.holidays.len(), 2);
    let first = &response.holidays[0];
    assert_eq!(first.date.to_string(), "2026-01-01");
    assert_eq!(first.name, "New Year's Day");
    assert_eq!(first.country_code, "AT");
    assert!(first.national_holiday);
    assert_eq!(first.subdivision_codes, None);
    assert_eq!(first.holiday_types, vec![HolidayType::Public]);
    assert_eq!(response.holidays[1].name, "Epiphany");
    assert_request(&server.single(), "/api/v4/Holidays/AT/2026");
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn filters_another_country_locally() {
    let server = TestServer::start(|_| Reply::json(200, DE)).await;
    let response = client(&server).holidays("DE", 2027).fetch().await.unwrap();
    let bank: Vec<_> = response
        .holidays
        .iter()
        .filter(|h| h.holiday_types.contains(&HolidayType::Bank))
        .map(|h| h.name.as_str())
        .collect();
    let public: Vec<_> = response
        .holidays
        .iter()
        .filter(|h| h.holiday_types.contains(&HolidayType::Public))
        .map(|h| h.name.as_str())
        .collect();
    let national_public: Vec<_> = response
        .holidays
        .iter()
        .filter(|h| h.national_holiday && h.holiday_types.contains(&HolidayType::Public))
        .map(|h| h.name.as_str())
        .collect();
    assert_eq!(bank, ["Synthetic national day", "Synthetic regional day"]);
    assert_eq!(public, ["Synthetic national day"]);
    assert_eq!(national_public, ["Synthetic national day"]);
    assert_eq!(
        response.holidays[1].subdivision_codes.as_deref(),
        Some(["DE-BY".to_owned()].as_slice())
    );
    assert_eq!(response.holidays[2].subdivision_codes, Some(vec![]));
    assert!(
        response.holidays[2]
            .holiday_types
            .contains(&HolidayType::Other("FutureKind".into()))
    );
    assert_request(&server.single(), "/api/v4/Holidays/DE/2027");
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn an_empty_array_is_a_completed_read() {
    let server = TestServer::start(|_| Reply::json(200, EMPTY)).await;
    let response = client(&server).holidays("AT", 2026).fetch().await.unwrap();
    assert_eq!(response.meta.status, StatusCode::OK);
    assert!(response.holidays.is_empty());
    assert_request(&server.single(), "/api/v4/Holidays/AT/2026");
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn invalid_request_retains_the_unread_error_body() {
    let server = TestServer::start(|_| Reply::text(400, "text/plain", REJECTION)).await;
    let error = client(&server).holidays("XX", 0).fetch().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidRequest);
    assert_eq!(error.status(), Some(StatusCode::BAD_REQUEST));
    assert_eq!(error.meta().unwrap().status, StatusCode::BAD_REQUEST);
    let response = error.into_concord().into_response().unwrap();
    assert_eq!(response.bytes().await.unwrap().as_ref(), REJECTION);
    assert_request(&server.single(), "/api/v4/Holidays/XX/0");
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn malformed_json_returns_an_error_not_a_partial_result() {
    let server = TestServer::start(|_| Reply::json(200, TRUNCATED)).await;
    let error = client(&server)
        .holidays("AT", 2026)
        .fetch()
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Concord(concord::ErrorKind::Decode));
    assert_eq!(error.status(), Some(StatusCode::OK));
    assert_request(&server.single(), "/api/v4/Holidays/AT/2026");
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn interrupted_response_returns_an_error_not_a_partial_result() {
    let server =
        TestServer::start(|_| Reply::json(200, b"").with_body(Body::FailAfter(vec![AT.into()])))
            .await;
    let error = client(&server)
        .holidays("AT", 2026)
        .fetch()
        .await
        .unwrap_err();
    assert!(matches!(error.kind(), ErrorKind::Concord(_)));
    assert_eq!(error.status(), Some(StatusCode::OK));
    assert_request(&server.single(), "/api/v4/Holidays/AT/2026");
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn failed_second_read_keeps_the_first_completed_result() {
    let server = TestServer::start(|request| match request.target.as_str() {
        "/api/v4/Holidays/AT/2026" => Reply::json(200, AT),
        "/api/v4/Holidays/DE/2027" => Reply::text(503, "text/plain", OUTAGE),
        _ => Reply::new(404),
    })
    .await;
    let api = client(&server);
    let first = api.holidays("AT", 2026).fetch().await.unwrap();
    let error = api.holidays("DE", 2027).fetch().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Http);
    assert_eq!(error.status(), Some(StatusCode::SERVICE_UNAVAILABLE));
    assert_eq!(first.holidays.len(), 2);
    assert_eq!(first.holidays[0].name, "New Year's Day");
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_request(&requests[0], "/api/v4/Holidays/AT/2026");
    assert_request(&requests[1], "/api/v4/Holidays/DE/2027");
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn wrong_media_type_is_not_mistaken_for_an_empty_read() {
    let server = TestServer::start(|_| Reply::text(200, "text/plain", EMPTY)).await;
    let error = client(&server)
        .holidays("AT", 2026)
        .fetch()
        .await
        .unwrap_err();
    assert_eq!(
        error.kind(),
        ErrorKind::Concord(concord::ErrorKind::Metadata)
    );
    assert_eq!(error.status(), Some(StatusCode::OK));
    assert_request(&server.single(), "/api/v4/Holidays/AT/2026");
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn failure_before_a_response_has_no_status() {
    let server = TestServer::start(|_| Reply::hang_up()).await;
    let error = client(&server)
        .holidays("AT", 2026)
        .fetch()
        .await
        .unwrap_err();
    assert_eq!(
        error.kind(),
        ErrorKind::Concord(concord::ErrorKind::Transport)
    );
    assert!(error.status().is_none());
    assert!(error.meta().is_none());
    assert_request(&server.single(), "/api/v4/Holidays/AT/2026");
    assert_eq!(server.shutdown().await.open_after, 0);
}

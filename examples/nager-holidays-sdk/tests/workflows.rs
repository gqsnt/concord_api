//! Complete country/year reads and local filters through the public SDK.

mod support;

use concord_example_testserver::{Reply, Script, TestServer};
use concord_sdk_nager_holidays::{ErrorKind, HolidayType};
use support::{AUSTRIA, GERMANY, INVALID, PERTURBED, assert_read, client};

#[tokio::test]
async fn workflow_a_retrieves_one_country_year_and_retains_known_fields_after_perturbation() {
    let script = Script::new([Reply::json(200, AUSTRIA), Reply::json(200, PERTURBED)]);
    let server = TestServer::start(move |_| script.next()).await;
    let api = client(&server);
    let original = api.holidays("AT", 2026).fetch().await.unwrap();
    let names: Vec<_> = original.holidays.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(names, ["New Year's Day", "Epiphany"]);
    assert!(original.holidays.iter().all(|h| h.national_holiday));
    let perturbed = api.holidays("AT", 2026).fetch().await.unwrap();
    assert_eq!(perturbed.holidays, original.holidays);
    for request in server.requests() {
        assert_read(&request, &server, "/api/v4/Holidays/AT/2026");
    }
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn workflow_b_retrieves_different_country_year_and_filters_locally() {
    let script = Script::new([Reply::json(200, AUSTRIA), Reply::json(200, GERMANY)]);
    let server = TestServer::start(move |_| script.next()).await;
    let api = client(&server);
    let austria = api.holidays("AT", 2026).fetch().await.unwrap();
    let germany = api.holidays("DE", 2027).fetch().await.unwrap();
    assert!(austria.holidays.iter().all(|h| h.country_code == "AT"));
    assert!(germany.holidays.iter().all(|h| h.country_code == "DE"));
    let national: Vec<_> = germany
        .holidays
        .iter()
        .filter(|h| h.national_holiday)
        .collect();
    assert_eq!(national.len(), 1);
    assert_eq!(national[0].name, "Synthetic national public day");
    let bank: Vec<_> = germany
        .holidays
        .iter()
        .filter(|h| h.holiday_types.contains(&HolidayType::Bank))
        .collect();
    assert_eq!(bank.len(), 1);
    assert_eq!(bank[0].name, "Synthetic regional bank day");
    assert_eq!(bank[0].subdivision_codes.as_deref().unwrap(), ["DE-BY"]);
    let national_public: Vec<_> = germany
        .holidays
        .iter()
        .filter(|h| h.national_holiday && h.holiday_types.contains(&HolidayType::Public))
        .collect();
    assert_eq!(national_public, national);
    let requests = server.requests();
    assert_eq!(requests.len(), 2, "filtering sends no additional requests");
    assert_read(&requests[0], &server, "/api/v4/Holidays/AT/2026");
    assert_read(&requests[1], &server, "/api/v4/Holidays/DE/2027");
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn failed_second_read_keeps_the_completed_first_workflow_result() {
    let script = Script::new([
        Reply::json(200, AUSTRIA),
        Reply::text(400, "text/plain", INVALID),
    ]);
    let server = TestServer::start(move |_| script.next()).await;
    let api = client(&server);
    let first = api.holidays("AT", 2026).fetch().await.unwrap();
    let second = api.holidays("XX", 0).fetch().await.unwrap_err();
    assert_eq!(second.kind(), ErrorKind::InvalidRequest);
    assert_eq!(
        first.holidays.len(),
        2,
        "earlier completed calls remain owned by the caller"
    );
    assert_eq!(
        second
            .into_concord()
            .into_response()
            .unwrap()
            .bytes()
            .await
            .unwrap()
            .as_ref(),
        INVALID
    );
    assert_eq!(server.requests().len(), 2);
    assert_eq!(server.shutdown().await.open_after, 0);
}

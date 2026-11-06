//! Offline wire, response-shape and failure checks for the public SDK.

mod support;

use concord::ErrorKind as ConcordErrorKind;
use concord::bytes::Bytes;
use concord::reqwest::{StatusCode, Version};
use concord_example_testserver::{Body, Reply, Script, TestServer};
use concord_sdk_nager_holidays::{API_VERSION, Date, ErrorKind, HolidayType, NagerHolidays};
use support::{AUSTRIA, GERMANY, INVALID, PERTURBED, assert_read, client};

#[tokio::test]
async fn v4_wire_and_documented_array() {
    let server = TestServer::start(|_| {
        Reply::json(200, AUSTRIA)
            .header("etag", "\"synthetic-fixture\"")
            .header("x-synthetic-request-id", "synthetic-read-1")
            .header("x-synthetic-multi", "one")
            .header("x-synthetic-multi", "two")
    })
    .await;
    let response = client(&server).holidays("AT", 2026).fetch().await.unwrap();
    assert_eq!(API_VERSION, "v4");
    assert_eq!(response.holidays.len(), 2);
    let first = &response.holidays[0];
    assert_eq!(first.date, Date::from_ymd_opt(2026, 1, 1).unwrap());
    assert_eq!(first.name, "New Year's Day");
    assert_eq!(first.country_code, "AT");
    assert!(first.national_holiday);
    assert_eq!(first.subdivision_codes, None);
    assert_eq!(first.holiday_types, [HolidayType::Public]);
    assert_eq!(response.holidays[1].name, "Epiphany");
    assert_eq!(response.meta.status, StatusCode::OK);
    assert_eq!(response.meta.version, Version::HTTP_11);
    assert_eq!(response.meta.headers["etag"], "\"synthetic-fixture\"");
    assert_eq!(
        response
            .meta
            .headers
            .get_all("x-synthetic-multi")
            .iter()
            .count(),
        2
    );
    assert_read(&server.single(), &server, "/api/v4/Holidays/AT/2026");
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn country_is_one_encoded_segment_and_synthetic_400_keeps_exact_body() {
    let server = TestServer::start(|_| {
        Reply::text(400, "text/plain", INVALID)
            .header("x-synthetic-request-id", "synthetic-invalid-1")
    })
    .await;
    let error = client(&server)
        .holidays("A/T ?#%é", 0)
        .fetch()
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidRequest);
    assert_eq!(error.status(), Some(StatusCode::BAD_REQUEST));
    assert_eq!(error.concord().kind(), ConcordErrorKind::Status);
    assert_eq!(error.concord().endpoint(), Some("NagerHolidays.holidays"));
    assert_eq!(error.concord().operation().unwrap().method(), Some("GET"));
    assert_eq!(
        error.meta().unwrap().headers["x-synthetic-request-id"],
        "synthetic-invalid-1"
    );
    assert!(std::error::Error::source(&error).is_some());
    let shown = format!("{error} {error:?}");
    assert!(!shown.contains("A/T"));
    assert!(!shown.contains("synthetic-invalid-1"));
    assert!(!shown.contains("Synthetic invalid request"));
    let response = error.into_concord().into_response().unwrap();
    assert_eq!(response.bytes().await.unwrap().as_ref(), INVALID);
    assert_read(
        &server.single(),
        &server,
        "/api/v4/Holidays/A%2FT%20%3F%23%25%C3%A9/0",
    );
    server.shutdown().await;
}

#[tokio::test]
async fn optional_unknown_fields_and_null_subdivisions_preserve_known_fields() {
    let script = Script::new([
        Reply::json(200, AUSTRIA),
        Reply::json(200, PERTURBED),
        Reply::json(200, PERTURBED),
    ]);
    let server = TestServer::start(move |_| script.next()).await;
    let api = client(&server);
    let original = api.holidays("AT", 2026).fetch().await.unwrap();
    let perturbed = api.holidays("AT", 2026).fetch().await.unwrap();
    assert_eq!(perturbed.holidays, original.holidays);
    assert!(
        perturbed
            .holidays
            .iter()
            .all(|h| h.subdivision_codes.is_none())
    );
    let sent = api.holidays("AT", 2026).send().await.unwrap();
    assert_eq!(sent, original.holidays);
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    for request in requests {
        assert_read(&request, &server, "/api/v4/Holidays/AT/2026");
    }
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn new_null_subdivision_and_unknown_field_preserve_other_known_fields() {
    let script = Script::new([
        Reply::json(200, GERMANY),
        Reply::json(
            200,
            include_bytes!("../fixtures/holidays-de-2027-perturbed-synthetic.json"),
        )
        .header("x-synthetic-request-id", "synthetic-maintenance"),
    ]);
    let server = TestServer::start(move |_| script.next()).await;
    let api = client(&server);
    let original = api.holidays("DE", 2027).fetch().await.unwrap();
    let perturbed = api.holidays("DE", 2027).fetch().await.unwrap();

    let mut expected = original.holidays;
    assert_eq!(expected[1].subdivision_codes.as_deref().unwrap(), ["DE-BY"]);
    // Only this known value changed on the wire. All other fields and entries,
    // including nationalHoliday=false and the populated/empty/omitted lists, survive.
    expected[1].subdivision_codes = None;
    assert_eq!(perturbed.holidays, expected);
    assert!(!perturbed.holidays[1].national_holiday);
    assert_eq!(perturbed.meta.status, StatusCode::OK);
    assert_eq!(
        perturbed.meta.headers["x-synthetic-request-id"],
        "synthetic-maintenance"
    );
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    for request in requests {
        assert_read(&request, &server, "/api/v4/Holidays/DE/2027");
    }
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn all_documented_types_and_nullable_empty_or_missing_subdivisions() {
    let server = TestServer::start(|_| Reply::json(200, GERMANY)).await;
    let response = client(&server).holidays("DE", 2027).fetch().await.unwrap();
    let holidays = response.holidays;
    assert_eq!(holidays[0].subdivision_codes, None);
    assert_eq!(holidays[1].subdivision_codes.as_deref().unwrap(), ["DE-BY"]);
    assert_eq!(
        holidays[2].subdivision_codes.as_deref().unwrap(),
        ["DE-BY", "DE-BE"]
    );
    assert_eq!(holidays[3].subdivision_codes, Some(vec![]));
    assert_eq!(holidays[4].subdivision_codes, None);
    assert_eq!(
        holidays
            .iter()
            .flat_map(|h| h.holiday_types.clone())
            .collect::<Vec<_>>(),
        [
            HolidayType::Public,
            HolidayType::Bank,
            HolidayType::School,
            HolidayType::Authorities,
            HolidayType::Optional,
            HolidayType::Observance,
        ]
    );
    assert_read(&server.single(), &server, "/api/v4/Holidays/DE/2027");
    server.shutdown().await;
}

#[tokio::test]
async fn future_type_is_retained_and_unicode_and_leap_day_decode() {
    let server = TestServer::start(|_| {
        Reply::json(
            200,
            include_bytes!("../fixtures/holidays-future-type-synthetic.json"),
        )
    })
    .await;
    let response = client(&server).holidays("AT", 2028).fetch().await.unwrap();
    assert_eq!(
        response.holidays[0].date,
        Date::from_ymd_opt(2028, 2, 29).unwrap()
    );
    assert_eq!(
        response.holidays[0].name,
        "Synthetic future classification – fête"
    );
    assert_eq!(
        response.holidays[0].holiday_types,
        [
            HolidayType::Public,
            HolidayType::Other("SyntheticFutureType".into()),
        ]
    );
    assert_eq!(response.holidays[0].subdivision_codes, None);
    server.shutdown().await;
}

#[tokio::test]
async fn empty_array_is_complete() {
    let server =
        TestServer::start(|_| Reply::json(200, include_bytes!("../fixtures/empty-array.json")))
            .await;
    let response = client(&server).holidays("AT", 2026).fetch().await.unwrap();
    assert!(response.holidays.is_empty());
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

#[tokio::test]
async fn invalid_dates_missing_fields_v3_wrong_root_and_truncation_are_decode_failures() {
    let script = Script::new([
        Reply::json(
            200,
            include_bytes!("../fixtures/invalid-date-synthetic.json"),
        ),
        Reply::json(
            200,
            include_bytes!("../fixtures/missing-required-field-synthetic.json"),
        ),
        Reply::json(200, include_bytes!("../fixtures/v3-shape-synthetic.json")),
        Reply::json(200, include_bytes!("../fixtures/wrong-root-synthetic.json")),
        Reply::json(
            200,
            include_bytes!("../fixtures/truncated-array-synthetic.json"),
        ),
    ]);
    let server = TestServer::start(move |_| {
        script
            .next()
            .header("x-synthetic-request-id", "synthetic-decode")
    })
    .await;
    let api = client(&server);
    for _ in 0..5 {
        let error = api.holidays("AT", 2026).fetch().await.unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Concord(ConcordErrorKind::Decode));
        assert_eq!(error.status(), Some(StatusCode::OK));
        assert_eq!(
            error.meta().unwrap().headers["x-synthetic-request-id"],
            "synthetic-decode"
        );
        assert_eq!(error.concord().endpoint(), Some("NagerHolidays.holidays"));
        assert!(error.concord().codec_error().is_some());
        assert!(error.concord().response().is_none());
    }
    assert_eq!(server.requests().len(), 5);
    server.shutdown().await;
}

#[tokio::test]
async fn wrong_or_missing_media_type_is_refused_before_body_decode() {
    let script = Script::new([
        Reply::text(
            200,
            "text/html",
            include_bytes!("../fixtures/proxy-error-synthetic.html"),
        ),
        Reply::new(200).body(AUSTRIA),
    ]);
    let server = TestServer::start(move |_| script.next()).await;
    let api = client(&server);
    for expected in [
        include_bytes!("../fixtures/proxy-error-synthetic.html").as_slice(),
        AUSTRIA,
    ] {
        let error = api.holidays("AT", 2026).fetch().await.unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Concord(ConcordErrorKind::Metadata));
        assert_eq!(error.status(), Some(StatusCode::OK));
        assert_eq!(
            error
                .into_concord()
                .into_response()
                .unwrap()
                .bytes()
                .await
                .unwrap()
                .as_ref(),
            expected
        );
    }
    server.shutdown().await;
}

#[tokio::test]
async fn other_status_keeps_headers_and_unread_non_json_body() {
    let server = TestServer::start(|_| {
        Reply::text(
            503,
            "text/html",
            include_bytes!("../fixtures/proxy-error-synthetic.html"),
        )
        .header("retry-after", "5")
    })
    .await;
    let error = client(&server)
        .holidays("AT", 2026)
        .fetch()
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Http);
    assert_eq!(error.status(), Some(StatusCode::SERVICE_UNAVAILABLE));
    assert_eq!(error.meta().unwrap().headers["retry-after"], "5");
    assert_eq!(
        error
            .into_concord()
            .into_response()
            .unwrap()
            .bytes()
            .await
            .unwrap()
            .as_ref(),
        include_bytes!("../fixtures/proxy-error-synthetic.html")
    );
    assert_eq!(server.requests().len(), 1, "no retry");
    server.shutdown().await;
}

#[tokio::test]
async fn dropped_connection_before_head_has_no_fabricated_status() {
    let server = TestServer::start(|_| Reply::hang_up()).await;
    let error = client(&server)
        .holidays("AT", 2026)
        .fetch()
        .await
        .unwrap_err();
    assert_eq!(
        error.kind(),
        ErrorKind::Concord(ConcordErrorKind::Transport)
    );
    assert_eq!(error.status(), None);
    assert!(error.meta().is_none());
    assert_eq!(error.concord().endpoint(), Some("NagerHolidays.holidays"));
    assert_eq!(server.requests().len(), 1, "no retry");
    server.shutdown().await;
}

#[tokio::test]
async fn dropped_body_keeps_head_and_never_returns_a_partial_array() {
    let server = TestServer::start(|_| {
        Reply::json(200, b"")
            .with_body(Body::FailAfter(vec![Bytes::from_static(include_bytes!(
                "../fixtures/truncated-array-synthetic.json"
            ))]))
            .header("x-synthetic-request-id", "synthetic-body-drop")
    })
    .await;
    let error = client(&server)
        .holidays("AT", 2026)
        .fetch()
        .await
        .unwrap_err();
    assert_eq!(
        error.kind(),
        ErrorKind::Concord(ConcordErrorKind::Transport)
    );
    assert_eq!(error.status(), Some(StatusCode::OK));
    assert_eq!(
        error.meta().unwrap().headers["x-synthetic-request-id"],
        "synthetic-body-drop"
    );
    assert_eq!(error.concord().endpoint(), Some("NagerHolidays.holidays"));
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

#[test]
fn construction_and_prepare_use_the_pinned_contract_without_io() {
    let api = NagerHolidays::new(concord::reqwest::Client::new());
    let prepared = api.holidays("AT", 2026).prepare().unwrap();
    assert_eq!(
        prepared.request().url().as_str(),
        "https://nagerholidays.com/api/v4/Holidays/AT/2026"
    );
    assert_eq!(prepared.request().method(), "GET");
    assert!(prepared.request().body().is_none());
    NagerHolidays::validate_header_names().unwrap();
    assert!(api.with_base_url("not a url").is_err());
}

#[tokio::test]
async fn dot_segments_fail_building_without_sending() {
    let server = TestServer::start(|_| panic!("invalid segment must not be sent")).await;
    let error = client(&server)
        .holidays("..", 2026)
        .fetch()
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Concord(ConcordErrorKind::Build));
    assert_eq!(error.status(), None);
    assert!(error.meta().is_none());
    assert!(server.requests().is_empty());
    server.shutdown().await;
}

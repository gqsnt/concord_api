//! Add precipitation through the public client; all requests stay on loopback.

use std::time::Duration;

use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_open_meteo::{
    Error, ForecastData, ForecastQuery, HourlyVariable, OpenMeteo, ResponseErrorKind,
    ResponseFormat,
};

const CSV: &[u8] = include_bytes!("../fixtures/maintenance-precipitation.csv");
const JSON: &[u8] = include_bytes!("../fixtures/maintenance-precipitation.json");

fn client(server: &TestServer) -> OpenMeteo {
    OpenMeteo::new(
        concord::reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap(),
    )
    .with_base_url(&server.url())
    .unwrap()
}

fn query() -> ForecastQuery {
    ForecastQuery::new(
        52.52,
        13.41,
        [
            HourlyVariable::temperature_2m(),
            HourlyVariable::wind_speed_10m(),
            HourlyVariable::precipitation(),
        ],
    )
    .unwrap()
}

fn assert_request(request: &Recorded, format: &str, accept: &str) {
    assert_eq!(request.method, "GET");
    assert_eq!(
        request.target,
        format!(
            "/v1/forecast?latitude=52.52&longitude=13.41&hourly=temperature_2m%2Cwind_speed_10m%2Cprecipitation&format={format}"
        )
    );
    assert_eq!(request.header_all("accept"), [accept]);
    assert_eq!(request.header("authorization"), None);
    assert!(request.body.is_empty());
}

#[tokio::test]
async fn selected_precipitation_maps_by_header_and_keeps_unknown_columns() {
    let server = TestServer::start(|_| {
        Reply::new(200)
            .header("content-type", "text/csv; charset=utf-8")
            .body(CSV)
    })
    .await;
    let response = client(&server)
        .forecast(&query(), ResponseFormat::Csv)
        .await
        .unwrap();
    assert_eq!(response.meta.status().as_u16(), 200);
    assert!(response.meta.operation().is_some());
    let ForecastData::Csv(csv) = response.data else {
        panic!("CSV requested")
    };
    assert_eq!(csv.bytes().as_ref(), CSV);
    assert_eq!(csv.rows.len(), 4);
    // The new variable is requested last but returned first; time is in the middle.
    assert_eq!(csv.columns[0].header, "precipitation (mm)");
    assert_eq!(csv.columns[0].variable, "precipitation");
    assert_eq!(csv.columns[0].unit.as_deref(), Some("mm"));
    assert_eq!(csv.columns[2].header, "time");
    for (index, (row, expected)) in csv
        .rows
        .iter()
        .zip([Some(0.0), None, None, Some(1.2)])
        .enumerate()
    {
        assert_eq!(row.time, format!("2024-01-01T0{index}:00"));
        assert_eq!(row.values.len(), 3);
        assert_eq!(row.values["precipitation"], expected);
        assert_eq!(row.unknown.len(), 2);
    }
    assert_eq!(csv.rows[0].values["temperature_2m"], Some(13.0));
    assert_eq!(csv.rows[0].values["wind_speed_10m"], Some(5.4));
    assert_eq!(csv.rows[0].unknown["surface_pressure (hPa)"], "1001.00");
    assert_eq!(csv.rows[2].unknown["surface_pressure (hPa)"], "NaN");
    assert_eq!(
        csv.rows[0].unknown["provider_note (text)"],
        "wet, then \"dry\""
    );
    assert_eq!(
        csv.rows[1].unknown["provider_note (text)"],
        "line one\nline two"
    );
    assert_eq!(csv.rows[2].unknown["provider_note (text)"], "");
    assert_request(&server.single(), "csv", "text/csv");
    server.shutdown().await;
}

#[tokio::test]
async fn unselected_precipitation_remains_exact_text_for_existing_callers() {
    let server =
        TestServer::start(|_| Reply::new(200).header("content-type", "text/csv").body(CSV)).await;
    let query = ForecastQuery::new(
        52.52,
        13.41,
        [
            HourlyVariable::temperature_2m(),
            HourlyVariable::wind_speed_10m(),
        ],
    )
    .unwrap();
    let response = client(&server)
        .forecast(&query, ResponseFormat::Csv)
        .await
        .unwrap();
    let ForecastData::Csv(csv) = response.data else {
        panic!("CSV requested")
    };
    assert_eq!(csv.rows.len(), 4);
    for (row, expected) in csv.rows.iter().zip(["0.00", "NaN", "", "1.20"]) {
        assert_eq!(row.values.len(), 2);
        assert!(!row.values.contains_key("precipitation"));
        assert_eq!(row.unknown["precipitation (mm)"], expected);
        assert_eq!(row.unknown.len(), 3);
    }
    assert_eq!(
        server.single().query_all("hourly"),
        ["temperature_2m,wind_speed_10m"]
    );
    server.shutdown().await;
}

#[tokio::test]
async fn precipitation_works_in_both_runtime_formats_with_equivalent_values() {
    let server = TestServer::start(|request| match request.query_all("format")[0].as_str() {
        "json" => Reply::json(200, JSON),
        "csv" => Reply::new(200).header("content-type", "text/csv").body(CSV),
        _ => panic!("unexpected format"),
    })
    .await;
    let api = client(&server);
    let query = query();
    let json = api.forecast(&query, ResponseFormat::Json).await.unwrap();
    let csv = api.forecast(&query, ResponseFormat::Csv).await.unwrap();
    let (ForecastData::Json(json), ForecastData::Csv(csv)) = (json.data, csv.data) else {
        panic!("requested JSON then CSV")
    };
    assert_eq!(json.hourly_units["precipitation"], "mm");
    assert_eq!(
        json.hourly.variables["precipitation"],
        [Some(0.0), None, None, Some(1.2)]
    );
    assert_eq!(json.hourly.time.len(), csv.rows.len());
    for (index, row) in csv.rows.iter().enumerate() {
        assert_eq!(json.hourly.time[index], row.time);
        for variable in ["temperature_2m", "wind_speed_10m", "precipitation"] {
            assert_eq!(json.hourly.variables[variable][index], row.values[variable]);
        }
    }
    assert_eq!(json.hourly.variables["surface_pressure"][0], Some(1001.0));
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_request(&requests[0], "json", "application/json");
    assert_request(&requests[1], "csv", "text/csv");
    server.shutdown().await;
}

#[tokio::test]
async fn absent_selected_precipitation_is_an_error_in_both_formats() {
    for (format, media, body, expected_kind) in [
        (
            ResponseFormat::Csv,
            "text/csv",
            &include_bytes!("../fixtures/forecast.csv")[..],
            ResponseErrorKind::Csv,
        ),
        (
            ResponseFormat::Json,
            "application/json",
            &include_bytes!("../fixtures/forecast.json")[..],
            ResponseErrorKind::Json,
        ),
    ] {
        let server =
            TestServer::start(move |_| Reply::new(200).header("content-type", media).body(body))
                .await;
        let query = ForecastQuery::new(
            52.52,
            13.41,
            [
                HourlyVariable::temperature_2m(),
                HourlyVariable::precipitation(),
            ],
        )
        .unwrap();
        let error = client(&server).forecast(&query, format).await.unwrap_err();
        assert_eq!(error.status().unwrap().as_u16(), 200);
        assert!(error.operation().is_some());
        let Error::Response { kind, .. } = error else {
            panic!("missing selected column must fail the whole forecast")
        };
        assert_eq!(kind, expected_kind);
        assert_eq!(
            server.single().query_all("hourly"),
            ["temperature_2m,precipitation"]
        );
        server.shutdown().await;
    }
}

//! Public-client JSON and runtime CSV workflows against loopback fixtures.

use std::time::Duration;

use concord::{bytes::Bytes, reqwest};
use concord_example_testserver::{Body, Reply, TestServer};
use concord_sdk_open_meteo::{
    ForecastData, ForecastQuery, HourlyVariable, OpenMeteo, ResponseFormat,
};

const JSON: &[u8] = include_bytes!("../fixtures/forecast.json");
const CSV: &[u8] = include_bytes!("../fixtures/forecast.csv");
const EXTRA: &[u8] = include_bytes!("../fixtures/forecast-extra.csv");

fn client(server: &TestServer) -> OpenMeteo {
    OpenMeteo::new(
        reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap(),
    )
    .with_base_url(&server.url())
    .unwrap()
}

#[tokio::test]
async fn workflow_a_retrieves_typed_json_and_preserves_units_and_metadata() {
    let server = TestServer::start(|_| {
        Reply::new(200)
            .header("content-type", "application/json")
            .header("x-request-id", "synthetic-json-forecast")
            .body(JSON)
    })
    .await;
    let query = ForecastQuery::new(52.52, 13.41, [HourlyVariable::temperature_2m()]).unwrap();
    let response = client(&server)
        .forecast(&query, ResponseFormat::Json)
        .await
        .unwrap();
    assert_eq!(
        response.meta.headers()["x-request-id"],
        "synthetic-json-forecast"
    );
    let ForecastData::Json(forecast) = response.data else {
        panic!("JSON requested")
    };
    assert_eq!(forecast.latitude, 52.52);
    assert_eq!(forecast.longitude, 13.419);
    assert_eq!(forecast.elevation, Some(44.812));
    assert_eq!(forecast.utc_offset_seconds, 0);
    assert_eq!(forecast.timezone, "Europe/Berlin");
    assert_eq!(forecast.timezone_abbreviation, "CEST");
    assert_eq!(forecast.hourly_units["temperature_2m"], "°C");
    assert_eq!(forecast.hourly.time[0], "2022-07-01T00:00");
    assert_eq!(
        forecast.hourly.variables["temperature_2m"],
        [Some(13.0), Some(12.7), Some(12.7)]
    );
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

#[tokio::test]
async fn workflow_b_consumes_received_csv_rows_with_quoted_data_and_added_column() {
    let server = TestServer::start(|_| {
        // Split UTF-8 unit symbols, quote escapes, newlines and records across HTTP chunks.
        let chunks = EXTRA
            .chunks(1)
            .map(|chunk| (Duration::ZERO, Bytes::copy_from_slice(chunk)))
            .collect();
        Reply::new(200)
            .header("content-type", "text/csv; charset=utf-8")
            .header(
                "content-disposition",
                "attachment; filename=\"synthetic-forecast.csv\"",
            )
            .with_body(Body::Chunks(chunks))
    })
    .await;
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
    assert_eq!(
        response.meta.headers()["content-type"],
        "text/csv; charset=utf-8"
    );
    assert!(
        response.meta.headers()["content-disposition"]
            .to_str()
            .unwrap()
            .contains("synthetic-forecast.csv")
    );
    let ForecastData::Csv(csv) = response.data else {
        panic!("CSV requested")
    };
    assert_eq!(csv.bytes().as_ref(), EXTRA);
    assert_eq!(csv.location_headers[0], "timezone_abbreviation");
    assert_eq!(csv.location["latitude"], "52.52");
    assert_eq!(csv.location["timezone"], "Europe/Berlin");
    assert_eq!(csv.location["provider_note"], "quoted, metadata");
    assert_eq!(csv.columns[0].header, "wind_speed_10m (km/h)");
    assert_eq!(csv.columns[0].unit.as_deref(), Some("km/h"));
    assert_eq!(csv.columns[3].unit.as_deref(), Some("°C"));
    let observed: Vec<_> = csv
        .rows
        .iter()
        .map(|row| {
            (
                &row.time,
                row.values["temperature_2m"],
                row.values["wind_speed_10m"],
            )
        })
        .collect();
    assert_eq!(observed.len(), 3);
    assert_eq!(observed[0].0, "2022-07-01T00:00");
    assert_eq!((observed[0].1, observed[0].2), (Some(13.0), Some(5.4)));
    assert_eq!((observed[1].1, observed[1].2), (Some(12.7), None));
    assert_eq!(
        csv.rows[0].unknown["provider_note (text)"],
        "calm, then \"breezy\""
    );
    assert_eq!(
        csv.rows[1].unknown["provider_note (text)"],
        "line one\nline two"
    );
    assert_eq!(csv.rows[2].unknown["provider_note (text)"], "");
    assert_eq!(
        server.single().query_all("hourly"),
        ["temperature_2m,wind_speed_10m"]
    );
    server.shutdown().await;
}

#[tokio::test]
async fn one_client_selects_json_or_csv_at_runtime_with_equivalent_forecast_values() {
    let server = TestServer::start(|request| match request.query_all("format")[0].as_str() {
        "json" => Reply::new(200)
            .header("content-type", "application/json")
            .body(JSON),
        "csv" => Reply::new(200).header("content-type", "text/csv").body(CSV),
        _ => panic!("unexpected format"),
    })
    .await;
    let api = client(&server);
    let query = ForecastQuery::new(52.52, 13.41, [HourlyVariable::temperature_2m()]).unwrap();
    let mut values = Vec::new();
    for format in [ResponseFormat::Json, ResponseFormat::Csv] {
        let response = api.forecast(&query, format).await.unwrap();
        assert_eq!(
            response.meta.operation().unwrap().name(),
            "ForecastApi.forecast"
        );
        values.push(match response.data {
            ForecastData::Json(forecast) => forecast.hourly.variables["temperature_2m"].clone(),
            ForecastData::Csv(csv) => csv
                .rows
                .iter()
                .map(|row| row.values["temperature_2m"])
                .collect(),
        });
    }
    assert_eq!(values[0], values[1]);
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

#[tokio::test]
async fn csv_accepts_crlf_without_changing_original_bytes() {
    let fixture = include_bytes!("../fixtures/forecast-crlf.csv");
    let server = TestServer::start(move |_| {
        Reply::new(200)
            .header("content-type", "text/csv; header=present")
            .body(fixture)
    })
    .await;
    let query = ForecastQuery::new(52.52, 13.41, [HourlyVariable::temperature_2m()]).unwrap();
    let response = client(&server)
        .forecast(&query, ResponseFormat::Csv)
        .await
        .unwrap();
    let ForecastData::Csv(csv) = response.data else {
        panic!("CSV requested")
    };
    assert_eq!(csv.bytes().as_ref(), fixture);
    assert_eq!(csv.rows.len(), 3);
    server.shutdown().await;
}

#[tokio::test]
async fn json_null_and_additional_hourly_variables_remain_visible() {
    for fixture in [
        &include_bytes!("../fixtures/null-values.json")[..],
        &include_bytes!("../fixtures/unknown-variable.json")[..],
    ] {
        let server = TestServer::start(move |_| {
            Reply::new(200)
                .header("content-type", "application/json")
                .body(fixture)
        })
        .await;
        let query = ForecastQuery::new(52.52, 13.41, [HourlyVariable::temperature_2m()]).unwrap();
        let response = client(&server)
            .forecast(&query, ResponseFormat::Json)
            .await
            .unwrap();
        let ForecastData::Json(forecast) = response.data else {
            panic!("JSON requested")
        };
        if fixture == include_bytes!("../fixtures/null-values.json") {
            assert_eq!(forecast.hourly.variables["temperature_2m"][1], None);
        } else {
            assert_eq!(
                forecast.hourly.variables["surface_pressure"][0],
                Some(1001.0)
            );
            assert_eq!(forecast.hourly_units["surface_pressure"], "hPa");
        }
        server.shutdown().await;
    }
}

#[tokio::test]
async fn another_documented_variable_uses_the_same_operation_without_sdk_changes() {
    let server = TestServer::start(|request| match request.query_all("format")[0].as_str() {
        "json" => Reply::new(200)
            .header("content-type", "application/json")
            .body(include_bytes!("../fixtures/unknown-variable.json")),
        "csv" => Reply::new(200)
            .header("content-type", "text/csv")
            .body(include_bytes!("../fixtures/forecast-pressure.csv")),
        _ => panic!("unexpected format"),
    })
    .await;
    let api = client(&server);
    let query = ForecastQuery::new(
        52.52,
        13.41,
        [HourlyVariable::named("surface_pressure").unwrap()],
    )
    .unwrap();
    for format in [ResponseFormat::Json, ResponseFormat::Csv] {
        let response = api.forecast(&query, format).await.unwrap();
        match response.data {
            ForecastData::Json(forecast) => {
                assert_eq!(forecast.hourly_units["surface_pressure"], "hPa");
                assert_eq!(
                    forecast.hourly.variables["surface_pressure"][0],
                    Some(1001.0)
                );
            }
            ForecastData::Csv(csv) => {
                assert_eq!(csv.columns[0].unit.as_deref(), Some("hPa"));
                assert_eq!(csv.rows[0].values["surface_pressure"], Some(1001.0));
                assert_eq!(csv.rows[0].unknown["temperature_2m (°C)"], "13.0");
            }
        }
    }
    assert!(
        server
            .requests()
            .iter()
            .all(|request| request.query_all("hourly") == ["surface_pressure"])
    );
    server.shutdown().await;
}

//! Application-style first use against an independently scripted local provider.
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use concord_sdk_open_meteo::{
    Error, ForecastData, ForecastQuery, HourlyVariable, OpenMeteo, ResponseErrorKind,
    ResponseFormat,
};

const JSON: &[u8] = include_bytes!("../fixtures/first_use_forecast.json");
const CSV: &[u8] = include_bytes!("../fixtures/first_use_forecast.csv");

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn query() -> Result<ForecastQuery, Box<dyn std::error::Error>> {
    Ok(ForecastQuery::new(
        52.52,
        13.41,
        [
            HourlyVariable::temperature_2m(),
            HourlyVariable::wind_speed_10m(),
        ],
    )?
    .with_timezone("Europe/Berlin")
    .with_forecast_days(1)?)
}

fn client(server: &TestServer) -> Result<OpenMeteo, Box<dyn std::error::Error>> {
    Ok(OpenMeteo::new(concord::reqwest::Client::new())
        .with_base_url(&format!("{}/proxy", server.url()))?)
}

fn assert_request(request: &Recorded, format: &str, accept: &str) {
    assert_eq!(request.method, "GET");
    assert_eq!(request.path(), "/proxy/v1/forecast");
    assert_eq!(request.query_all("latitude"), ["52.52"]);
    assert_eq!(request.query_all("longitude"), ["13.41"]);
    assert_eq!(
        request.query_all("hourly"),
        ["temperature_2m,wind_speed_10m"]
    );
    assert_eq!(request.query_all("timezone"), ["Europe/Berlin"]);
    assert_eq!(request.query_all("forecast_days"), ["1"]);
    assert_eq!(request.query_all("format"), [format]);
    assert_eq!(request.query_pairs().len(), 6);
    assert_eq!(request.header("accept"), Some(accept));
    assert_eq!(request.body_len, 0);
}

#[tokio::test]
async fn json_forecast_is_fully_available_with_metadata() -> TestResult {
    let server = TestServer::start(|_| Reply::json(200, JSON).header("x-fixture", "json")).await;
    let response = client(&server)?
        .forecast(&query()?, ResponseFormat::Json)
        .await?;
    assert_eq!(response.meta.status().as_u16(), 200);
    assert_eq!(response.meta.headers()["x-fixture"], "json");
    assert!(response.meta.operation().is_some());
    let ForecastData::Json(forecast) = response.data else {
        panic!("expected JSON")
    };
    assert_eq!(forecast.timezone, "Europe/Berlin");
    assert_eq!(
        forecast.hourly.time,
        ["2024-01-01T00:00", "2024-01-01T01:00"]
    );
    assert_eq!(forecast.hourly_units["temperature_2m"], "°C");
    assert_eq!(
        forecast.hourly.variables["temperature_2m"],
        [Some(13.0), None]
    );
    assert_eq!(
        forecast.hourly.variables["wind_speed_10m"],
        [Some(4.5), Some(5.0)]
    );
    assert_request(&server.single(), "json", "application/json");
    server.shutdown().await;
    Ok(())
}

#[tokio::test]
async fn provider_problem_keeps_status_headers_and_document() -> TestResult {
    let server = TestServer::start(|_| {
        Reply::json(
            400,
            br#"{"error":true,"reason":"synthetic invalid hourly variable"}"#,
        )
        .header("retry-after", "7")
    })
    .await;
    let error = client(&server)?
        .forecast(&query()?, ResponseFormat::Json)
        .await
        .unwrap_err();
    assert_eq!(error.status().map(|s| s.as_u16()), Some(400));
    assert!(error.operation().is_some());
    assert_eq!(error.meta().unwrap().headers()["retry-after"], "7");
    assert!(!error.to_string().contains("synthetic invalid"));
    let Error::Provider(problem) = error else {
        panic!("expected provider error")
    };
    let document = problem.document?;
    assert!(document.error);
    assert_eq!(document.reason, "synthetic invalid hourly variable");
    assert_request(&server.single(), "json", "application/json");
    server.shutdown().await;
    Ok(())
}

#[tokio::test]
async fn inconsistent_json_is_not_a_forecast() -> TestResult {
    let broken = String::from_utf8(JSON.to_vec())?.replace("[4.5,5.0]", "[4.5]");
    let server = TestServer::start(move |_| Reply::json(200, &broken)).await;
    let error = client(&server)?
        .forecast(&query()?, ResponseFormat::Json)
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        Error::Response {
            kind: ResponseErrorKind::Json,
            ..
        }
    ));
    assert_eq!(error.status().map(|s| s.as_u16()), Some(200));
    assert_request(&server.single(), "json", "application/json");
    server.shutdown().await;
    Ok(())
}

#[tokio::test]
async fn runtime_csv_yields_rows_units_and_original_bytes() -> TestResult {
    let server = TestServer::start(|_| Reply::text(200, "text/csv; charset=utf-8", CSV)).await;
    // A runtime application setting selects the representation for this request.
    let format = ResponseFormat::Csv;
    let response = client(&server)?.forecast(&query()?, format).await?;
    assert_eq!(
        response.meta.headers()["content-type"],
        "text/csv; charset=utf-8"
    );
    let ForecastData::Csv(csv) = response.data else {
        panic!("expected CSV")
    };
    assert_eq!(csv.location["timezone"], "Europe/Berlin");
    assert_eq!(csv.bytes().as_ref(), CSV);
    assert!(
        csv.columns
            .iter()
            .any(|c| c.variable == "temperature_2m" && c.unit.as_deref() == Some("°C"))
    );
    assert_eq!(csv.rows.len(), 2);
    assert_eq!(csv.rows[0].time, "2024-01-01T00:00");
    assert_eq!(csv.rows[0].values["temperature_2m"], Some(13.0));
    assert_eq!(csv.rows[0].values["wind_speed_10m"], Some(4.5));
    assert_eq!(csv.rows[0].unknown["note"], "clear, cold");
    assert_eq!(csv.rows[1].values["temperature_2m"], None);
    assert_eq!(csv.rows[1].unknown["note"], "say \"hello\"");
    assert_request(&server.single(), "csv", "text/csv");
    server.shutdown().await;
    Ok(())
}

#[tokio::test]
async fn malformed_late_csv_row_fails_the_whole_forecast() -> TestResult {
    let broken = String::from_utf8(CSV.to_vec())?.replace("5.0,NaN", "5.0,not-a-number");
    let server = TestServer::start(move |_| Reply::text(200, "text/csv", &broken)).await;
    let error = client(&server)?
        .forecast(&query()?, ResponseFormat::Csv)
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        Error::Response {
            kind: ResponseErrorKind::Csv,
            ..
        }
    ));
    assert_eq!(error.status().map(|s| s.as_u16()), Some(200));
    assert_request(&server.single(), "csv", "text/csv");
    server.shutdown().await;
    Ok(())
}

#[tokio::test]
async fn csv_connection_cut_after_valid_bytes_is_not_completion() -> TestResult {
    let server = TestServer::start(|_| {
        Reply::new(200)
            .header("content-type", "text/csv")
            .with_body(Body::FailAfter(vec![CSV.to_vec().into()]))
    })
    .await;
    let error = client(&server)?
        .forecast(&query()?, ResponseFormat::Csv)
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Concord { .. }));
    assert_eq!(error.status().map(|s| s.as_u16()), Some(200));
    assert!(error.operation().is_some());
    assert_request(&server.single(), "csv", "text/csv");
    server.shutdown().await;
    Ok(())
}

#[tokio::test]
async fn non_json_provider_error_still_keeps_the_http_head() -> TestResult {
    let server = TestServer::start(|_| {
        Reply::text(503, "text/plain", "synthetic upstream failure").header("retry-after", "9")
    })
    .await;
    let error = client(&server)?
        .forecast(&query()?, ResponseFormat::Csv)
        .await
        .unwrap_err();
    assert_eq!(error.status().map(|s| s.as_u16()), Some(503));
    assert!(error.operation().is_some());
    assert_eq!(error.meta().unwrap().headers()["retry-after"], "9");
    let Error::Provider(problem) = error else {
        panic!("expected provider error")
    };
    assert!(problem.document.is_err());
    assert_request(&server.single(), "csv", "text/csv");
    server.shutdown().await;
    Ok(())
}

#[tokio::test]
async fn mismatched_success_media_is_rejected() -> TestResult {
    let server = TestServer::start(|_| Reply::json(200, JSON)).await;
    let error = client(&server)?
        .forecast(&query()?, ResponseFormat::Csv)
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        Error::Response {
            kind: ResponseErrorKind::Media,
            ..
        }
    ));
    assert_eq!(error.status().map(|s| s.as_u16()), Some(200));
    assert_request(&server.single(), "csv", "text/csv");
    server.shutdown().await;
    Ok(())
}

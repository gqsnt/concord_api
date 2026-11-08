//! Offline request-wire, failure-shape and completion checks.

use std::time::Duration;

use concord::{ErrorKind, bytes::Bytes};
use concord_example_testserver::{Body, Recorded, Reply, TestServer, paused};
use concord_sdk_open_meteo::{
    ERROR_BODY_LIMIT, Error, ErrorBodyError, FORECAST_BODY_LIMIT, ForecastData, ForecastQuery,
    HourlyVariable, OpenMeteo, ResponseErrorKind, ResponseFormat,
};

const JSON: &[u8] = include_bytes!("../fixtures/forecast.json");
const CSV: &[u8] = include_bytes!("../fixtures/forecast.csv");
const PROBLEM: &[u8] = include_bytes!("../fixtures/provider-error.json");

fn query() -> ForecastQuery {
    ForecastQuery::new(52.52, 13.41, [HourlyVariable::temperature_2m()]).unwrap()
}

fn client(server: &TestServer) -> OpenMeteo {
    OpenMeteo::new(
        concord::reqwest::Client::builder()
            .no_proxy()
            .redirect(concord::reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap(),
    )
    .with_base_url(&server.url())
    .unwrap()
}

fn assert_get(request: &Recorded, format: &str, accept: &str) {
    assert_eq!(request.method, "GET");
    assert_eq!(request.query_all("format"), [format]);
    assert_eq!(request.header_all("accept"), [accept]);
    assert_eq!(request.header("authorization"), None);
    assert_eq!(request.header("content-type"), None);
    assert_eq!(request.header("transfer-encoding"), None);
    assert_eq!(request.body, b"");
    assert_eq!(request.body_len, 0);
    assert!(!request.chunked);
}

#[tokio::test]
async fn json_wire_has_exact_target_headers_and_empty_body() {
    let server = TestServer::start(|_| {
        Reply::new(200)
            .header("content-type", "application/json")
            .body(JSON)
    })
    .await;
    let api = client(&server)
        .with_base_url(&format!("{}/proxy", server.url()))
        .unwrap();
    let query = query()
        .with_timezone("Europe/Berlin")
        .with_forecast_days(1)
        .unwrap();
    let result = api.forecast(&query, ResponseFormat::Json).await.unwrap();
    assert!(matches!(result.data, ForecastData::Json(_)));
    assert_eq!(
        result.meta.operation().unwrap().name(),
        "ForecastApi.forecast"
    );
    let request = server.single();
    assert_get(&request, "json", "application/json");
    assert_eq!(
        request.target,
        "/proxy/v1/forecast?latitude=52.52&longitude=13.41&hourly=temperature_2m&format=json&timezone=Europe%2FBerlin&forecast_days=1"
    );
    assert_eq!(request.host, server.addr().to_string());
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn csv_wire_uses_same_operation_and_omits_optional_parameters() {
    let server = TestServer::start(|_| {
        Reply::new(200)
            .header("content-type", "text/csv; charset=utf-8")
            .body(CSV)
    })
    .await;
    let result = client(&server)
        .forecast(&query(), ResponseFormat::Csv)
        .await
        .unwrap();
    assert!(matches!(result.data, ForecastData::Csv(_)));
    assert_eq!(
        result.meta.operation().unwrap().name(),
        "ForecastApi.forecast"
    );
    let request = server.single();
    assert_get(&request, "csv", "text/csv");
    assert_eq!(
        request.target,
        "/v1/forecast?latitude=52.52&longitude=13.41&hourly=temperature_2m&format=csv"
    );
    server.shutdown().await;
}

#[tokio::test]
async fn multiple_variables_are_one_encoded_query_value() {
    let server = TestServer::start(|_| {
        Reply::new(200)
            .header("content-type", "text/csv")
            .body(include_bytes!("../fixtures/forecast-extra.csv"))
    })
    .await;
    let query = ForecastQuery::new(
        -33.86,
        151.21,
        [
            HourlyVariable::temperature_2m(),
            HourlyVariable::wind_speed_10m(),
        ],
    )
    .unwrap();
    client(&server)
        .forecast(&query, ResponseFormat::Csv)
        .await
        .unwrap();
    let request = server.single();
    assert_eq!(
        request.target,
        "/v1/forecast?latitude=-33.86&longitude=151.21&hourly=temperature_2m%2Cwind_speed_10m&format=csv"
    );
    assert_eq!(
        request.query_all("hourly"),
        ["temperature_2m,wind_speed_10m"]
    );
    assert_get(&request, "csv", "text/csv");
    server.shutdown().await;
}

#[tokio::test]
async fn provider_json_errors_are_typed_even_when_csv_was_requested() {
    for format in [ResponseFormat::Json, ResponseFormat::Csv] {
        let server = TestServer::start(|_| {
            Reply::new(400)
                .header("content-type", "application/json; charset=utf-8")
                .header("x-request-id", "synthetic-open-meteo-request")
                .header("retry-after", "17")
                .body(PROBLEM)
        })
        .await;
        let error = client(&server)
            .forecast(&query(), format)
            .await
            .unwrap_err();
        assert_eq!(error.status().unwrap().as_u16(), 400);
        assert_eq!(error.operation().unwrap().name(), "ForecastApi.forecast");
        assert_eq!(error.meta().unwrap().headers()["retry-after"], "17");
        assert_eq!(
            error.meta().unwrap().headers()["x-request-id"],
            "synthetic-open-meteo-request"
        );
        assert!(!format!("{error:?}").contains("tempeture_2m"));
        let Error::Provider(provider) = error else {
            panic!("expected typed provider error")
        };
        let problem = provider.document.unwrap();
        assert!(problem.error);
        assert!(problem.reason.contains("tempeture_2m"));
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn malformed_error_document_retains_status_and_decode_diagnostic() {
    let server = TestServer::start(|_| {
        Reply::new(429)
            .header("content-type", "application/json")
            .header("retry-after", "5")
            .body(include_bytes!("../fixtures/malformed-error.json"))
    })
    .await;
    let error = client(&server)
        .forecast(&query(), ResponseFormat::Csv)
        .await
        .unwrap_err();
    assert_eq!(error.status().unwrap().as_u16(), 429);
    let Error::Provider(provider) = error else {
        panic!("provider error")
    };
    assert!(matches!(provider.document, Err(ErrorBodyError::Decode(_))));
    assert_eq!(provider.meta.headers()["retry-after"], "5");
    server.shutdown().await;
}

#[tokio::test]
async fn non_json_error_document_retains_head_without_reading_stalled_body() {
    let server = TestServer::start(|_| {
        Reply::new(502)
            .header("content-type", "text/html")
            .with_body(Body::Stall)
    })
    .await;
    // Clock frozen while the call runs: the bound fails only a call that waits for something
    // that never comes (then reported as stuck).
    let error = tokio::time::timeout(
        Duration::from_secs(1),
        paused::frozen(client(&server).forecast(&query(), ResponseFormat::Csv)),
    )
    .await
    .expect("must reject the head without waiting for the body")
    .unwrap_err();
    let Error::Provider(provider) = error else {
        panic!("provider error")
    };
    assert_eq!(provider.meta.status().as_u16(), 502);
    assert!(matches!(
        provider.document,
        Err(ErrorBodyError::Metadata(_))
    ));
    server.shutdown().await;
}

#[tokio::test]
async fn oversized_error_document_has_typed_limit_and_real_metadata() {
    let server = TestServer::start(|_| {
        Reply::new(503)
            .header("content-type", "application/json")
            .body(vec![b' '; ERROR_BODY_LIMIT + 1])
    })
    .await;
    let error = client(&server)
        .forecast(&query(), ResponseFormat::Json)
        .await
        .unwrap_err();
    let Error::Provider(provider) = error else {
        panic!("provider error")
    };
    assert_eq!(provider.meta.status().as_u16(), 503);
    let Err(ErrorBodyError::Read(error)) = provider.document else {
        panic!("bounded read error")
    };
    assert_eq!(error.kind(), ErrorKind::Limit);
    assert_eq!(error.operation().unwrap().name(), "ForecastApi.forecast");
    server.shutdown().await;
}

#[tokio::test]
async fn mismatched_missing_duplicate_and_unsupported_media_are_refused_before_body() {
    for (format, headers) in [
        (ResponseFormat::Csv, vec!["application/json"]),
        (ResponseFormat::Json, vec!["text/csv"]),
        (ResponseFormat::Csv, vec![]),
        (ResponseFormat::Csv, vec!["text/csv", "text/csv"]),
        (ResponseFormat::Csv, vec!["text/csv; charset=iso-8859-1"]),
        (ResponseFormat::Csv, vec!["text/csv; header=absent"]),
        (
            ResponseFormat::Json,
            vec!["application/json", "application/json"],
        ),
    ] {
        let server = TestServer::start(move |_| {
            let mut reply = Reply::new(200).with_body(Body::Stall);
            for header in &headers {
                reply = reply.header("content-type", *header);
            }
            reply
        })
        .await;
        // Clock frozen while the call runs: the bound fails only a call that waits for something
        // that never comes (then reported as stuck).
        let error = tokio::time::timeout(
            Duration::from_secs(1),
            paused::frozen(client(&server).forecast(&query(), format)),
        )
        .await
        .expect("head refusal must not read a stalled body")
        .unwrap_err();
        assert!(matches!(
            error,
            Error::Response {
                kind: ResponseErrorKind::Media,
                ..
            }
        ));
        assert_eq!(error.status().unwrap().as_u16(), 200);
        assert_eq!(error.operation().unwrap().name(), "ForecastApi.forecast");
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn csv_structure_and_late_row_failures_return_no_partial_forecast() {
    for fixture in [
        &include_bytes!("../fixtures/wrong-width.csv")[..],
        &include_bytes!("../fixtures/missing-time.csv")[..],
        &include_bytes!("../fixtures/missing-variable.csv")[..],
        &include_bytes!("../fixtures/missing-unit.csv")[..],
        &include_bytes!("../fixtures/duplicate-header.csv")[..],
        &include_bytes!("../fixtures/duplicate-variable.csv")[..],
        &include_bytes!("../fixtures/invalid-number.csv")[..],
        &include_bytes!("../fixtures/unterminated-quote.csv")[..],
        &include_bytes!("../fixtures/missing-location.csv")[..],
        &include_bytes!("../fixtures/wrong-location-width.csv")[..],
        &include_bytes!("../fixtures/non-utf8.csv")[..],
    ] {
        let server = TestServer::start(move |_| {
            Reply::new(200)
                .header("content-type", "text/csv")
                .body(fixture)
        })
        .await;
        let error = client(&server)
            .forecast(&query(), ResponseFormat::Csv)
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            Error::Response {
                kind: ResponseErrorKind::Csv,
                ..
            }
        ));
        assert_eq!(error.status().unwrap().as_u16(), 200);
        assert_eq!(error.operation().unwrap().name(), "ForecastApi.forecast");
        server.shutdown().await;
    }
}

#[tokio::test]
async fn json_structure_errors_return_no_partial_forecast() {
    for fixture in [
        &include_bytes!("../fixtures/mismatched-arrays.json")[..],
        &include_bytes!("../fixtures/missing-json-unit.json")[..],
        &include_bytes!("../fixtures/malformed-error.json")[..],
    ] {
        let server = TestServer::start(move |_| {
            Reply::new(200)
                .header("content-type", "application/json")
                .body(fixture)
        })
        .await;
        let error = client(&server)
            .forecast(&query(), ResponseFormat::Json)
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            Error::Response {
                kind: ResponseErrorKind::Json,
                ..
            }
        ));
        assert_eq!(error.status().unwrap().as_u16(), 200);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn success_body_limit_keeps_real_head() {
    let server = TestServer::start(|_| {
        Reply::new(200)
            .header("content-type", "text/csv")
            .body(vec![b' '; FORECAST_BODY_LIMIT + 1])
    })
    .await;
    let error = client(&server)
        .forecast(&query(), ResponseFormat::Csv)
        .await
        .unwrap_err();
    assert_eq!(error.status().unwrap().as_u16(), 200);
    let Error::Concord { source, meta } = error else {
        panic!("body read error")
    };
    assert_eq!(source.kind(), ErrorKind::Limit);
    assert_eq!(
        meta.unwrap().operation().unwrap().name(),
        "ForecastApi.forecast"
    );
    server.shutdown().await;
}

#[tokio::test]
async fn incomplete_http_framing_fails_even_after_complete_csv_bytes() {
    let server = TestServer::start(|_| {
        Reply::new(200)
            .header("content-type", "text/csv")
            .with_body(Body::FailAfter(vec![Bytes::from_static(CSV)]))
    })
    .await;
    let error = client(&server)
        .forecast(&query(), ResponseFormat::Csv)
        .await
        .unwrap_err();
    assert_eq!(error.status().unwrap().as_u16(), 200);
    let Error::Concord { source, .. } = error else {
        panic!("transport failure")
    };
    assert_eq!(source.kind(), ErrorKind::Transport);
    server.shutdown().await;
}

#[tokio::test]
async fn transport_failure_before_head_retains_operation_without_inventing_status() {
    let server = TestServer::start(|_| Reply::hang_up()).await;
    let error = client(&server)
        .forecast(&query(), ResponseFormat::Json)
        .await
        .unwrap_err();
    assert!(error.meta().is_none());
    assert!(error.status().is_none());
    assert_eq!(error.operation().unwrap().name(), "ForecastApi.forecast");
    let Error::Concord { source, .. } = error else {
        panic!("transport failure")
    };
    assert_eq!(source.kind(), ErrorKind::Transport);
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

#[test]
fn invalid_query_shapes_are_rejected_locally() {
    assert!(ForecastQuery::new(f64::NAN, 0.0, [HourlyVariable::temperature_2m()]).is_err());
    assert!(ForecastQuery::new(91.0, 0.0, [HourlyVariable::temperature_2m()]).is_err());
    assert!(ForecastQuery::new(0.0, 181.0, [HourlyVariable::temperature_2m()]).is_err());
    assert!(ForecastQuery::new(0.0, 0.0, []).is_err());
    assert!(
        ForecastQuery::new(
            0.0,
            0.0,
            [
                HourlyVariable::temperature_2m(),
                HourlyVariable::temperature_2m()
            ]
        )
        .is_err()
    );
    assert!(query().with_forecast_days(17).is_err());
    assert!(HourlyVariable::named("temperature_2m,wind_speed_10m").is_err());
    assert!(HourlyVariable::named("").is_err());
}

/// A final quoted field that never closes is a truncated body. Concord's CSV reader refuses it
/// since the between-wave 4-5 fix (this SDK's FRICTION.md F4 recorded the frozen reader accepting
/// it, and an SDK quote-count guard that is now removed).
#[tokio::test]
async fn csv_ending_inside_a_quoted_field_is_refused() {
    use concord::csv::{CsvOptions, CsvReader, HeaderMode};
    use concord::stream::{Chunks, Completion};

    let fixture = &include_bytes!("../fixtures/unterminated-quote.csv")[..];
    let reader = CsvReader::new(CsvOptions::new(HeaderMode::Absent, b',').flexible(true)).unwrap();
    let mut records = reader.read_raw(Chunks::new([fixture]));
    let mut count = 0;
    let error = loop {
        match records.next().await {
            Some(Ok(_)) => count += 1,
            Some(Err(error)) => break error,
            None => panic!("a truncated quoted field ended as if complete"),
        }
    };
    // The five complete records arrive; the sixth, still inside its quoted field, is refused.
    assert_eq!(count, 5);
    assert!(error.is_terminal());
    assert_eq!(records.completion(), Completion::Failed);

    let server = TestServer::start(move |_| {
        Reply::new(200)
            .header("content-type", "text/csv")
            .body(fixture)
    })
    .await;
    let error = client(&server)
        .forecast(&query(), ResponseFormat::Csv)
        .await
        .unwrap_err();
    let Error::Response { kind, source, .. } = error else {
        panic!("CSV refusal")
    };
    assert_eq!(kind, ResponseErrorKind::Csv);
    assert_eq!(source.label(), "the CSV input ends inside a quoted field");
    server.shutdown().await;
}

#[tokio::test]
async fn csv_record_and_record_byte_limits_are_not_successful_completion() {
    let prefix = CSV.splitn(2, |&byte| byte == b'\n').next().unwrap();
    for oversized_record in [false, true] {
        let body = if oversized_record {
            // One large unquoted field, below the overall body cap.
            [prefix, b"\n", &vec![b'x'; 256 * 1024 + 1]].concat()
        } else {
            // The first three records are the location/header envelope.
            let envelope = CSV
                .split(|&byte| byte == b'\n')
                .take(4)
                .flat_map(|line| line.iter().copied().chain(*b"\n"))
                .collect::<Vec<_>>();
            [
                envelope,
                b"2022-07-01T00:00,13.0\n"
                    .repeat(concord_sdk_open_meteo::CSV_RECORD_LIMIT as usize),
            ]
            .concat()
        };
        assert!(body.len() < FORECAST_BODY_LIMIT);
        let server = TestServer::start(move |_| {
            Reply::new(200)
                .header("content-type", "text/csv")
                .body(&body)
        })
        .await;
        let error = client(&server)
            .forecast(&query(), ResponseFormat::Csv)
            .await
            .unwrap_err();
        assert_eq!(error.status().unwrap().as_u16(), 200);
        let Error::Response { kind, source, .. } = error else {
            panic!("CSV limit refusal")
        };
        assert_eq!(kind, ResponseErrorKind::Csv);
        assert!(source.label().contains("exceeds"));
        server.shutdown().await;
    }
}

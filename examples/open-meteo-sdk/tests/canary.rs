//! No secret inputs: public forecasts have no credentials, #[sensitive] inputs or withheld
//! configuration strings. An ordinary timezone canary is the delivery control and is echoed
//! into the provider reason withheld by error wrappers (src/error.rs:56,65).
//! All seven scenarios run in JSON and CSV. Check client, results, response metadata, SDK/
//! Concord/codec errors and source chains. The ordinary ForecastQuery and returned provider/
//! forecast text and raw Problem are clear data; they are not absence-checked with the
//! control canary.
//! Native reqwest and explicit unredacted accessors excluded. No public endpoint/Prepared
//! handles (wire module is private). No wire exemptions.

use concord_check_canary::Canaries;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use std::{net::TcpListener, time::Duration};

fn http() -> concord::reqwest::Client {
    concord::reqwest::Client::builder()
        .no_proxy()
        .redirect(concord::reqwest::redirect::Policy::none())
        .retry(concord::reqwest::retry::never())
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap()
}

fn refused() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    url
}

fn reply(case: usize, media: &str, success: &str, problem: &str, echo: &str) -> Reply {
    let response = match case {
        0 | 6 => Reply::text(200, media, success),
        1 => Reply::text(400, media, problem),
        2 => Reply::text(503, media, problem),
        3 => Reply::text(200, media, format!("<{{invalid {echo}")),
        4 => Reply::text(200, "text/html", echo),
        5 => Reply::text(200, media, "").with_body(Body::FailAfter(vec![
            format!("<{{incomplete {echo}").into(),
        ])),
        _ => unreachable!(),
    };
    response
        .header("x-private", echo)
        .header("set-cookie", echo)
}

fn concord_error(c: &mut Canaries, e: &concord::Error) {
    c.assert_debug_absent("Concord error", e);
    c.assert_error_absent("Concord error/source chain", e);
    c.assert_debug_absent("error kind", &e.kind());
    c.assert_debug_absent("error head", &e.head());
    c.assert_debug_absent("error metadata", &e.head_meta());
}

use concord_sdk_open_meteo::{Error, ForecastQuery, HourlyVariable, OpenMeteo, ResponseFormat};

fn error(c: &mut Canaries, e: &Error) {
    c.assert_debug_absent("SDK error", e);
    c.assert_error_absent("SDK error/source chain", e);
    c.assert_debug_absent("error metadata", &e.meta());
    c.assert_debug_absent("error operation", &e.operation());
    match e {
        Error::Concord { source, .. } => concord_error(c, source),
        Error::Provider(p) => {
            c.assert_debug_absent("provider error", p);
            c.assert_error_absent("provider error/source chain", p.as_ref());
            if let Err(e) = &p.document {
                c.assert_error_absent("provider body error", e);
            }
        }
        Error::Response { kind, source, .. } => {
            c.assert_debug_absent("response stage", kind);
            c.assert_debug_absent("codec error", source);
            c.assert_error_absent("codec error/source chain", source);
        }
    }
}

#[tokio::test]
async fn request_control_never_enters_diagnostics() {
    let mut c = Canaries::new("open-meteo");
    let timezone = c.issue();
    let query = ForecastQuery::new(52.52, 13.41, [HourlyVariable::temperature_2m()])
        .unwrap()
        .with_timezone(&timezone);
    for format in [ResponseFormat::Json, ResponseFormat::Csv] {
        for case in 0..7 {
            let echo = timezone.clone();
            let server = TestServer::start(move |_: &Recorded| {
                let (media, success) = match format {
                    ResponseFormat::Json => (
                        "application/json",
                        include_str!("../fixtures/forecast.json"),
                    ),
                    ResponseFormat::Csv => ("text/csv", include_str!("../fixtures/forecast.csv")),
                };
                if matches!(case, 1 | 2) {
                    return Reply::json(
                        if case == 1 { 400 } else { 503 },
                        format!(r#"{{"error":true,"reason":"{echo}"}}"#),
                    )
                    .header("x-private", &echo);
                }
                reply(case, media, success, "", &echo)
            })
            .await;
            let api = OpenMeteo::new(http())
                .with_base_url(&if case == 6 { refused() } else { server.url() })
                .unwrap();
            c.assert_debug_absent("client", &api);
            let result = api.forecast(&query, format).await;
            c.assert_debug_absent("forecast Result", &result);
            assert_eq!(result.is_ok(), case == 0);
            match result {
                Ok(response) => {
                    c.assert_debug_absent("forecast response", &response);
                    c.assert_debug_absent("HTTP metadata", &response.meta);
                    c.assert_debug_absent("forecast data", &response.data);
                }
                Err(e) => {
                    error(&mut c, &e);
                    if case == 1 {
                        assert_eq!(e.status().unwrap().as_u16(), 400);
                        assert!(e.to_string().contains("400"));
                        assert_eq!(e.operation().unwrap().name(), "ForecastApi.forecast");
                        let Error::Provider(p) = &e else {
                            panic!("provider failure expected")
                        };
                        assert_eq!(p.document.as_ref().unwrap().reason, timezone);
                    }
                }
            }
            if case == 0 {
                c.assert_on_wire(&server.requests());
            }
            if case == 6 {
                assert!(server.requests().is_empty());
            }
            server.shutdown().await;
        }
    }
    let invalid =
        ForecastQuery::new(f64::NAN, 0.0, [HourlyVariable::temperature_2m()]).unwrap_err();
    c.assert_debug_absent("query validation error", &invalid);
    c.assert_error_absent("query validation error", &invalid);
    assert!(c.checked() >= 200);
    println!(
        "open-meteo: {} renderings; ordinary-string positive control passed",
        c.checked()
    );
}

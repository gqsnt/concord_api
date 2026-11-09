//! No secret input: Frankfurter is unauthenticated (`Frankfurter::new(http)`, no config record
//! or token, `src/lib.rs:20-32`) and its declaration marks nothing `#[sensitive]`. Withheld by
//! the SDK (README:121-123, `src/error.rs:73-87`): the provider error `message` and every
//! response header value. So the test puts one control canary in an ordinary string value it
//! sends, the `providers` filter of both operations (a query value), and the server echoes that
//! canary into the withheld places: the 4xx/5xx `message`, headers (`x-private`, `set-cookie`,
//! `retry-after`) and every malformed, wrong-media and truncated body. `latest_pair` and
//! `historical_pair` (both reads; the SDK has no writes) cover the seven scenarios through the
//! SDK `send()` and the native `Prepared`.
//! Asserted not to print the canary: the client, `RateResponse`, `ResponseMeta`, the SDK
//! `Error` and its source chain, `ProviderError`, `ErrorBodyError`, `concord::Error`, and the
//! Concord response head. Not asserted: the generated endpoint and `Prepared` `Debug`, which show
//! the unmarked `providers` query value by design (Concord prints ordinary values; checked here
//! to contain it); native reqwest values; explicit accessors (`ProviderError::document`'s
//! `Problem`, whose derived `Debug` is the unredacted access, and `meta.headers()`); `Rate`,
//! returned in clear (the success body carries no canary). No wire exemptions.

use concord_check_canary::Canaries;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use std::time::Duration;

use concord_sdk_frankfurter::{Date, Error, Frankfurter, RateResponse};

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
    concord_example_testserver::refused_url()
}

// Echo the canary into headers and into every body whose content the SDK withholds.
fn reply(case: usize, success: &str, problem: &str, echo: &str) -> Reply {
    let response = match case {
        0 | 6 => Reply::json(200, success),
        1 => Reply::json(422, problem),
        2 => Reply::json(503, problem),
        3 => Reply::json(200, format!("{{invalid {echo}")),
        4 => Reply::text(200, "text/html", echo),
        5 => Reply::json(200, "{}").with_body(Body::FailAfter(vec![concord::bytes::Bytes::from(
            format!("{{\"incomplete\":\"{echo}"),
        )])),
        _ => unreachable!(),
    };
    response
        .header("x-private", echo)
        .header("set-cookie", echo)
        .header("retry-after", echo)
}

async fn receive<T>(c: &mut Canaries, prepared: concord::Prepared<T>) -> Result<T, concord::Error> {
    let result = prepared.receive().await;
    c.assert_debug_absent("Received Result", &result);
    match result {
        Ok(received) => {
            c.assert_debug_absent("response head", &received.head());
            c.assert_debug_absent("response metadata", &received.meta());
            received.decode().await
        }
        Err(error) => Err(error),
    }
}

fn concord_error(c: &mut Canaries, error: &concord::Error) {
    c.assert_debug_absent("concord::Error", error);
    c.assert_error_absent("concord::Error and sources", error);
    c.assert_debug_absent("error kind", &error.kind());
    if let Some(meta) = error.head_meta() {
        c.assert_debug_absent("error response metadata", &meta);
    }
}

fn error(c: &mut Canaries, error: &Error) {
    c.assert_debug_absent("SDK error", error);
    c.assert_error_absent("SDK error and sources", error);
    c.assert_debug_absent("error metadata", &error.meta());
    c.assert_debug_absent("error status", &error.status());
    c.assert_debug_absent("error operation", &error.operation());
    match error {
        Error::Concord { source, meta } => {
            concord_error(c, source);
            c.assert_debug_absent("Concord error metadata", meta);
        }
        Error::Provider(provider) => {
            c.assert_debug_absent("provider error", provider);
            c.assert_display_absent("provider error", provider);
            c.assert_error_absent("provider error and sources", provider);
            c.assert_debug_absent("provider metadata", &provider.meta);
            if let Err(e) = &provider.document {
                c.assert_debug_absent("error body error", e);
                c.assert_error_absent("error body error and sources", e);
            }
        }
    }
}

fn result(c: &mut Canaries, label: &str, result: &Result<RateResponse, Error>) {
    match result {
        Ok(response) => {
            c.assert_debug_absent(label, response);
            c.assert_debug_absent("rate metadata", &response.meta);
        }
        Err(e) => error(c, e),
    }
}

#[tokio::test]
async fn nothing_prints_the_control_canary() {
    let mut c = Canaries::new("frankfurter");
    let providers = c.issue();
    let date = Date::from_ymd_opt(2024, 1, 15).unwrap();
    for case in 0..7 {
        let echo = providers.clone();
        let server = TestServer::start(move |r: &Recorded| {
            let success = if r.query().is_some_and(|q| q.contains("date=")) {
                r#"{"date":"2024-01-15","base":"EUR","quote":"USD","rate":1.089}"#
            } else {
                r#"{"date":"2026-03-25","base":"EUR","quote":"USD","rate":1.1568}"#
            };
            let problem = format!(r#"{{"status":422,"message":"{echo}"}}"#);
            reply(case, success, &problem, &echo)
        })
        .await;
        let base = if case == 6 { refused() } else { server.url() };
        let api = Frankfurter::new(http()).with_base_url(&base).unwrap();
        c.assert_debug_absent("client", &api);
        let read = api
            .latest_pair("EUR", "USD")
            .set_providers(providers.clone());
        let historical = api
            .historical_pair("EUR", "USD", date)
            .set_providers(providers.clone());
        // Ordinary query value: the generated endpoint and Prepared show it by design.
        let prepared = read.clone().prepare().unwrap();
        assert!(format!("{read:?}{prepared:?}").contains(&providers));
        let native = receive(&mut c, prepared).await;
        c.assert_debug_absent("native Result", &native);
        assert_eq!(native.is_ok(), case == 0);
        if let Err(e) = &native {
            concord_error(&mut c, e);
        }

        let latest = read.send().await;
        assert_eq!(latest.is_ok(), case == 0);
        result(&mut c, "latest Result", &latest);
        let past = historical.send().await;
        assert_eq!(past.is_ok(), case == 0);
        result(&mut c, "historical Result", &past);
        match (case, &latest) {
            (0, Ok(response)) => assert_eq!(response.rate.base, "EUR"),
            // Safe diagnosis survives: status, operation and the withheld message on purpose.
            (1, Err(e @ Error::Provider(provider))) => {
                assert_eq!(e.to_string(), "Frankfurter HTTP 422 Unprocessable Entity");
                assert_eq!(e.status().map(|s| s.as_u16()), Some(422));
                assert!(format!("{:?}", e.operation()).contains("latest_pair"));
                assert_eq!(provider.document.as_ref().unwrap().message, providers);
            }
            (3, Err(e @ Error::Concord { source, .. })) => {
                assert_eq!(source.kind(), concord::ErrorKind::Decode);
                assert_eq!(e.status().map(|s| s.as_u16()), Some(200));
            }
            (6, Err(e)) => assert_eq!(e.status(), None),
            (n, other) => assert!(matches!(n, 2 | 4 | 5), "case {n}: {other:?}"),
        }
        if case == 0 {
            c.assert_on_wire(&server.requests());
            for r in server.requests() {
                assert_eq!(r.query_all("providers"), [providers.as_str()]);
            }
        }
        server.shutdown().await;
    }
    assert!(c.checked() >= 150);
    println!(
        "frankfurter: {} renderings; positive control passed",
        c.checked()
    );
}

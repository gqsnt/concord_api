//! Independent first-use suite: an application reads latest (A) and historical (B) Frankfurter
//! v2 pair rates through the SDK's public API, against a local fake of the provider written from
//! the public documentation (frankfurter.dev v2 "Rate"/"Errors" and the linked OpenAPI 2.1.1
//! `getRate` operation). Predictions and the adoption trace are in `FIRST_USE.md`; fixture
//! provenance is listed there too.

use std::time::Duration;

use concord::{ErrorKind, reqwest};
use concord_example_testserver::{Body, Recorded, Reply, Script, TestServer};
use concord_sdk_frankfurter::{
    Date, Decimal, ERROR_BODY_LIMIT, Error, ErrorBodyError, Frankfurter, Problem, ProviderError,
    RateResponse, ResponseMeta,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const LATEST_EUR_USD: &[u8] = include_bytes!("../fixtures/first-use/latest-eur-usd.json");
const IDENTITY_EUR_EUR: &[u8] = include_bytes!("../fixtures/first-use/identity-eur-eur.json");
const PRECISE_EUR_JPY: &[u8] = include_bytes!("../fixtures/first-use/precise-eur-jpy.json");
const EXPANDED_EUR_USD: &[u8] = include_bytes!("../fixtures/first-use/expanded-eur-usd.json");
const INVALID_CURRENCY_422: &[u8] =
    include_bytes!("../fixtures/first-use/invalid-currency-422.json");
const NOT_FOUND_404: &[u8] = include_bytes!("../fixtures/first-use/not-found-404.json");
const UNAVAILABLE_503: &[u8] = include_bytes!("../fixtures/first-use/unavailable-503.json");
const HISTORICAL_2024_01_15: &[u8] =
    include_bytes!("../fixtures/first-use/historical-eur-usd-2024-01-15.json");
const HISTORICAL_2024_01_12: &[u8] =
    include_bytes!("../fixtures/first-use/historical-eur-usd-2024-01-12.json");

/// Currencies the fake knows. The real provider covers far more; only the error path matters.
const KNOWN: &[&str] = &["EUR", "USD", "JPY", "GBP", "CHF"];

// ---------------------------------------------------------------------------------------------
// The fake provider
// ---------------------------------------------------------------------------------------------

/// A table-driven stand-in for `GET /v2/rate/{base}/{quote}[?date=..][&providers=..]`.
///
/// Documented behaviour it mimics: JSON `Rate` object on 200, the website's
/// `{"status":422,"message":"invalid currency: ABC"}` envelope for an unknown currency, and the
/// OpenAPI `NotFound` message-only body. Case-insensitive currency matching follows the
/// website's lower-case example URL (`/v2/rate/eur/usd`). Everything else is synthetic.
fn fake_frankfurter(request: &Recorded) -> Reply {
    if request.method != "GET" {
        return Reply::json(405, r#"{"message":"first-use synthetic: method"}"#);
    }
    let segments: Vec<&str> = request.path().split('/').collect();
    let ["", "v2", "rate", base, quote] = segments.as_slice() else {
        return Reply::json(404, NOT_FOUND_404);
    };
    for code in [base, quote] {
        if !KNOWN.contains(&code.to_ascii_uppercase().as_str()) {
            return Reply::json(
                422,
                format!(r#"{{"status":422,"message":"invalid currency: {code}"}}"#),
            );
        }
    }
    let pair = (base.to_ascii_uppercase(), quote.to_ascii_uppercase());
    let pair = (pair.0.as_str(), pair.1.as_str());
    let dates = request.query_all("date");
    let body: Option<&[u8]> = match dates.as_slice() {
        [] => match pair {
            ("EUR", "USD") => Some(LATEST_EUR_USD),
            ("EUR", "EUR") => Some(IDENTITY_EUR_EUR),
            ("EUR", "JPY") => Some(PRECISE_EUR_JPY),
            _ => None,
        },
        [date] => historical_body(pair, date),
        _ => {
            return Reply::json(
                400,
                r#"{"status":400,"message":"first-use synthetic: date"}"#,
            );
        }
    };
    match body {
        Some(body) => Reply::json(200, body).header("x-fake", "frankfurter"),
        None => Reply::json(404, NOT_FOUND_404),
    }
}

/// Historical rows of the fake. The weekend rows answering with the preceding Friday are a
/// synthetic scenario: they show that the SDK returns the provider's date, nothing more.
fn historical_body(pair: (&str, &str), date: &str) -> Option<&'static [u8]> {
    match (pair, date) {
        (("EUR", "USD"), "2024-01-15") => Some(HISTORICAL_2024_01_15),
        (("EUR", "USD"), "2024-01-12" | "2024-01-13" | "2024-01-14") => Some(HISTORICAL_2024_01_12),
        _ => None,
    }
}

/// The application's client, pointed at a loopback server.
fn local(server: &TestServer) -> Frankfurter {
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("reqwest client");
    Frankfurter::new(http)
        .with_base_url(server.url())
        .expect("loopback base URL")
}

fn decimal(text: &str) -> Decimal {
    text.parse().expect("test decimal")
}

fn provider_error(result: Result<RateResponse, Error>) -> Box<ProviderError> {
    match result {
        Err(Error::Provider(error)) => error,
        other => panic!("expected Error::Provider, got {other:?}"),
    }
}

fn concord_error(
    result: Result<RateResponse, Error>,
) -> (concord::Error, Option<Box<ResponseMeta>>) {
    match result {
        Err(Error::Concord { source, meta }) => (source, meta),
        other => panic!("expected Error::Concord, got {other:?}"),
    }
}

/// What an ordinary GET to the pair endpoint must look like on the wire.
fn assert_plain_get(request: &Recorded, target: &str) {
    assert_eq!(request.method, "GET");
    assert_eq!(request.target, target);
    assert_eq!(request.header_all("accept"), ["application/json"]);
    assert_eq!(request.body_len, 0);
    assert!(!request.chunked);
    for absent in [
        "content-type",
        "authorization",
        "cookie",
        "x-api-key",
        "apikey",
    ] {
        assert_eq!(request.header(absent), None, "unexpected {absent} header");
    }
}

async fn shut_down(server: TestServer) {
    let cleanup = server.shutdown().await;
    assert_eq!(cleanup.open_after, 0, "{cleanup:?}");
}

// ---------------------------------------------------------------------------------------------
// Outcome A: latest pair rate
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_latest_pair_returns_the_documented_observation() -> TestResult {
    let server = TestServer::start(fake_frankfurter).await;
    let api = local(&server);

    let latest = api.latest_pair("EUR", "USD").send().await?;

    assert_eq!(latest.rate.date, "2026-03-25".parse()?);
    assert_eq!(latest.rate.base, "EUR");
    assert_eq!(latest.rate.quote, "USD");
    assert_eq!(latest.rate.rate, decimal("1.1568"));
    assert_eq!(latest.meta.status, reqwest::StatusCode::OK);
    assert_eq!(
        latest.meta.headers.get("x-fake").map(|v| v.as_bytes()),
        Some(&b"frankfurter"[..])
    );
    let operation = latest.meta.operation.expect("operation identity");
    assert_eq!(operation.name(), "Frankfurter.latest_pair");
    assert_eq!(operation.method(), Some("GET"));

    assert_plain_get(&server.single(), "/v2/rate/EUR/USD");
    shut_down(server).await;
    Ok(())
}

#[tokio::test]
async fn a_currency_text_is_sent_as_supplied() -> TestResult {
    let server = TestServer::start(fake_frankfurter).await;
    let api = local(&server);

    // Lower-case is not normalized by the SDK; the provider answers with its own spelling.
    let latest = api.latest_pair("eur", "usd").send().await?;
    assert_eq!(
        (latest.rate.base.as_str(), latest.rate.quote.as_str()),
        ("EUR", "USD")
    );

    // A slash stays inside one path segment, so it cannot reach another route.
    let error = provider_error(api.latest_pair("A/B", "USD").send().await);
    assert_eq!(error.meta.status, reqwest::StatusCode::UNPROCESSABLE_ENTITY);

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_plain_get(&requests[0], "/v2/rate/eur/usd");
    assert_plain_get(&requests[1], "/v2/rate/A%2FB/USD");
    shut_down(server).await;
    Ok(())
}

#[tokio::test]
async fn a_providers_filter_is_one_query_value() -> TestResult {
    let server = TestServer::start(fake_frankfurter).await;
    let api = local(&server);

    let filtered = api
        .latest_pair("EUR", "USD")
        .set_providers("ECB,TCMB")
        .send()
        .await?;
    let unfiltered = api
        .latest_pair("EUR", "USD")
        .maybe_providers(None)
        .send()
        .await?;
    assert_eq!(filtered.rate, unfiltered.rate);

    let requests = server.requests();
    assert_eq!(requests[0].path(), "/v2/rate/EUR/USD");
    assert_eq!(requests[0].query(), Some("providers=ECB%2CTCMB"));
    assert_eq!(
        requests[0].query_pairs(),
        [("providers".to_owned(), "ECB,TCMB".to_owned())]
    );
    assert_eq!(requests[1].target, "/v2/rate/EUR/USD");
    shut_down(server).await;
    Ok(())
}

#[tokio::test]
async fn a_rate_is_an_exact_decimal() -> TestResult {
    let server = TestServer::start(fake_frankfurter).await;
    let api = local(&server);

    let jpy = api.latest_pair("EUR", "JPY").send().await?;
    let exact = decimal("172.345678901234567890123456789");
    assert_eq!(jpy.rate.rate, exact);
    // The same token through f64 loses digits, so equality above rules out a float detour.
    let via_float: f64 = "172.345678901234567890123456789".parse()?;
    assert_ne!(decimal(&via_float.to_string()), exact);

    // The documented identity rate of a same-currency pair is the JSON integer 1.
    let identity = api.latest_pair("EUR", "EUR").send().await?;
    assert_eq!(identity.rate.rate, decimal("1"));
    shut_down(server).await;
    Ok(())
}

#[tokio::test]
async fn a_unknown_and_optional_fields_are_ignored() -> TestResult {
    // The schema's optional `providers` array (present with `expand=providers`) plus nesting.
    let server = TestServer::start(|_| Reply::json(200, EXPANDED_EUR_USD)).await;
    let latest = local(&server).latest_pair("EUR", "USD").send().await?;
    assert_eq!(latest.rate.rate, decimal("1.1568"));
    assert_eq!(latest.rate.date, "2026-03-25".parse()?);
    shut_down(server).await;
    Ok(())
}

#[tokio::test]
async fn a_self_hosted_prefix_is_kept() -> TestResult {
    let server = TestServer::start(|_| Reply::json(200, LATEST_EUR_USD)).await;
    let api =
        Frankfurter::new(reqwest::Client::new()).with_base_url(format!("{}/fx", server.url()))?;
    api.latest_pair("EUR", "USD").send().await?;
    assert_plain_get(&server.single(), "/fx/v2/rate/EUR/USD");
    shut_down(server).await;
    Ok(())
}

#[tokio::test]
async fn a_documented_provider_errors_are_typed() -> TestResult {
    let server = TestServer::start(|request: &Recorded| match request.path() {
        "/v2/rate/EUR/ABC" => Reply::json(422, INVALID_CURRENCY_422),
        "/v2/rate/EUR/GBP" => Reply::json(404, NOT_FOUND_404),
        _ => Reply::json(503, UNAVAILABLE_503).header("retry-after", "7"),
    })
    .await;
    let api = local(&server);

    let result = api.latest_pair("EUR", "ABC").send().await;
    let shown = match &result {
        Err(error) => (format!("{error}"), format!("{error:?}")),
        Ok(_) => panic!("expected an error"),
    };
    let invalid = provider_error(result);
    assert_eq!(
        invalid.meta.status,
        reqwest::StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        invalid.document.as_ref().ok(),
        Some(&Problem {
            status: Some(422),
            message: "invalid currency: ABC".to_owned()
        })
    );
    assert_eq!(
        invalid.meta.operation.map(|o| o.name()),
        Some("Frankfurter.latest_pair")
    );
    // Logging an error does not print the provider's (untrusted) message text.
    assert!(
        !shown.0.contains("invalid currency"),
        "Display: {}",
        shown.0
    );
    assert!(!shown.1.contains("invalid currency"), "Debug: {}", shown.1);

    let missing = provider_error(api.latest_pair("EUR", "GBP").send().await);
    assert_eq!(missing.meta.status, reqwest::StatusCode::NOT_FOUND);
    let problem = missing.document.as_ref().expect("message-only document");
    assert_eq!(problem.status, None);
    assert!(problem.message.starts_with("first-use synthetic"));

    let busy = provider_error(api.latest_pair("EUR", "CHF").send().await);
    assert_eq!(busy.meta.status, reqwest::StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        busy.meta.headers.get("retry-after").map(|v| v.as_bytes()),
        Some(&b"7"[..])
    );
    assert!(busy.document.is_ok());
    shut_down(server).await;
    Ok(())
}

#[tokio::test]
async fn a_unusable_error_documents_keep_the_status() -> TestResult {
    let oversized = format!(r#"{{"message":"{}"}}"#, "x".repeat(ERROR_BODY_LIMIT));
    let server = TestServer::start(move |request: &Recorded| match request.path() {
        "/v2/rate/EUR/HTML" => Reply::text(502, "text/html", "<html>bad gateway</html>"),
        "/v2/rate/EUR/BROKEN" => Reply::json(422, r#"{"status":422,"message":"#),
        "/v2/rate/EUR/BIG" => Reply::json(422, oversized.as_bytes()),
        _ => Reply::new(503)
            .header("content-type", "application/json")
            .with_body(Body::FailAfter(vec![
                b"{\"message\":\"dead".to_vec().into(),
            ])),
    })
    .await;
    let api = local(&server);

    let html = provider_error(api.latest_pair("EUR", "HTML").send().await);
    assert_eq!(html.meta.status, reqwest::StatusCode::BAD_GATEWAY);
    assert!(
        matches!(html.document, Err(ErrorBodyError::Metadata(_))),
        "{:?}",
        html.document
    );

    let broken = provider_error(api.latest_pair("EUR", "BROKEN").send().await);
    assert_eq!(
        broken.meta.status,
        reqwest::StatusCode::UNPROCESSABLE_ENTITY
    );
    assert!(
        matches!(broken.document, Err(ErrorBodyError::Decode(_))),
        "{:?}",
        broken.document
    );

    let big = provider_error(api.latest_pair("EUR", "BIG").send().await);
    assert_eq!(big.meta.status, reqwest::StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        matches!(big.document, Err(ErrorBodyError::Read(_))),
        "{:?}",
        big.document
    );

    let cut = provider_error(api.latest_pair("EUR", "CUT").send().await);
    assert_eq!(cut.meta.status, reqwest::StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        matches!(cut.document, Err(ErrorBodyError::Read(_))),
        "{:?}",
        cut.document
    );
    shut_down(server).await;
    Ok(())
}

#[tokio::test]
async fn a_success_body_cut_off_yields_no_rate() -> TestResult {
    let prefix = LATEST_EUR_USD[..40].to_vec();
    let server = TestServer::start(move |_| {
        Reply::new(200)
            .header("content-type", "application/json")
            .header("x-fake", "cut")
            .with_body(Body::FailAfter(vec![prefix.clone().into()]))
    })
    .await;

    let (source, meta) = concord_error(local(&server).latest_pair("EUR", "USD").send().await);
    assert_eq!(source.kind(), ErrorKind::Transport, "{source}");
    let meta = meta.expect("the 200 head arrived before the body broke");
    assert_eq!(meta.status, reqwest::StatusCode::OK);
    assert_eq!(
        meta.headers.get("x-fake").map(|v| v.as_bytes()),
        Some(&b"cut"[..])
    );
    shut_down(server).await;
    Ok(())
}

#[tokio::test]
async fn a_stalled_success_body_ends_at_the_transport_timeout() -> TestResult {
    let server = TestServer::start(|_| {
        Reply::new(200)
            .header("content-type", "application/json")
            .with_body(Body::Stall)
    })
    .await;
    let http = reqwest::Client::builder()
        .timeout(Duration::from_millis(300))
        .build()?;
    let api = Frankfurter::new(http).with_base_url(server.url())?;

    let started = std::time::Instant::now();
    let result = api.latest_pair("EUR", "USD").send().await;
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "{:?}",
        started.elapsed()
    );
    let (source, meta) = concord_error(result);
    assert_eq!(source.kind(), ErrorKind::Transport, "{source}");
    assert_eq!(meta.map(|m| m.status), Some(reqwest::StatusCode::OK));
    shut_down(server).await;
    Ok(())
}

#[tokio::test]
async fn a_wrong_media_type_is_refused_and_recoverable() -> TestResult {
    let server = TestServer::start(|_| Reply::text(200, "text/html", LATEST_EUR_USD)).await;

    let (source, meta) = concord_error(local(&server).latest_pair("EUR", "USD").send().await);
    assert_eq!(source.kind(), ErrorKind::Metadata, "{source}");
    assert_eq!(meta.map(|m| m.status), Some(reqwest::StatusCode::OK));
    let response = source.into_response().expect("unread response");
    assert_eq!(response.bytes().await?.as_ref(), LATEST_EUR_USD);
    shut_down(server).await;
    Ok(())
}

#[tokio::test]
async fn a_invalid_success_documents_are_decode_errors() -> TestResult {
    let cases: [(&str, &str); 7] = [
        (
            "missing rate",
            r#"{"date":"2026-03-25","base":"EUR","quote":"USD"}"#,
        ),
        (
            "quoted rate",
            r#"{"date":"2026-03-25","base":"EUR","quote":"USD","rate":"1.1568"}"#,
        ),
        (
            "zero rate",
            r#"{"date":"2026-03-25","base":"EUR","quote":"USD","rate":0}"#,
        ),
        (
            "negative rate",
            r#"{"date":"2026-03-25","base":"EUR","quote":"USD","rate":-1.1}"#,
        ),
        (
            "bad date",
            r#"{"date":"2026-02-30","base":"EUR","quote":"USD","rate":1.1568}"#,
        ),
        (
            "trailing bytes",
            r#"{"date":"2026-03-25","base":"EUR","quote":"USD","rate":1.1568}{}"#,
        ),
        (
            "v1-style envelope",
            r#"{"amount":1.0,"base":"EUR","date":"2026-03-25","rates":{"USD":1.1568}}"#,
        ),
    ];
    for (name, body) in cases {
        let server = TestServer::start(move |_| Reply::json(200, body)).await;
        let (source, meta) = concord_error(local(&server).latest_pair("EUR", "USD").send().await);
        assert_eq!(source.kind(), ErrorKind::Decode, "{name}: {source}");
        assert_eq!(
            meta.map(|m| m.status),
            Some(reqwest::StatusCode::OK),
            "{name}"
        );
        shut_down(server).await;
    }
    Ok(())
}

#[tokio::test]
async fn a_connection_closed_before_a_head_has_no_status() -> TestResult {
    let server = TestServer::start(|_| Reply::hang_up()).await;
    let result = local(&server).latest_pair("EUR", "USD").send().await;
    let error = result.as_ref().expect_err("no answer");
    assert_eq!(error.status(), None);
    assert!(error.meta().is_none());
    assert_eq!(
        error.operation().map(|o| o.name()),
        Some("Frankfurter.latest_pair")
    );
    let (source, meta) = concord_error(result);
    assert_eq!(source.kind(), ErrorKind::Transport, "{source}");
    assert!(meta.is_none());
    shut_down(server).await;
    Ok(())
}

#[tokio::test]
async fn a_empty_currency_is_rejected_before_sending() -> TestResult {
    let server = TestServer::start(fake_frankfurter).await;
    let (source, meta) = concord_error(local(&server).latest_pair("", "USD").send().await);
    assert_eq!(source.kind(), ErrorKind::Build, "{source}");
    assert!(meta.is_none());
    assert!(server.requests().is_empty());
    shut_down(server).await;
    Ok(())
}

/// How an application would build the client from configuration: one error type throughout.
fn connect(base_url: &str) -> Result<Frankfurter, Error> {
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("reqwest client");
    Ok(Frankfurter::new(http).with_base_url(base_url)?)
}

#[tokio::test]
async fn a_base_url_is_validated_as_configuration() -> TestResult {
    let server = TestServer::start(fake_frankfurter).await;
    let api = connect(&server.url())?;
    api.latest_pair("EUR", "USD").send().await?;

    let refused = connect(&format!("{}/?region=eu", server.url()));
    let error = refused
        .as_ref()
        .expect_err("a query in the base URL is refused");
    assert!(error.meta().is_none());
    assert_eq!(error.operation(), None);
    let Err(Error::Concord { source, .. }) = refused else {
        unreachable!()
    };
    assert_eq!(source.kind(), ErrorKind::Build, "{source}");
    assert_eq!(server.requests().len(), 1);
    shut_down(server).await;
    Ok(())
}

#[tokio::test]
async fn a_prepared_send_skips_the_typed_error_adapter() -> TestResult {
    // Between waves 2-3 (coordinator integration): the client declares `send SendPolicy;`, so
    // the generated `send()` is the typed adapter (formerly `fetch()`). The Concord-only path
    // this test checked is now `prepare()?.send()`; its expected values are unchanged.
    let server = TestServer::start(fake_frankfurter).await;
    let api = local(&server);

    let rate = api.latest_pair("EUR", "USD").prepare()?.send().await?;
    assert_eq!(rate, api.latest_pair("EUR", "USD").send().await?.rate);

    let typed = provider_error(api.latest_pair("EUR", "ABC").send().await);
    assert_eq!(typed.meta.status, reqwest::StatusCode::UNPROCESSABLE_ENTITY);

    let error = api
        .latest_pair("EUR", "ABC")
        .prepare()?
        .send()
        .await
        .expect_err("422");
    assert_eq!(error.kind(), ErrorKind::Status);
    assert_eq!(
        error.status(),
        Some(reqwest::StatusCode::UNPROCESSABLE_ENTITY)
    );
    assert_eq!(error.endpoint(), Some("Frankfurter.latest_pair"));
    let body = error
        .into_response()
        .expect("unread response")
        .bytes()
        .await?;
    // The fake formats this body itself (no final LF, unlike the fixture file).
    assert_eq!(
        body.as_ref(),
        br#"{"status":422,"message":"invalid currency: ABC"}"#
    );
    shut_down(server).await;
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Outcome B: historical pair rate
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn b_historical_pair_sends_the_date_and_returns_the_observation() -> TestResult {
    let server = TestServer::start(fake_frankfurter).await;
    let api = local(&server);

    let date: Date = "2024-01-15".parse()?;
    let historical = api.historical_pair("EUR", "USD", date).send().await?;

    assert_eq!(historical.rate.date, date);
    assert_eq!(
        (
            historical.rate.base.as_str(),
            historical.rate.quote.as_str()
        ),
        ("EUR", "USD")
    );
    assert_eq!(historical.rate.rate, decimal("1.0942"));
    assert_eq!(historical.meta.status, reqwest::StatusCode::OK);
    let operation = historical.meta.operation.expect("operation identity");
    assert_eq!(operation.name(), "Frankfurter.historical_pair");
    assert_eq!(operation.method(), Some("GET"));

    let request = server.single();
    assert_plain_get(&request, "/v2/rate/EUR/USD?date=2024-01-15");
    assert_eq!(request.query_all("date"), ["2024-01-15"]);
    shut_down(server).await;
    Ok(())
}

#[tokio::test]
async fn b_the_returned_date_is_the_providers() -> TestResult {
    let server = TestServer::start(fake_frankfurter).await;
    let saturday: Date = "2024-01-13".parse()?;

    let historical = local(&server)
        .historical_pair("EUR", "USD", saturday)
        .send()
        .await?;

    assert_eq!(historical.rate.date, "2024-01-12".parse::<Date>()?);
    assert_ne!(historical.rate.date, saturday);
    // `1.0950` on the wire; trailing zeroes do not change the value.
    assert_eq!(historical.rate.rate, decimal("1.095"));
    assert_plain_get(&server.single(), "/v2/rate/EUR/USD?date=2024-01-13");
    shut_down(server).await;
    Ok(())
}

#[tokio::test]
async fn b_providers_filter_follows_the_date() -> TestResult {
    let server = TestServer::start(fake_frankfurter).await;
    let date: Date = "2024-01-15".parse()?;

    let filtered = local(&server)
        .historical_pair("EUR", "USD", date)
        .set_providers("ECB")
        .send()
        .await?;

    assert_eq!(filtered.rate.date, date);
    assert_plain_get(
        &server.single(),
        "/v2/rate/EUR/USD?date=2024-01-15&providers=ECB",
    );
    shut_down(server).await;
    Ok(())
}

#[tokio::test]
async fn b_prepare_shows_the_request_without_sending() -> TestResult {
    let server = TestServer::start(fake_frankfurter).await;
    let api = local(&server);
    let first: Date = "2024-01-15".parse()?;
    let second: Date = "2024-01-12".parse()?;

    let request = api.historical_pair("EUR", "USD", first);
    let prepared = request.clone().set_date(second).prepare()?;
    assert_eq!(prepared.request().method(), reqwest::Method::GET);
    assert_eq!(
        prepared.request().url().as_str(),
        format!("{}/v2/rate/EUR/USD?date=2024-01-12", server.url())
    );
    // The original request value keeps its own date.
    let original = request.prepare()?;
    assert_eq!(original.request().url().query(), Some("date=2024-01-15"));
    assert!(server.requests().is_empty(), "prepare() must not send");
    shut_down(server).await;
    Ok(())
}

#[test]
fn b_impossible_calendar_dates_fail_in_the_application() {
    assert!("2024-02-30".parse::<Date>().is_err());
    assert!("2024-13-01".parse::<Date>().is_err());
    assert!("15/01/2024".parse::<Date>().is_err());
    assert!("2024-02-29".parse::<Date>().is_ok());
}

#[tokio::test]
async fn b_dates_and_currencies_without_data_are_provider_errors() -> TestResult {
    let server = TestServer::start(fake_frankfurter).await;
    let api = local(&server);

    let before_series: Date = "1998-12-31".parse()?;
    let missing = provider_error(
        api.historical_pair("EUR", "USD", before_series)
            .send()
            .await,
    );
    assert_eq!(missing.meta.status, reqwest::StatusCode::NOT_FOUND);
    // `ErrorBodyError` is not `PartialEq` (E0369 on the first attempt), so compare the `Ok` side.
    assert_eq!(missing.document.as_ref().ok().map(|p| p.status), Some(None));
    assert_eq!(
        missing.meta.operation.map(|o| o.name()),
        Some("Frankfurter.historical_pair")
    );

    let date: Date = "2024-01-15".parse()?;
    let invalid = provider_error(api.historical_pair("EUR", "XYZ", date).send().await);
    assert_eq!(
        invalid.meta.status,
        reqwest::StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        invalid.document.as_ref().ok(),
        Some(&Problem {
            status: Some(422),
            message: "invalid currency: XYZ".to_owned()
        })
    );

    let requests = server.requests();
    assert_plain_get(&requests[0], "/v2/rate/EUR/USD?date=1998-12-31");
    assert_plain_get(&requests[1], "/v2/rate/EUR/XYZ?date=2024-01-15");
    shut_down(server).await;
    Ok(())
}

#[tokio::test]
async fn b_one_fetch_is_one_attempt_and_a_clone_can_be_resent() -> TestResult {
    let script = Script::new([
        Reply::new(200)
            .header("content-type", "application/json")
            .with_body(Body::FailAfter(vec![
                HISTORICAL_2024_01_15[..30].to_vec().into(),
            ])),
        Reply::json(200, HISTORICAL_2024_01_15),
    ]);
    let replies = script.clone();
    let server = TestServer::start(move |_| replies.next()).await;
    let date: Date = "2024-01-15".parse()?;
    let request = local(&server).historical_pair("EUR", "USD", date);

    // The cut-off body is one failed attempt with its head kept; the SDK does not retry.
    let (source, meta) = concord_error(request.clone().send().await);
    assert_eq!(source.kind(), ErrorKind::Transport, "{source}");
    assert_eq!(meta.map(|m| m.status), Some(reqwest::StatusCode::OK));
    assert_eq!(server.requests().len(), 1);
    assert_eq!(script.remaining(), 1);

    // Retrying is the application's decision: the cloned request sends the same thing again.
    let retried = request.send().await?;
    assert_eq!(retried.rate.rate, decimal("1.0942"));
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].target, requests[1].target);
    assert_eq!(requests[0].headers, requests[1].headers);
    shut_down(server).await;
    Ok(())
}

#[tokio::test]
async fn b_latest_and_historical_share_one_client_concurrently() -> TestResult {
    let server = TestServer::start(fake_frankfurter).await;
    let api = local(&server);
    let worker = api.clone();
    let date: Date = "2024-01-15".parse()?;

    let (latest, historical) = tokio::join!(
        api.latest_pair("EUR", "USD").send(),
        worker.historical_pair("EUR", "USD", date).send(),
    );
    assert_eq!(latest?.rate.date, "2026-03-25".parse::<Date>()?);
    assert_eq!(historical?.rate.date, date);

    let mut targets: Vec<String> = server.requests().into_iter().map(|r| r.target).collect();
    targets.sort();
    assert_eq!(
        targets,
        ["/v2/rate/EUR/USD", "/v2/rate/EUR/USD?date=2024-01-15"]
    );
    shut_down(server).await;
    Ok(())
}

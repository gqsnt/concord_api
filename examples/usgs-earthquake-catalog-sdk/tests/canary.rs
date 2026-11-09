//! Secrets: the public catalog has no credential input. Its one withheld input is the lookup's
//! event ID (`#[sensitive] query eventid`, `src/lib.rs:66`; README: "SDK and Concord
//! diagnostics withhold provider bodies, response header values and the lookup request's event
//! ID"), which gets its own canary and travels in the query. The server echoes that canary into
//! response headers, into the 4xx/5xx provider text (`HttpFailure` Debug withholds it,
//! `src/error.rs:79`; "may contain echoed secrets") and into every malformed, wrong-media and
//! truncated body. `catalog.event` (generated `send()` and the native `Prepared`) is the
//! representative read and covers the seven scenarios; the window's GeoJSON page, CSV page
//! (streamed rows and completion), `pages` traversal and CSV `export` run beside it on the same
//! replies. The SDK has no writes. An event ID refused by `EventId::new` is checked separately.
//! Not covered: native reqwest values; explicit accessors (`document_unredacted`,
//! `ResponseMeta::headers`, `EventId::as_str`); explicit model formatting, which the README
//! says exposes data (`EventId`'s `Display`, `Feature`, `CsvRow`); windows,
//! magnitudes and returned event IDs are public. `EventId`'s own `Debug` withholding the value
//! (the declaration marks it `#[sensitive]`) is checked at the end of this file. No wire
//! exemptions.

use concord_check_canary::Canaries;
use concord_sdk_usgs_earthquake_catalog::{
    Error, EventId, Magnitude, PageSize, Usgs, Window, concord, concord::reqwest,
};
use concord_testserver::{Body, Recorded, Reply, TestServer};
use std::{
    num::{NonZeroU64, NonZeroUsize},
    time::Duration,
};

const EVENT: &str = r#"{"type":"Feature","id":"synthetic-a","properties":{"mag":4.5,"place":"10 km west of Example","time":1388538000000,"updated":1388538600000,"status":"reviewed","type":"earthquake"},"geometry":{"type":"Point","coordinates":[-122.5,37.5,8.25]}}"#;
const CSV: &str = "time,latitude,longitude,depth,mag,magType,nst,gap,dmin,rms,net,id,updated,place,type,horizontalError,depthError,magError,magNst,status,locationSource,magSource\n2014-01-01T01:00:00Z,37.5,-122.5,8.25,4.5,mw,10,40,0.1,0.2,xx,synthetic-a,2014-01-01T01:10:00Z,\"10 km west of Example\",earthquake,1,2,0.1,8,reviewed,xx,xx\n";

fn refused() -> String {
    concord_testserver::refused_url()
}

fn client(base: &str) -> Usgs {
    let http = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    Usgs::for_fixture(http, base).unwrap()
}

// Echo the event ID in headers and in provider text whose formatting is withheld.
fn reply(case: usize, success: Reply, echo: &str) -> Reply {
    let problem = format!("Synthetic service refusal. Submitted URI: /query?eventid={echo}");
    let response = match case {
        0 | 6 => success,
        1 => Reply::text(400, "text/plain", problem),
        2 => Reply::text(503, "text/plain", problem),
        3 => Reply::json(200, format!("{{invalid {echo}")),
        4 => Reply::text(200, "text/html", format!("<html>{echo}</html>")),
        5 => Reply::json(200, "{}").with_body(Body::FailAfter(vec![concord::bytes::Bytes::from(
            format!("{{\"incomplete\":\"{echo}"),
        )])),
        _ => unreachable!(),
    };
    response
        .header("x-request-id", echo)
        .header("set-cookie", echo)
}

fn respond(case: usize, r: &Recorded, echo: &str) -> Reply {
    let format = r.query_all("format");
    let success = if !r.query_all("eventid").is_empty() {
        Reply::json(200, EVENT)
    } else if format.iter().any(|f| f == "csv") {
        Reply::text(200, "text/csv; charset=utf-8", CSV)
    } else {
        Reply::json(
            200,
            format!(
                r#"{{"type":"FeatureCollection","metadata":{{"generated":1388620800000,"count":1,"status":200}},"features":[{EVENT}]}}"#
            ),
        )
    };
    reply(case, success, echo)
}

async fn receive<T>(c: &mut Canaries, prepared: concord::Prepared<T>) -> Result<T, concord::Error> {
    let result = prepared.receive().await;
    c.assert_debug_absent("Received Result", &result);
    let received = result?;
    c.assert_debug_absent("response head", &received.head());
    c.assert_debug_absent("response metadata", &received.meta());
    received.decode().await
}

fn concord_error(c: &mut Canaries, error: &concord::Error) {
    c.assert_debug_absent("concord::Error", error);
    c.assert_error_absent("concord::Error and sources", error);
    if let Some(meta) = error.head_meta() {
        c.assert_debug_absent("error response metadata", &meta);
    }
}

fn error(c: &mut Canaries, error: &Error) {
    c.assert_debug_absent("SDK error", error);
    c.assert_error_absent("SDK error and sources", error);
    match error {
        Error::Concord(e) => concord_error(c, e),
        Error::Http(failure) => {
            c.assert_debug_absent("HTTP failure", failure);
            c.assert_debug_absent("HTTP failure kind", &failure.kind);
            c.assert_debug_absent("HTTP failure metadata", &failure.meta);
            if let Err(e) = failure.document_unredacted() {
                concord_error(c, e);
            }
        }
        Error::Stream(e) => {
            c.assert_debug_absent("stream error", e);
            c.assert_error_absent("stream error and sources", e);
        }
        _ => {}
    }
}

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("usgs-earthquake-catalog");
    let event_id = c.issue();
    let id = EventId::new(&event_id).unwrap();
    let window = Window::new("2014-01-01T00:00:00Z", "2014-01-02T00:00:00Z")
        .unwrap()
        .min_magnitude(Magnitude::new(4.0).unwrap())
        .with_page_size(PageSize::new(2).unwrap());
    c.assert_debug_absent("window", &window);
    let cap = NonZeroUsize::new(3).unwrap();
    for case in 0..7 {
        let echo = event_id.clone();
        let server = TestServer::start(move |r: &Recorded| respond(case, r, &echo)).await;
        let api = client(&if case == 6 { refused() } else { server.url() });
        let catalog = api.catalog();
        let read = catalog.event(id.clone());
        c.assert_debug_absent("client", &api);
        c.assert_debug_absent("catalog scope", &catalog);
        c.assert_debug_absent("event endpoint", &read);
        let prepared = read.clone().prepare().unwrap();
        c.assert_debug_absent("event Prepared", &prepared);
        let native = receive(&mut c, prepared).await;
        c.assert_debug_absent("native Result", &native);
        assert_eq!(native.is_ok(), case == 0);
        if let Err(e) = &native {
            concord_error(&mut c, e);
        }
        let result = read.send().await;
        c.assert_debug_absent("event Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        match &result {
            Ok(response) => c.assert_debug_absent("event metadata", &response.meta),
            Err(e) => {
                error(&mut c, e);
                if case == 1 {
                    // Withholding keeps diagnosis: status, kind and operation stay visible.
                    assert_eq!(e.to_string(), "USGS HTTP 400 (BadRequest)");
                    let Error::Http(failure) = e else {
                        panic!("{e:?}")
                    };
                    let operation = failure.meta.operation().unwrap();
                    assert_eq!(operation.name(), "Usgs.catalog.event");
                    assert!(failure.document_unredacted().unwrap().contains(&event_id));
                }
            }
        }
        let window_scope = catalog.query(&window);
        c.assert_debug_absent("window scope", &window_scope);
        let page = window_scope.geojson().send().await;
        c.assert_debug_absent("GeoJSON page Result", &page);
        assert_eq!(page.is_ok(), case == 0);
        if let Err(e) = &page {
            error(&mut c, e);
        }
        match window_scope.csv().send().await {
            Ok(response) => {
                c.assert_debug_absent("CSV response", &response);
                let mut rows = response.data;
                while let Some(row) = rows.next().await {
                    if let Err(e) = &row {
                        assert_ne!(case, 0);
                        error(&mut c, e);
                    }
                }
                c.assert_debug_absent("CSV rows", &rows);
                c.assert_debug_absent("CSV completion", &rows.completion());
            }
            Err(e) => {
                assert_ne!(case, 0);
                error(&mut c, &e);
            }
        }
        let mut pages = catalog.pages(&window, NonZeroU64::MIN, cap);
        c.assert_debug_absent("GeoJSON traversal", &pages);
        loop {
            match pages.next_page().await {
                Ok(Some(page)) => c.assert_debug_absent("traversal cursor", &page.next),
                Ok(None) => break,
                Err(e) => {
                    assert_ne!(case, 0);
                    c.assert_debug_absent("PagesError", &e);
                    c.assert_display_absent("PagesError", &e);
                    c.assert_error_absent("PagesError and sources", &e);
                    error(&mut c, e.provider_error().unwrap());
                    break;
                }
            }
        }
        let mut export = catalog.export(&window, NonZeroU64::MIN, cap);
        loop {
            match export.next().await {
                Ok(Some(_)) => {}
                Ok(None) => break,
                Err(e) => {
                    assert_ne!(case, 0);
                    error(&mut c, &e);
                    break;
                }
            }
        }
        c.assert_debug_absent("CSV export", &export);
        if case == 0 {
            let requests = server.requests();
            c.assert_on_wire(&requests);
            assert!(
                requests
                    .iter()
                    .any(|r| r.query_all("eventid") == [event_id.clone()])
            );
        }
        server.shutdown().await;
    }
    assert!(c.checked() >= 200);
    println!(
        "usgs-earthquake-catalog: {} renderings; positive control passed",
        c.checked()
    );
}

#[test]
fn refused_event_id_never_prints() {
    let mut c = Canaries::new("usgs-earthquake-catalog");
    let e = EventId::new(format!("{}\n", c.issue())).unwrap_err();
    error(&mut c, &e);
}

// `EventId`'s `Debug` withholds the value the declaration marks `#[sensitive]`; its `Display`
// (explicit formatting, documented as exposing data) shows it.
#[test]
fn event_id_debug_withholds_the_id() {
    let mut c = Canaries::new("usgs-earthquake-catalog");
    let id = EventId::new(c.issue()).unwrap();
    c.assert_debug_absent("event ID", &id);
}

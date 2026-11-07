//! Fresh-consumer application for the USGS Earthquake Catalog SDK (see FIRST_USE.md).
//!
//! Two caller tasks, one per required workflow, plus their failure paths. Every request goes to
//! `examples/testserver` on loopback; the live USGS service is NOT_RUN. Fixture bodies are
//! written here by hand in the shape of the FDSN event service's GeoJSON/CSV output.

use std::collections::BTreeMap;
use std::num::{NonZeroU64, NonZeroUsize};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use concord_sdk_usgs_earthquake_catalog::{
    CsvRow, Error, EventId, HttpKind, Magnitude, PageSize, Timestamp, Usgs, UsgsCatalog, Window,
    concord::{ErrorKind, reqwest},
    concord_codecs::stream::{Completion, ReadErrorKind},
    concord_pagination::{PagesError, Resume},
};
use concord_testserver::{Body, Recorded, Reply, TestServer};

// ---------------------------------------------------------------------------------------------
// Caller setup
// ---------------------------------------------------------------------------------------------

fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .build()
        .expect("reqwest client")
}

fn connect(server: &TestServer) -> UsgsCatalog {
    Usgs::new(http())
        .with_base_url(server.url())
        .expect("loopback base")
        .catalog()
}

/// One UTC day of M2.5+ earthquakes, two events per response so the window needs paging.
/// The start is written with a +02:00 offset on purpose: the SDK should normalize it to UTC.
fn march_first() -> Window {
    Window::new("2024-03-01T02:00:00+02:00", "2024-03-01T23:59:59Z")
        .expect("valid window")
        .min_magnitude(Magnitude::new(2.5).expect("finite"))
        .with_page_size(PageSize::new(2).expect("in range"))
}

fn n(value: u64) -> NonZeroU64 {
    NonZeroU64::new(value).expect("nonzero")
}

fn budget(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).expect("nonzero")
}

/// The query pairs every page request of `march_first()` must carry, in declaration order.
fn window_pairs(offset: u64, format: &str) -> Vec<(String, String)> {
    [
        ("nodata", "404"),
        ("starttime", "2024-03-01T00:00:00Z"),
        ("endtime", "2024-03-01T23:59:59Z"),
        ("minmagnitude", "2.5"),
        ("limit", "2"),
        ("orderby", "time-asc"),
        ("eventtype", "earthquake"),
        ("offset", &offset.to_string()),
        ("format", format),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v.to_owned()))
    .collect()
}

fn param(request: &Recorded, name: &str) -> Option<String> {
    request.query_all(name).into_iter().next()
}

// ---------------------------------------------------------------------------------------------
// Fixture data: three events on 2024-03-01, in time order
// ---------------------------------------------------------------------------------------------

struct Quake {
    id: &'static str,
    mag: f64,
    /// Place text exactly as the provider means it (commas, quotes, a newline).
    place: &'static str,
    time_ms: i64,
    time_text: &'static str,
    lon: f64,
    lat: f64,
    depth: f64,
}

static QUAKES: [Quake; 3] = [
    Quake {
        id: "us7000m1a2",
        mag: 4.6,
        place: "112 km SSE of Sand Point, Alaska",
        time_ms: 1_709_255_554_567,
        time_text: "2024-03-01T01:12:34.567Z",
        lon: -159.912,
        lat: 54.318,
        depth: 35.0,
    },
    Quake {
        id: "ak024abc1x",
        mag: 2.9,
        place: "25 km W of \"Cantwell\", Alaska",
        time_ms: 1_709_274_600_000,
        time_text: "2024-03-01T06:30:00.000Z",
        lon: -149.415,
        lat: 63.392,
        depth: 88.4,
    },
    Quake {
        id: "nc73912345",
        mag: 3.1,
        place: "3 km NNW of The Geysers, CA\r\nAftershock zone, \"Sonoma\" county",
        time_ms: 1_709_316_310_250,
        time_text: "2024-03-01T18:05:10.250Z",
        lon: -122.771,
        lat: 38.802,
        depth: 1.9,
    },
];

fn json_str(value: &str) -> String {
    let mut out = String::from('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn feature(q: &Quake, extra_properties: &str) -> String {
    format!(
        concat!(
            r#"{{"type":"Feature","properties":{{"mag":{mag},"place":{place},"time":{time},"#,
            r#""updated":{updated},"tz":null,"url":"https://earthquake.usgs.gov/earthquakes/eventpage/{id}","#,
            r#""detail":"https://earthquake.usgs.gov/fdsnws/event/1/query?eventid={id}&format=geojson","#,
            r#""felt":null,"cdi":null,"mmi":null,"alert":null,"status":"reviewed","tsunami":0,"sig":326,"#,
            r#""net":"{net}","code":"{code}","ids":",{id},","sources":",{net},","types":",origin,phase-data,","#,
            r#""nst":45,"dmin":1.2,"rms":0.8,"gap":110,"magType":"mb","type":"earthquake","#,
            r#""title":"M {mag} - {title}"{extra}}},"#,
            r#""geometry":{{"type":"Point","coordinates":[{lon},{lat},{depth}]}},"id":"{id}"}}"#
        ),
        mag = q.mag,
        place = json_str(q.place),
        time = q.time_ms,
        updated = q.time_ms + 3_600_000,
        id = q.id,
        net = &q.id[..2],
        code = &q.id[2..],
        title = q.place.replace(['\r', '\n', '"'], " "),
        extra = extra_properties,
        lon = q.lon,
        lat = q.lat,
        depth = q.depth,
    )
}

fn collection(quakes: &[&Quake]) -> String {
    let features: Vec<String> = quakes.iter().map(|q| feature(q, "")).collect();
    format!(
        concat!(
            r#"{{"type":"FeatureCollection","metadata":{{"generated":1709337600000,"#,
            r#""url":"https://earthquake.usgs.gov/fdsnws/event/1/query?format=geojson","#,
            r#""title":"USGS Earthquakes","status":200,"api":"1.14.1","limit":2,"count":{count}}},"#,
            r#""features":[{features}],"bbox":[-159.912,38.802,1.9,-122.771,63.392,88.4]}}"#
        ),
        count = features.len(),
        features = features.join(","),
    )
}

const CSV_HEADER: &str = "time,latitude,longitude,depth,mag,magType,nst,gap,dmin,rms,net,id,updated,place,type,horizontalError,depthError,magError,magNst,status,locationSource,magSource\n";

fn csv_field(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

/// One provider-shaped CSV row. The Alaska event has empty optional numeric cells.
fn csv_row(q: &Quake, updated: &str) -> String {
    let sparse = q.id.starts_with("ak");
    let opt = |v: &str| if sparse { String::new() } else { v.to_owned() };
    format!(
        "{time},{lat},{lon},{depth},{mag},ml,{nst},{gap},{dmin},0.62,{net},{id},{updated},{place},earthquake,{herr},2.1,{merr},{mnst},reviewed,{net},{net}\n",
        time = q.time_text,
        lat = q.lat,
        lon = q.lon,
        depth = q.depth,
        mag = q.mag,
        nst = opt("31"),
        gap = opt("64"),
        dmin = opt("0.07"),
        net = &q.id[..2],
        id = q.id,
        updated = updated,
        place = csv_field(q.place),
        herr = opt("0.31"),
        merr = opt("0.12"),
        mnst = opt("18"),
    )
}

const UPDATED: &str = "2024-03-01T20:00:00.040Z";

fn csv_page(quakes: &[&Quake]) -> String {
    let mut body = CSV_HEADER.to_owned();
    for q in quakes {
        body.push_str(&csv_row(q, UPDATED));
    }
    body
}

fn csv_reply(body: impl AsRef<[u8]>) -> Reply {
    Reply::text(200, "text/csv", body)
}

/// The provider's plain-text no-data document (it echoes the request, including the event ID).
fn no_data(request: &Recorded) -> Reply {
    Reply::text(
        404,
        "text/plain",
        format!(
            "Error 404: No Data Found\n\nNo data matches your request\n\nRequest:\n{}\n\nRequest Submitted:\n2024-03-02T00:00:00+00:00\n\nService version:\n1.14.1\n",
            request.target
        ),
    )
}

// ---------------------------------------------------------------------------------------------
// Workflow 1: query a bounded window as GeoJSON, then retrieve a chosen event
// ---------------------------------------------------------------------------------------------

/// Caller task 1. Walk the window's GeoJSON pages, pick the strongest event, look it up by ID.
#[tokio::test]
async fn workflow1_geojson_window_then_strongest_event() {
    let [a, b, c] = &QUAKES;
    let event_body = feature(
        a,
        r#","products":{"origin":[{"code":"7000m1a2","source":"us","properties":{"depth":"35.0","magnitude":"4.6"}}],"phase-data":[{"code":"7000m1a2","source":"us"}]}"#,
    );
    let server = TestServer::start(move |r: &Recorded| {
        if let Some(id) = param(r, "eventid") {
            return if id == "us7000m1a2" {
                Reply::json(200, &event_body)
            } else {
                no_data(r)
            };
        }
        match param(r, "offset").as_deref() {
            Some("1") => Reply::json(200, collection(&[a, b])),
            Some("3") => Reply::json(200, collection(&[c])),
            _ => Reply::new(500),
        }
    })
    .await;
    let catalog = connect(&server);
    let window = march_first();

    // --- the window, page by page -----------------------------------------------------------
    let mut pages = catalog.pages(&window, NonZeroU64::MIN, budget(5));
    let mut seen = Vec::new();
    let mut continuations = Vec::new();
    while let Some(page) = pages.next_page().await.expect("every page succeeds") {
        continuations.push(page.next);
        seen.extend(page.items);
    }
    // A short page ends the walk: the full first page continued at offset 3, the second did not.
    assert_eq!(continuations, [Some(n(3)), None]);
    // The end is fused: asking again is still a clean end and sends nothing.
    assert!(pages.next_page().await.expect("still ended").is_none());
    let progress = pages.progress();
    assert_eq!(
        (progress.admitted, progress.succeeded, progress.failed),
        (2, 2, 0)
    );

    let ids: Vec<&str> = seen.iter().map(|f| f.id.as_str()).collect();
    assert_eq!(ids, ["us7000m1a2", "ak024abc1x", "nc73912345"]);
    let third = &seen[2];
    assert_eq!(third.properties.place.as_deref(), Some(c.place));
    assert_eq!(third.properties.time, c.time_ms);
    assert_eq!(third.properties.event_type, "earthquake");
    assert_eq!(third.properties.status, "reviewed");
    assert_eq!(
        third.geometry.as_ref().map(|p| p.coordinates),
        Some([c.lon, c.lat, c.depth])
    );

    // Caller's own choice: the strongest event in the window.
    let strongest = seen
        .iter()
        .max_by(|x, y| x.properties.mag.partial_cmp(&y.properties.mag).unwrap())
        .map(|f| f.id.clone())
        .expect("non-empty window");

    // --- the chosen event --------------------------------------------------------------------
    let endpoint = catalog.event(strongest.clone());
    assert!(
        !format!("{endpoint:?}").contains("us7000m1a2"),
        "sensitive event id withheld from Debug"
    );
    let event = endpoint.send().await.expect("event lookup");
    assert_eq!(event.meta.status(), 200);
    assert_eq!(
        event
            .meta
            .headers()
            .get("content-type")
            .map(|v| v.to_str().unwrap()),
        Some("application/json; charset=utf-8"),
        "the head the loopback server sent is kept"
    );
    assert_eq!(event.data.id, strongest);
    assert_eq!(event.data.properties.mag, Some(4.6));
    assert_eq!(event.data.properties.place.as_deref(), Some(a.place));
    assert_eq!(
        event.data.properties.detail.as_deref(),
        Some("https://earthquake.usgs.gov/fdsnws/event/1/query?eventid=us7000m1a2&format=geojson")
    );
    let products = event
        .data
        .properties
        .products
        .as_ref()
        .expect("products kept as JSON");
    assert_eq!(products["origin"][0]["properties"]["magnitude"], "4.6");

    // --- the wire ----------------------------------------------------------------------------
    let requests = server.requests();
    assert_eq!(
        requests.len(),
        3,
        "two pages and one lookup; detail URL never followed"
    );
    for (request, offset) in requests[..2].iter().zip([1, 3]) {
        assert_eq!(request.method, "GET");
        assert_eq!(request.path(), "/fdsnws/event/1/query");
        assert_eq!(request.query_pairs(), window_pairs(offset, "geojson"));
        assert_eq!(request.header("accept"), Some("application/json"));
        // No credentials, API key or user agent: exactly what reqwest and the SDK put there.
        let names: Vec<&str> = request.headers.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(names, ["accept", "host"]);
        assert!(request.body.is_empty());
    }
    let lookup = &requests[2];
    assert_eq!(lookup.path(), "/fdsnws/event/1/query");
    assert_eq!(
        lookup.query_pairs(),
        [
            ("nodata".to_owned(), "404".to_owned()),
            ("eventid".to_owned(), "us7000m1a2".to_owned()),
            ("format".to_owned(), "geojson".to_owned()),
        ],
        "lookup inherits no window, magnitude, limit, ordering or offset"
    );

    // --- a lookup that finds nothing ---------------------------------------------------------
    let gone = EventId::new("us6000zzzz").expect("valid id");
    let err = catalog
        .event(gone)
        .send()
        .await
        .expect_err("404 is not an event");
    assert!(err.is_no_data());
    let Error::Http(failure) = &err else {
        panic!("expected an HTTP failure, got {err:?}");
    };
    assert_eq!(failure.kind, HttpKind::NoData);
    assert_eq!(failure.meta.status(), 404);
    let document = failure.document_unredacted().expect("bounded text body");
    assert!(document.starts_with("Error 404: No Data Found"));
    assert!(
        document.contains("us6000zzzz"),
        "the provider echoes the ID in its body"
    );
    assert!(!format!("{err:?}").contains("us6000zzzz"));
    assert!(!err.to_string().contains("us6000zzzz"));

    let cleanup = server.shutdown().await;
    assert_eq!(cleanup.open_after, 0);
}

/// Workflow 1 failure paths: a provider 503 keeps the prefix and a typed resume point; a budget
/// is not an end; an exact multiple needs one more (no-data) request; a malformed page is a
/// Concord decode error.
#[tokio::test]
async fn workflow1_failures_keep_prefix_and_resume_point() {
    let [a, b, c] = &QUAKES;
    let recovered = Arc::new(AtomicBool::new(false));
    let flag = recovered.clone();
    let server = TestServer::start(move |r: &Recorded| match param(r, "offset").as_deref() {
        Some("1") => Reply::json(200, collection(&[a, b])),
        Some("3") if !flag.load(Ordering::SeqCst) => Reply::text(
            503,
            "text/html",
            "<html><body>Service Temporarily Unavailable</body></html>",
        )
        .header("retry-after", "120"),
        Some("3") => Reply::json(200, collection(&[c])),
        _ => Reply::new(500),
    })
    .await;
    let catalog = connect(&server);
    let window = march_first();

    // 503 on the second page.
    let mut pages = catalog.pages(&window, NonZeroU64::MIN, budget(5));
    let first = pages.next_page().await.expect("page 1").expect("not ended");
    assert_eq!(first.items.len(), 2);
    let failure = pages.next_page().await.expect_err("page 2 is a 503");
    let PagesError::Provider {
        error,
        failed,
        progress,
        ..
    } = &failure
    else {
        panic!("expected a provider failure, got {failure:?}");
    };
    assert_eq!(*failed, Resume::At(n(3)));
    assert_eq!(
        (progress.admitted, progress.succeeded, progress.failed),
        (2, 1, 1)
    );
    let Error::Http(http) = error else {
        panic!("expected HTTP, got {error:?}");
    };
    assert_eq!(http.kind, HttpKind::Server);
    assert_eq!(http.meta.status(), 503);
    assert_eq!(
        http.retry_after(SystemTime::now()).expect("valid header"),
        Some(Duration::from_secs(120))
    );
    assert!(
        http.document_unredacted()
            .unwrap()
            .contains("Temporarily Unavailable")
    );
    // Not a clean end afterwards.
    match pages.next_page().await {
        Err(PagesError::Ended { .. }) => {}
        other => panic!("expected Ended after a failure, got {other:?}"),
    }
    assert_eq!(server.requests().len(), 2, "no automatic retry");

    // Resume where it failed, once the provider recovers.
    recovered.store(true, Ordering::SeqCst);
    let resume_from = match failure.resume() {
        Some(Resume::At(offset)) => *offset,
        other => panic!("expected a resume offset, got {other:?}"),
    };
    let rest = catalog
        .pages(&window, resume_from, budget(5))
        .collect_complete()
        .await
        .expect("remaining pages");
    let rest_ids: Vec<&str> = rest
        .iter()
        .flat_map(|p| p.items.iter().map(|f| f.id.as_str()))
        .collect();
    assert_eq!(rest_ids, ["nc73912345"]);
    assert_eq!(param(&server.requests()[2], "offset").as_deref(), Some("3"));

    // A budget reached on a full page is an error carrying the next offset, not an end.
    let bounded = catalog
        .pages(&window, NonZeroU64::MIN, budget(1))
        .collect_complete()
        .await
        .expect_err("budget of one cannot prove exhaustion");
    assert_eq!(bounded.collected.len(), 1);
    match &bounded.error {
        PagesError::Bounded { resume, .. } => assert_eq!(*resume, Resume::At(n(3))),
        other => panic!("expected Bounded, got {other:?}"),
    }
    server.shutdown().await;

    // Exact multiple: two full pages need a third request; its 404 is exhaustion.
    let [a, b, c] = &QUAKES;
    let exact = TestServer::start(move |r: &Recorded| match param(r, "offset").as_deref() {
        Some("1") => Reply::json(200, collection(&[a, b])),
        Some("3") => Reply::json(200, collection(&[c, a])),
        _ => no_data(r),
    })
    .await;
    let all = connect(&exact)
        .pages(&window, NonZeroU64::MIN, budget(5))
        .collect_complete()
        .await
        .expect("404 after a full page ends cleanly");
    assert_eq!(all.iter().map(|p| p.items.len()).sum::<usize>(), 4);
    let offsets: Vec<Option<String>> = exact
        .requests()
        .iter()
        .map(|r| param(r, "offset"))
        .collect();
    assert_eq!(
        offsets,
        [Some("1".into()), Some("3".into()), Some("5".into())]
    );
    exact.shutdown().await;

    // Malformed 200 body and a mislabelled body: Concord decode/metadata errors with a head.
    let broken = TestServer::start(|r: &Recorded| match param(r, "offset").as_deref() {
        Some("1") => Reply::json(
            200,
            r#"{"type":"FeatureCollection","metadata":{"generated":1,"#,
        ),
        _ => Reply::text(200, "text/html", "<html>maintenance</html>"),
    })
    .await;
    let catalog = connect(&broken);
    let err = catalog
        .query(&window)
        .geojson()
        .send()
        .await
        .expect_err("truncated JSON");
    let Error::Concord(concord) = &err else {
        panic!("expected Concord, got {err:?}");
    };
    assert_eq!(concord.kind(), ErrorKind::Decode);
    assert_eq!(concord.head().map(|h| h.status().as_u16()), Some(200));
    let err = catalog
        .query(&window)
        .geojson()
        .set_offset(n(3))
        .send()
        .await
        .expect_err("text/html is not GeoJSON");
    let Error::Concord(concord) = &err else {
        panic!("expected Concord, got {err:?}");
    };
    assert_eq!(concord.kind(), ErrorKind::Metadata);
    broken.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Workflow 2: export the same window as CSV and process rows incrementally
// ---------------------------------------------------------------------------------------------

/// Caller task 2. Stream the same window as CSV, aggregating per network as rows arrive, and
/// reconcile with the GeoJSON IDs.
#[tokio::test]
async fn workflow2_csv_export_same_window_incrementally() {
    let [a, b, c] = &QUAKES;
    let page1 = csv_page(&[a, b]);
    // Split inside the second row's quoted place, right after its doubled quote, and hold the
    // rest back long enough to observe that the first row was delivered before the body ended.
    let cut = page1.find("\"\"Cantwell").expect("quoted place") + 2;
    let (head, tail) = page1.split_at(cut);
    let (head, tail) = (head.to_owned(), tail.to_owned());
    let page2 = csv_page(&[c]);
    let server = TestServer::start(move |r: &Recorded| match param(r, "offset").as_deref() {
        Some("1") if param(r, "format").as_deref() == Some("csv") => {
            csv_reply("").with_body(Body::Chunks(vec![
                (Duration::ZERO, head.clone().into()),
                (Duration::from_millis(300), tail.clone().into()),
            ]))
        }
        Some("3") if param(r, "format").as_deref() == Some("csv") => csv_reply(page2.clone()),
        Some("1") => Reply::json(200, collection(&[a, b])),
        Some("3") => Reply::json(200, collection(&[c])),
        _ => Reply::new(500),
    })
    .await;
    let catalog = connect(&server);
    let window = march_first();

    let mut export = catalog.export(&window, NonZeroU64::MIN, budget(4));
    assert_eq!(export.completion(), Completion::Reading);
    assert_eq!(
        export.pages_requested(),
        0,
        "building the export sends nothing"
    );

    let mut rows: Vec<CsvRow> = Vec::new();
    let mut per_network: BTreeMap<String, (u32, f64)> = BTreeMap::new();
    let mut first_row_while_body_open = None;
    while let Some(row) = export.next().await.expect("clean export") {
        if rows.is_empty() {
            // The first page is still being written by the server.
            first_row_while_body_open = Some(server.active_bodies() == 1);
        }
        let entry = per_network.entry(row.net.clone()).or_default();
        entry.0 += 1;
        entry.1 = entry.1.max(row.mag.unwrap_or(f64::NEG_INFINITY));
        rows.push(row);
    }
    assert_eq!(
        first_row_while_body_open,
        Some(true),
        "rows arrive incrementally"
    );
    assert_eq!(export.completion(), Completion::EndOfInput);
    assert_eq!(export.rows_delivered(), 3);
    assert_eq!(export.pages_requested(), 2);
    assert!(export.next().await.expect("fused clean end").is_none());

    // Quoted commas, doubled quotes and CRLF inside a field survive, across a chunk boundary.
    let places: Vec<&str> = rows.iter().map(|r| r.place.as_str()).collect();
    assert_eq!(places, [a.place, b.place, c.place]);
    assert_eq!(rows[2].place.lines().count(), 2);
    // Typed values; empty optional numeric cells are None, not 0.
    assert_eq!(rows[0].mag, Some(4.6));
    assert_eq!(rows[0].nst, Some(31));
    assert_eq!(rows[1].nst, None);
    assert_eq!(rows[1].gap, None);
    assert_eq!(rows[1].mag_nst, None);
    assert_eq!(rows[1].horizontal_error, None);
    assert_eq!(rows[1].depth_error, Some(2.1));
    assert_eq!(rows[2].time, c.time_text);
    assert_eq!(rows[2].updated, UPDATED);
    assert_eq!(
        (rows[2].latitude, rows[2].longitude, rows[2].depth),
        (c.lat, c.lon, c.depth)
    );
    assert_eq!(rows[2].event_type, "earthquake");
    assert_eq!(rows[2].mag_type, "ml");
    assert_eq!(per_network["ak"], (1, 2.9));
    assert_eq!(per_network["us"], (1, 4.6));

    // The same window as GeoJSON, for the caller's reconciliation by ID.
    let geo = catalog
        .pages(&window, NonZeroU64::MIN, budget(4))
        .collect_complete()
        .await
        .expect("geojson window");
    let geo_ids: Vec<EventId> = geo
        .into_iter()
        .flat_map(|p| p.items)
        .map(|f| f.id)
        .collect();
    let csv_ids: Vec<EventId> = rows.iter().map(|r| r.id.clone()).collect();
    assert_eq!(csv_ids, geo_ids);

    // Wire: only `format` differs between the two workflows.
    let requests = server.requests();
    assert_eq!(requests.len(), 4);
    assert_eq!(requests[0].query_pairs(), window_pairs(1, "csv"));
    assert_eq!(requests[1].query_pairs(), window_pairs(3, "csv"));
    assert_eq!(requests[2].query_pairs(), window_pairs(1, "geojson"));
    for csv in &requests[..2] {
        assert_eq!(csv.path(), "/fdsnws/event/1/query");
        assert_eq!(csv.header("accept"), Some("text/csv"));
        let names: Vec<&str> = csv.headers.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(names, ["accept", "host"]);
    }
    assert_eq!(server.shutdown().await.open_after, 0);
}

/// Workflow 2 failure: a bad row is terminal and is never skipped; earlier rows stay delivered;
/// the export never reports a clean end afterwards.
#[tokio::test]
async fn workflow2_bad_row_is_terminal_never_skipped() {
    let [a, b, c] = &QUAKES;
    // Page 2: a good row, then a row whose provider forgot to quote a place containing a comma
    // (23 cells under a 22-name header), then a good row that must never be delivered.
    let bad = format!(
        "2024-03-01T18:05:10.250Z,38.802,-122.771,1.9,3.1,ml,31,64,0.07,0.62,nc,nc73912341,\
         {UPDATED},3 km NNW of The Geysers, CA,earthquake,0.31,2.1,0.12,18,reviewed,nc,nc\n"
    );
    let mut page2 = CSV_HEADER.to_owned();
    page2.push_str(&csv_row(c, UPDATED).replace("nc73912345", "nc73912340"));
    page2.push_str(&bad);
    page2.push_str(&csv_row(c, UPDATED).replace("nc73912345", "nc73912342"));
    let page1 = csv_page(&[a, b]);
    let server = TestServer::start(move |r: &Recorded| match param(r, "offset").as_deref() {
        Some("1") => csv_reply(page1.clone()),
        Some("3") => csv_reply(page2.clone()),
        _ => Reply::new(500),
    })
    .await;
    let catalog = connect(&server);
    let window = march_first();

    let mut export = catalog.export(&window, NonZeroU64::MIN, budget(4));
    let mut delivered = Vec::new();
    let err = loop {
        match export.next().await {
            Ok(Some(row)) => delivered.push(row.id.to_string()),
            Ok(None) => panic!("a bad row must not end cleanly"),
            Err(err) => break err,
        }
    };
    assert_eq!(delivered, ["us7000m1a2", "ak024abc1x", "nc73912340"]);
    let Error::Stream(read) = &err else {
        panic!("expected a stream error, got {err:?}");
    };
    assert_eq!(read.kind(), ReadErrorKind::Format);
    assert_eq!(
        read.position().records,
        1,
        "one row of this response preceded it"
    );
    assert_eq!(read.operation(), Some("Usgs.catalog.window.csv"));
    assert_eq!(export.completion(), Completion::Failed);
    assert_eq!(export.rows_delivered(), 3);
    assert!(matches!(export.next().await, Err(Error::ExportFailed)));
    assert!(matches!(export.next().await, Err(Error::ExportFailed)));
    assert_eq!(server.requests().len(), 2, "no page after the failure");
    server.shutdown().await;
}

/// Single-response CSV reads (`query(..).csv().send()`): unclosed quote at end of body, a bad
/// number, a transfer cut mid-record. Each is reported once, after the good row, then `None`.
#[tokio::test]
async fn workflow2_single_page_csv_failures() {
    let [a, b, _] = &QUAKES;
    let good = csv_row(a, UPDATED);
    let unclosed = format!(
        "{CSV_HEADER}{good}{}",
        csv_row(b, UPDATED)
            .trim_end()
            .replacen("Alaska\"", "Alaska", 1)
    );
    let bad_number = format!(
        "{CSV_HEADER}{good}{}",
        csv_row(b, UPDATED).replacen(",2.9,", ",2.9x,", 1)
    );
    let truncated = format!("{CSV_HEADER}{good}{}", &csv_row(b, UPDATED)[..40]);
    let server = TestServer::start(move |r: &Recorded| match param(r, "offset").as_deref() {
        Some("1") => csv_reply(unclosed.clone()),
        Some("2") => csv_reply(bad_number.clone()),
        Some("3") => csv_reply("").with_body(Body::FailAfter(vec![truncated.clone().into()])),
        _ => Reply::new(500),
    })
    .await;
    let catalog = connect(&server);
    let window = march_first();

    async fn read_one(
        catalog: &UsgsCatalog,
        window: &Window,
        offset: u64,
    ) -> (Vec<String>, Error, Completion) {
        let response = catalog
            .query(window)
            .csv()
            .set_offset(n(offset))
            .send()
            .await
            .expect("the head is fine");
        assert_eq!(response.meta.status(), 200);
        let mut rows = response.data;
        let mut ids = Vec::new();
        let err = loop {
            match rows.next().await {
                Some(Ok(row)) => ids.push(row.id.to_string()),
                Some(Err(err)) => break err,
                None => panic!("offset {offset}: ended without an error"),
            }
        };
        assert!(
            rows.next().await.is_none(),
            "error is reported once, then None"
        );
        (ids, err, rows.completion())
    }

    let (ids, err, completion) = read_one(&catalog, &window, 1).await;
    assert_eq!(ids, ["us7000m1a2"]);
    assert_eq!(completion, Completion::Failed);
    assert!(
        matches!(&err, Error::Stream(e) if e.kind() == ReadErrorKind::Format),
        "{err:?}"
    );

    let (ids, err, completion) = read_one(&catalog, &window, 2).await;
    assert_eq!(ids, ["us7000m1a2"]);
    assert_eq!(completion, Completion::Failed);
    assert!(
        matches!(&err, Error::Stream(e) if e.kind() == ReadErrorKind::Decode),
        "{err:?}"
    );

    let (ids, err, completion) = read_one(&catalog, &window, 3).await;
    assert_eq!(ids, ["us7000m1a2"]);
    assert_eq!(completion, Completion::Failed);
    assert!(
        matches!(&err, Error::Stream(e) if e.kind() == ReadErrorKind::Transport),
        "{err:?}"
    );

    server.shutdown().await;
}

/// Workflow 2 budget and provider updates between pages.
#[tokio::test]
async fn workflow2_budget_and_shifting_offsets() {
    let [a, b, c] = &QUAKES;
    // Between page 1 and page 2 a new event was inserted early in the window, so offset 3 now
    // starts with B again, and B was revised (later `updated`).
    let page1 = csv_page(&[a, b]);
    let mut page2 = CSV_HEADER.to_owned();
    page2.push_str(&csv_row(b, "2024-03-01T21:15:00.000Z"));
    page2.push_str(&csv_row(c, UPDATED));
    let server = TestServer::start(move |r: &Recorded| match param(r, "offset").as_deref() {
        Some("1") => csv_reply(page1.clone()),
        Some("3") => csv_reply(page2.clone()),
        _ => no_data(r),
    })
    .await;
    let catalog = connect(&server);
    let window = march_first();

    // Budget 1: a full page cannot prove the end.
    let mut export = catalog.export(&window, NonZeroU64::MIN, budget(1));
    let mut count = 0;
    let err = loop {
        match export.next().await {
            Ok(Some(_)) => count += 1,
            Ok(None) => panic!("budget is not exhaustion"),
            Err(err) => break err,
        }
    };
    assert_eq!(count, 2);
    assert!(
        matches!(err, Error::PageBudget { next_offset } if next_offset == n(3)),
        "{err:?}"
    );
    assert_eq!(
        export.completion(),
        Completion::Failed,
        "a budget stop is not an end"
    );
    assert_eq!(export.rows_delivered(), 2);
    assert!(matches!(export.next().await, Err(Error::ExportFailed)));

    // Enough budget: the duplicate is delivered as-is; the caller reconciles by ID + updated.
    let mut export = catalog.export(&window, NonZeroU64::MIN, budget(4));
    let mut latest: BTreeMap<String, CsvRow> = BTreeMap::new();
    let mut raw = 0;
    while let Some(row) = export.next().await.expect("clean export") {
        raw += 1;
        let keep = latest
            .get(row.id.as_str())
            .is_none_or(|old| old.updated < row.updated);
        if keep {
            latest.insert(row.id.to_string(), row);
        }
    }
    assert_eq!(export.completion(), Completion::EndOfInput);
    assert_eq!(raw, 4, "SDK does not deduplicate");
    assert_eq!(latest.len(), 3);
    assert_eq!(latest["ak024abc1x"].updated, "2024-03-01T21:15:00.000Z");
    // offsets 1, 3 (full) then 5 → 404 → end.
    let offsets: Vec<String> = server.requests()[1..]
        .iter()
        .filter_map(|r| param(r, "offset"))
        .collect();
    assert_eq!(offsets, ["1", "3", "5"]);
    server.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Inputs refused before I/O
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn invalid_inputs_are_refused_before_io() {
    let server = TestServer::start(|_| Reply::new(500)).await;
    let catalog = connect(&server);

    let invalid = |r: Result<Window, Error>| matches!(r, Err(Error::InvalidInput(_)));
    assert!(invalid(Window::new(
        "2024-03-02T00:00:00Z",
        "2024-03-01T00:00:00Z"
    )));
    assert!(invalid(Window::new(
        "2024-01-01T00:00:00Z",
        "2024-02-02T00:00:00Z"
    )));
    assert!(invalid(Window::new("2024-03-01", "2024-03-02")));
    assert!(invalid(Window::new(
        "2024-03-01T00:00:00",
        "2024-03-02T00:00:00"
    )));
    assert!(
        Window::new("2024-01-01T00:00:00Z", "2024-02-01T00:00:00Z").is_ok(),
        "31 days"
    );
    assert!(matches!(PageSize::new(0), Err(Error::InvalidInput(_))));
    assert!(matches!(PageSize::new(20_001), Err(Error::InvalidInput(_))));
    assert!(PageSize::new(20_000).is_ok());
    assert!(matches!(
        Magnitude::new(f64::NAN),
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(EventId::new(""), Err(Error::InvalidInput(_))));
    assert!(matches!(
        EventId::new("us7000\nm1a2"),
        Err(Error::InvalidInput(_))
    ));

    // A generated scope setter that breaks the window is caught at send time, before I/O.
    let window = march_first();
    let scope = catalog
        .query(&window)
        .set_endtime(Timestamp::parse("2024-02-01T00:00:00Z").unwrap());
    let err = scope.geojson().send().await.expect_err("end before start");
    assert!(matches!(err, Error::InvalidInput(_)), "{err:?}");
    let err = scope.csv().send().await.expect_err("end before start");
    assert!(matches!(err, Error::InvalidInput(_)), "{err:?}");

    assert!(server.requests().is_empty());
    server.shutdown().await;
}

/// A header that lacks a required column is never accepted silently, but how it is reported
/// depends on whether a data row follows and on the column's type. The README's table says
/// `Protocol` for every missing required column; with a data row and a non-optional column the
/// observed result is `Stream(Decode)` instead (FIRST_USE.md, mismatch M-3).
#[tokio::test]
async fn missing_csv_column_is_never_accepted() {
    fn without(column: &str, with_row: bool) -> String {
        let names: Vec<&str> = CSV_HEADER.trim_end().split(',').collect();
        let at = names
            .iter()
            .position(|c| *c == column)
            .expect("known column");
        let mut body = names
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != at)
            .map(|(_, c)| *c)
            .collect::<Vec<_>>()
            .join(",");
        body.push('\n');
        if with_row {
            // QUAKES[2]'s quoted place holds commas; build the row from typed cells instead.
            let mut cells: Vec<String> = [
                "2024-03-01T18:05:10.250Z",
                "38.802",
                "-122.771",
                "1.9",
                "3.1",
                "md",
                "31",
                "64",
                "0.07",
                "0.62",
                "nc",
                "nc73912345",
                UPDATED,
                "\"3 km NNW of The Geysers, CA\"",
                "earthquake",
                "0.31",
                "2.1",
                "0.12",
                "18",
                "reviewed",
                "nc",
                "nc",
            ]
            .map(str::to_owned)
            .to_vec();
            cells.remove(at);
            body.push_str(&cells.join(","));
            body.push('\n');
        }
        body
    }
    let cases = [
        ("magSource", false),
        ("magSource", true),
        ("id", true),
        ("nst", true),
        ("mag", true),
    ];
    let bodies: Vec<String> = cases.iter().map(|(c, r)| without(c, *r)).collect();
    let server = TestServer::start(move |r: &Recorded| {
        let i: usize = param(r, "offset").unwrap().parse().unwrap();
        csv_reply(bodies[i - 1].clone())
    })
    .await;
    let catalog = connect(&server);
    let window = march_first();
    let mut outcomes = Vec::new();
    for (i, (column, with_row)) in cases.iter().enumerate() {
        let response = catalog
            .query(&window)
            .csv()
            .set_offset(n(i as u64 + 1))
            .send()
            .await
            .expect("head accepted");
        let mut rows = response.data;
        let first = rows.next().await;
        assert!(rows.next().await.is_none());
        assert_eq!(
            rows.completion(),
            Completion::Failed,
            "{column} with_row={with_row}"
        );
        let outcome = match first {
            Some(Err(Error::Protocol(_))) => "Protocol",
            Some(Err(Error::Stream(e))) if e.kind() == ReadErrorKind::Decode => {
                assert_eq!(e.position().records, 0, "no row was delivered");
                "Stream(Decode)"
            }
            other => panic!("{column} with_row={with_row}: {other:?}"),
        };
        outcomes.push(outcome);
    }
    // Observed classification (not the documented one for rows 2 and 3).
    assert_eq!(
        outcomes,
        [
            "Protocol",
            "Stream(Decode)",
            "Stream(Decode)",
            "Protocol",
            "Protocol"
        ]
    );
    server.shutdown().await;
}

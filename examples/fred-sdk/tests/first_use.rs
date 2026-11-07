//! Fresh-caller workflows, with FRED represented only by loopback fixtures.
use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_fred::{
    Error, Fred, FredConfig, ObservationLimit, SearchLimit,
    concord::ErrorKind,
    concord_pagination::{PagesError, Resume},
    http_client, pages,
};
use std::{
    io::{Cursor, Write},
    num::NonZeroUsize,
};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

const KEY: &str = "synthetic-local-key";
const SERIES: &str = r#"<series id="INDPRO" title="Industrial Production" realtime_start="2024-01-01" realtime_end="2024-01-01" observation_start="1919-01-01" observation_end="2024-01-01" frequency="Monthly" frequency_short="M" units="Index 2017=100" units_short="Index" seasonal_adjustment="Seasonally Adjusted" seasonal_adjustment_short="SA" last_updated="2024-02-01 09:15:00-06" popularity="80"/>"#;
const CSV: &[u8] = b"observation_date,INDPRO\n2024-01-01,.\n2024-02-01,NA\n2024-03-01,102.00\n";

fn api(server: &TestServer) -> Fred {
    Fred::new(
        http_client().unwrap(),
        FredConfig {
            api_key: KEY.into(),
        },
    )
    .with_base_url(server.url())
    .unwrap()
}
fn check_wire(r: &Recorded, path: &str, pairs: &[(&str, &str)], accept: &str) {
    assert_eq!(r.method, "GET");
    assert_eq!(r.path(), path);
    assert_eq!(r.body_len, 0);
    assert_eq!(r.header("accept"), Some(accept));
    assert_eq!(r.query_pairs().len(), pairs.len());
    for (key, value) in pairs {
        assert_eq!(r.query_all(key), [*value]);
    }
}
fn zip_csv(bytes: &[u8]) -> Vec<u8> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file(
        "INDPRO.csv",
        SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
    )
    .unwrap();
    zip.write_all(bytes).unwrap();
    zip.start_file("README.txt", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"Fixture export").unwrap();
    zip.finish().unwrap().into_inner()
}
fn concord_kind(error: Error, expected: ErrorKind) {
    match error {
        Error::Concord(e) => assert_eq!(e.kind(), expected),
        other => panic!("unexpected {other:?}"),
    }
}

#[tokio::test]
async fn search_and_metadata() {
    let server = TestServer::start(|r: &Recorded| {
        if r.path() == "/fred/series" {
            return Reply::text(
                200,
                "application/xml",
                format!("<seriess>{SERIES}</seriess>"),
            );
        }
        let offset = r.query_all("offset");
        if r.query_all("search_text") == ["broken"] {
            return Reply::json(400, "{broken");
        }
        if r.query_all("search_text") == ["failure"] && offset == ["1"] {
            return Reply::json(
                429,
                format!(r#"{{"error_code":429,"error_message":"key {KEY}"}}"#),
            )
            .header("Retry-After", "5");
        }
        let series = if offset == ["0"] {
            SERIES.to_owned()
        } else {
            SERIES.replace("INDPRO", "IPMAN")
        };
        Reply::text(
            200,
            "application/xml",
            format!(
                r#"<seriess count="2" offset="{}" limit="1">{series}</seriess>"#,
                offset[0]
            ),
        )
    })
    .await;
    let api = api(&server);
    assert!(!format!("{api:?}").contains(KEY));
    assert!(matches!(
        SearchLimit::new(0),
        Err(Error::InvalidLimit { maximum: 1000 })
    ));
    let template = api
        .search("industrial production")
        .set_limit(SearchLimit::new(1).unwrap());
    let mut walk = pages(template, Resume::Start, NonZeroUsize::new(3).unwrap());
    let first = walk.next_page().await.unwrap().unwrap();
    assert_eq!(first.items[0].id, "INDPRO");
    let second = walk.next_page().await.unwrap().unwrap();
    assert_eq!(second.items[0].id, "IPMAN");
    assert!(walk.next_page().await.unwrap().is_none());
    let metadata = api
        .series(&first.items[0].id)
        .metadata()
        .send()
        .await
        .unwrap();
    assert_eq!(metadata.seriess[0].title, "Industrial Production");
    assert_eq!(metadata.seriess[0].frequency, "Monthly");
    let mut failed = pages(
        api.search("failure")
            .set_limit(SearchLimit::new(1).unwrap()),
        Resume::Start,
        NonZeroUsize::new(3).unwrap(),
    );
    let partial = failed.next_page().await.unwrap().unwrap().items;
    let err = failed.next_page().await.unwrap_err();
    assert_eq!(partial[0].id, "INDPRO");
    assert_eq!(err.resume(), Some(&Resume::At(1)));
    assert!(!format!("{err:?}").contains(KEY));
    match err {
        PagesError::Provider {
            error: Error::Http(http),
            ..
        } => {
            assert_eq!(http.meta.status().as_u16(), 429);
            assert_eq!(http.retry_after.unwrap().unwrap().as_secs(), 5);
            let problem = http.document.unwrap();
            assert_eq!(problem.error_code, 429);
            assert!(problem.message_unredacted().contains(KEY));
            assert!(!format!("{problem:?}").contains(KEY));
        }
        other => panic!("unexpected {other:?}"),
    }
    let broken = api.search("broken").send().await.unwrap_err();
    assert!(!format!("{broken:?} {broken}").contains(KEY));
    match broken {
        Error::Http(http) => {
            assert_eq!(http.meta.status().as_u16(), 400);
            assert!(http.document.is_err());
        }
        other => panic!("unexpected {other:?}"),
    }
    let requests = server.requests();
    assert_eq!(requests.len(), 6);
    for (i, offset) in [(0, "0"), (1, "1")] {
        check_wire(
            &requests[i],
            "/fred/series/search",
            &[
                ("api_key", KEY),
                ("search_text", "industrial production"),
                ("limit", "1"),
                ("offset", offset),
            ],
            "text/xml",
        );
    }
    check_wire(
        &requests[2],
        "/fred/series",
        &[("api_key", KEY), ("series_id", "INDPRO")],
        "text/xml",
    );
    for (i, offset) in [(3, "0"), (4, "1")] {
        check_wire(
            &requests[i],
            "/fred/series/search",
            &[
                ("api_key", KEY),
                ("search_text", "failure"),
                ("limit", "1"),
                ("offset", offset),
            ],
            "text/xml",
        );
    }
    check_wire(
        &requests[5],
        "/fred/series/search",
        &[
            ("api_key", KEY),
            ("search_text", "broken"),
            ("limit", "1000"),
            ("offset", "0"),
        ],
        "text/xml",
    );
    server.shutdown().await;
}

#[tokio::test]
async fn observations_and_csv_export() {
    let zip = zip_csv(CSV);
    let large = zip_csv(&vec![b'x'; 4 * 1024 * 1024 + 1]);
    assert!(large.len() < 1024 * 1024);
    let server = TestServer::start(move |r: &Recorded| {
        let offset = r.query_all("offset");
        match r.query_all("file_type").first().map(String::as_str) {
            Some("csv") => match offset[0].as_str() {
                "0" => Reply::new(200).header("content-type", "application/zip").body(&zip),
                "1" => Reply::text(200, "text/csv", CSV),
                "2" => Reply::new(200).header("content-type", "application/zip").body(CSV),
                _ => Reply::new(200).header("content-type", "application/zip").body(&large),
            },
            Some("json") => Reply::json(200, r#"{"count":2,"offset":0,"limit":2,"observations":[{"date":"2024-01-01","value":".","realtime_start":"2024-01-01","realtime_end":"2024-01-01"},{"date":"2024-02-01","value":"NA","realtime_start":"2024-01-01","realtime_end":"2024-01-01"}]}"#),
            _ if offset == ["9"] => Reply::text(200, "application/xml", "<observations>"),
            _ => Reply::text(200, "application/xml", format!(r#"<observations count="2" offset="{}" limit="1"><observation date="2024-0{}-01" value="{}" realtime_start="2024-01-01" realtime_end="2024-01-01"/></observations>"#, offset[0], if offset == ["0"] {1} else {2}, if offset == ["0"] {"."} else {"102.00"})),
        }
    }).await;
    let api = api(&server);
    let series = api.series("INDPRO");
    assert!(matches!(
        ObservationLimit::new(100001),
        Err(Error::InvalidLimit { maximum: 100000 })
    ));
    let template = series
        .observations()
        .set_limit(ObservationLimit::new(1).unwrap());
    let mut walk = pages(
        template.clone(),
        Resume::Start,
        NonZeroUsize::new(1).unwrap(),
    );
    let partial = walk.next_page().await.unwrap().unwrap().items;
    assert_eq!(partial[0].value, ".");
    let bounded = walk.next_page().await.unwrap_err();
    assert!(matches!(bounded, PagesError::Bounded { .. }));
    assert_eq!(bounded.resume(), Some(&Resume::At(1)));
    let mut resumed = pages(template, Resume::At(1), NonZeroUsize::new(2).unwrap());
    assert_eq!(
        resumed.next_page().await.unwrap().unwrap().items[0].value,
        "102.00"
    );
    assert!(resumed.next_page().await.unwrap().is_none());
    let json = series
        .observations_json()
        .set_limit(ObservationLimit::new(2).unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!((json.count, json.offset, json.limit), (2, 0, 2));
    assert_eq!(
        json.observations
            .iter()
            .map(|o| o.value.as_str())
            .collect::<Vec<_>>(),
        [".", "NA"]
    );
    let export = series
        .export_csv()
        .set_limit(ObservationLimit::new(2).unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(export.csv(), CSV);
    assert_eq!(export.readme(), Some(&b"Fixture export"[..]));
    concord_kind(
        series.export_csv().set_offset(1).send().await.unwrap_err(),
        ErrorKind::Metadata,
    );
    concord_kind(
        series.export_csv().set_offset(2).send().await.unwrap_err(),
        ErrorKind::Decode,
    );
    concord_kind(
        series.export_csv().set_offset(3).send().await.unwrap_err(),
        ErrorKind::Decode,
    );
    concord_kind(
        series
            .observations()
            .set_offset(9)
            .send()
            .await
            .unwrap_err(),
        ErrorKind::Decode,
    );
    let requests = server.requests();
    assert_eq!(requests.len(), 8);
    let base = [("api_key", KEY), ("series_id", "INDPRO")];
    for (i, limit, offset, kind, accept) in [
        (0, "1", "0", None, "text/xml"),
        (1, "1", "1", None, "text/xml"),
        (2, "2", "0", Some("json"), "application/json"),
        (3, "2", "0", Some("csv"), "application/zip"),
        (4, "100000", "1", Some("csv"), "application/zip"),
        (5, "100000", "2", Some("csv"), "application/zip"),
        (6, "100000", "3", Some("csv"), "application/zip"),
        (7, "100000", "9", None, "text/xml"),
    ] {
        let mut expected = base.to_vec();
        expected.extend([("limit", limit), ("offset", offset)]);
        if let Some(kind) = kind {
            expected.push(("file_type", kind));
        }
        check_wire(&requests[i], "/fred/series/observations", &expected, accept);
    }
    server.shutdown().await;
}

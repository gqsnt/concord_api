//! Independent caller workflows, derived from public documentation only.

use std::time::Duration;

use concord::{ErrorKind, reqwest};
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use concord_sdk_nasa_neows::{Asteroid, BrowsePage, Error, Feed, NeoWs, NeoWsConfig};

// Deliberately synthetic; reserved characters exercise query-value encoding.
const KEY: &str = "first-use-key+&=?/ sentinel";
const EROS: &str = r#"{
    "id":"2000433", "name":"433 Eros (A898 PA)",
    "is_potentially_hazardous_asteroid":false,
    "absolute_magnitude_h":10.41,
    "estimated_diameter":{"kilometers":{"estimated_diameter_min":22.0067}}
}"#;
const GANYMED: &str = r#"{"id":"2001036","name":"1036 Ganymed (A924 UB)"}"#;
const FEED: &str = r#"{
    "element_count":2,
    "links":{"self":"https://fixture.invalid/feed?api_key=first-use-key"},
    "near_earth_objects":{
        "2015-09-07":[{"id":"2000433","name":"433 Eros (A898 PA)",
            "is_potentially_hazardous_asteroid":false}],
        "2015-09-08":[{"id":"2001036","name":"1036 Ganymed (A924 UB)"}]
    }
}"#;
const PAGE_ZERO: &str = r#"{
    "page":{"size":1,"total_elements":2,"total_pages":2,"number":0},
    "links":{"next":"https://fixture.invalid/never-follow?api_key=first-use-key"},
    "near_earth_objects":[{"id":"2000433","name":"433 Eros (A898 PA)",
        "is_potentially_hazardous_asteroid":false}]
}"#;
const PAGE_ONE: &str = r#"{
    "page":{"size":1,"total_elements":2,"total_pages":2,"number":1},
    "near_earth_objects":[{"id":"2001036","name":"1036 Ganymed (A924 UB)"}]
}"#;
const EMPTY_FEED: &str = r#"{"element_count":0,"near_earth_objects":{}}"#;
const EMPTY_PAGE: &str = r#"{
    "page":{"size":20,"total_elements":0,"total_pages":0,"number":0},
    "near_earth_objects":[]
}"#;

fn client(server: &TestServer) -> NeoWs {
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .no_proxy()
        .build()
        .unwrap();
    NeoWs::new(
        http,
        NeoWsConfig {
            api_key: KEY.into(),
        },
    )
    .with_base_url(format!("{}/neo/rest/v1", server.url()))
    .unwrap()
}

fn assert_wire(request: &Recorded, path: &str, query: &[(&str, &str)]) {
    assert_eq!(request.method, "GET");
    assert_eq!(request.path(), path);
    let mut actual = request.query_pairs();
    let mut expected = vec![("api_key".to_owned(), KEY.to_owned())];
    expected.extend(query.iter().map(|(k, v)| (k.to_string(), v.to_string())));
    actual.sort();
    expected.sort();
    assert_eq!(actual, expected);
    assert_eq!(request.header("accept"), Some("application/json"));
    assert_eq!(request.header("authorization"), None);
    assert_eq!(request.header("x-api-key"), None);
    assert_eq!(request.header("content-type"), None);
    assert!(request.host.starts_with("127.0.0.1:"));
    assert!(request.body.is_empty());
    assert_eq!(request.body_len, 0);
    assert!(!request.chunked);
}

fn provider_failure(status: u16, code: &str) -> Reply {
    Reply::json(
        status,
        format!(r#"{{"error":{{"code":"{code}","message":"body-echo {KEY}"}}}}"#,),
    )
    .header("x-ratelimit-remaining", "0")
    .header("retry-after", "60")
    .header("x-provider-diagnostic", format!("header-echo {KEY}"))
}

fn assert_safe_error(error: &Error, kind: ErrorKind, status: Option<u16>) {
    assert_eq!(error.kind, kind);
    assert_eq!(error.status.map(|status| status.as_u16()), status);
    // A common prefix catches both raw and URL-encoded forms of the test key.
    for rendered in [
        format!("{error}"),
        format!("{error:?}"),
        format!("{error:#?}"),
    ] {
        for forbidden in [
            KEY,
            "first-use-key",
            "body-echo",
            "header-echo",
            "http://",
            "API_KEY_INVALID",
            "OVER_RATE_LIMIT",
        ] {
            assert!(!rendered.contains(forbidden), "unsafe error diagnostic");
        }
    }
    assert!(std::error::Error::source(error).is_none());
}

#[derive(Default)]
struct FeedInspection {
    feed: Option<Feed>,
    details: Vec<Asteroid>,
    complete: bool,
    error: Option<Error>,
}

// Keep the successful feed and prior details if an inspection fails.
async fn inspect_feed(api: &NeoWs) -> FeedInspection {
    let neo = api.neows();
    let mut report = FeedInspection::default();
    let feed = match neo.feed("2015-09-07", "2015-09-08").send().await {
        Ok(feed) => feed,
        Err(error) => {
            report.error = Some(error);
            return report;
        }
    };
    for object in feed.near_earth_objects.values().flatten() {
        match neo.lookup(&object.id).send().await {
            Ok(detail) => report.details.push(detail),
            Err(error) => {
                report.error = Some(error);
                break;
            }
        }
    }
    report.feed = Some(feed);
    report.complete = report.error.is_none();
    report
}

#[derive(Default)]
struct BrowseInspection {
    pages: Vec<BrowsePage>,
    details: Vec<Asteroid>,
    complete: bool,
    error: Option<Error>,
}

// This small caller explicitly visits at most three pages; it never follows links.
async fn inspect_browse(api: &NeoWs) -> BrowseInspection {
    let neo = api.neows();
    let mut report = BrowseInspection::default();
    for number in 0..3_u64 {
        let page = match neo.browse().set_page(number).set_size(1).send().await {
            Ok(page) => page,
            Err(error) => {
                report.error = Some(error);
                return report;
            }
        };
        // Completion is an application decision based on this bounded page.
        let terminal = page.page.total_pages == 0
            || page
                .page
                .number
                .checked_add(1)
                .is_some_and(|n| n >= page.page.total_pages);
        for object in &page.near_earth_objects {
            match neo.lookup(&object.id).send().await {
                Ok(detail) => report.details.push(detail),
                Err(error) => {
                    report.error = Some(error);
                    break;
                }
            }
        }
        report.pages.push(page);
        if report.error.is_some() {
            return report;
        }
        if terminal {
            report.complete = true;
            return report;
        }
    }
    report
}

#[tokio::test]
async fn date_range_feed_then_inspect() {
    let server = TestServer::start(|request: &Recorded| -> Reply {
        match request.path() {
            "/neo/rest/v1/feed" => Reply::json(200, FEED)
                .header("x-ratelimit-limit", "1000")
                .header("x-ratelimit-remaining", "997"),
            "/neo/rest/v1/neo/2000433" => Reply::json(200, EROS),
            "/neo/rest/v1/neo/2001036" => Reply::json(200, GANYMED),
            _ => Reply::new(404),
        }
    })
    .await;
    let report = inspect_feed(&client(&server)).await;
    assert!(
        report.complete,
        "{:?}; observed paths: {:?}",
        report.error,
        server
            .requests()
            .iter()
            .map(Recorded::path)
            .collect::<Vec<_>>()
    );
    assert!(report.error.is_none());
    let feed = report.feed.as_ref().unwrap();
    assert_eq!(feed.element_count, 2);
    assert_eq!(feed.near_earth_objects.len(), 2);
    assert!(!format!("{feed:?}").contains("first-use-key"));
    assert_eq!(report.details.len(), 2);
    assert_eq!(
        report.details[0].id,
        feed.near_earth_objects["2015-09-07"][0].id
    );
    assert_eq!(report.details[0].name, "433 Eros (A898 PA)");
    assert_eq!(
        report.details[0].is_potentially_hazardous_asteroid,
        Some(false)
    );
    assert_eq!(report.details[1].is_potentially_hazardous_asteroid, None);
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert_wire(
        &requests[0],
        "/neo/rest/v1/feed",
        &[("start_date", "2015-09-07"), ("end_date", "2015-09-08")],
    );
    assert_wire(&requests[1], "/neo/rest/v1/neo/2000433", &[]);
    assert_wire(&requests[2], "/neo/rest/v1/neo/2001036", &[]);
    server.shutdown().await;

    // Empty success is completed work, not a missing-response failure.
    let server = TestServer::start(|_: &Recorded| Reply::json(200, EMPTY_FEED)).await;
    let report = inspect_feed(&client(&server)).await;
    assert!(report.complete);
    assert!(report.error.is_none());
    assert_eq!(report.feed.unwrap().element_count, 0);
    assert!(report.details.is_empty());
    assert_wire(
        &server.single(),
        "/neo/rest/v1/feed",
        &[("start_date", "2015-09-07"), ("end_date", "2015-09-08")],
    );
    server.shutdown().await;

    // No whole-feed partial value escapes if that response itself fails.
    for (case, kind, status) in [
        ("invalid_key", ErrorKind::Status, 403),
        ("quota_stalled_body", ErrorKind::Status, 429),
        ("malformed", ErrorKind::Decode, 200),
        ("provider_envelope_on_200", ErrorKind::Decode, 200),
        ("wrong_content_type", ErrorKind::Metadata, 200),
        ("oversized", ErrorKind::Limit, 200),
    ] {
        let server = TestServer::start(move |_: &Recorded| -> Reply {
            match case {
                "invalid_key" => provider_failure(403, "API_KEY_INVALID"),
                "quota_stalled_body" => {
                    provider_failure(429, "OVER_RATE_LIMIT").with_body(Body::Stall)
                }
                "malformed" => Reply::json(200, &FEED[..FEED.len() - 1]),
                "provider_envelope_on_200" => provider_failure(200, "API_KEY_INVALID"),
                "wrong_content_type" => Reply::new(200)
                    .header("content-type", "text/html")
                    .with_body(Body::Stall),
                "oversized" => Reply::json(
                    200,
                    format!(
                        r#"{{"element_count":0,"near_earth_objects":{{}},"padding":"{}"}}"#,
                        "x".repeat(4 * 1024 * 1024),
                    ),
                ),
                _ => unreachable!(),
            }
        })
        .await;
        let report = inspect_feed(&client(&server)).await;
        assert!(!report.complete, "{case}");
        assert!(report.feed.is_none(), "{case}");
        assert!(report.details.is_empty(), "{case}");
        assert_safe_error(report.error.as_ref().expect(case), kind, Some(status));
        assert_wire(
            &server.single(),
            "/neo/rest/v1/feed",
            &[("start_date", "2015-09-07"), ("end_date", "2015-09-08")],
        );
        if matches!(case, "quota_stalled_body" | "wrong_content_type") {
            assert_eq!(server.body_bytes_sent(), 0);
        }
        server.shutdown().await;
    }

    // A quota failure on the second inspection retains both the feed and Eros.
    let server = TestServer::start(|request: &Recorded| -> Reply {
        match request.path() {
            "/neo/rest/v1/feed" => Reply::json(200, FEED),
            "/neo/rest/v1/neo/2000433" => Reply::json(200, EROS),
            "/neo/rest/v1/neo/2001036" => provider_failure(429, "OVER_RATE_LIMIT"),
            _ => Reply::new(404),
        }
    })
    .await;
    let api = client(&server);
    for debug in [
        format!("{api:?}"),
        format!(
            "{:?}",
            NeoWsConfig {
                api_key: KEY.into()
            }
        ),
        format!("{:?}", api.neows()),
        format!("{:?}", api.neows().feed("2015-09-07", "2015-09-08")),
        format!("{:?}", api.neows().browse()),
        format!("{:?}", api.neows().lookup("2000433")),
    ] {
        assert!(!debug.contains("first-use-key"));
    }
    let report = inspect_feed(&api).await;
    assert!(!report.complete);
    assert_eq!(report.feed.as_ref().unwrap().element_count, 2);
    assert_eq!(report.details.len(), 1);
    assert_eq!(report.details[0].id, "2000433");
    assert_safe_error(report.error.as_ref().unwrap(), ErrorKind::Status, Some(429));
    let requests = server.requests();
    assert_eq!(requests.len(), 3, "quota failure must not retry");
    assert_wire(
        &requests[0],
        "/neo/rest/v1/feed",
        &[("start_date", "2015-09-07"), ("end_date", "2015-09-08")],
    );
    assert_wire(&requests[1], "/neo/rest/v1/neo/2000433", &[]);
    assert_wire(&requests[2], "/neo/rest/v1/neo/2001036", &[]);
    server.shutdown().await;

    // A refused loopback connection must not expose the key in reqwest's URL.
    let error = api.neows().lookup("2000433").send().await.unwrap_err();
    assert_safe_error(&error, ErrorKind::Transport, None);

    // Date strings are sent as supplied; provider status owns range rejection.
    let server = TestServer::start(|_: &Recorded| provider_failure(400, "BAD_DATE")).await;
    let error = client(&server)
        .neows()
        .feed("not-a-date", "2015-09-08")
        .send()
        .await
        .unwrap_err();
    assert_safe_error(&error, ErrorKind::Status, Some(400));
    assert_wire(
        &server.single(),
        "/neo/rest/v1/feed",
        &[("start_date", "not-a-date"), ("end_date", "2015-09-08")],
    );
    server.shutdown().await;
}

#[tokio::test]
async fn browse_then_inspect() {
    let server = TestServer::start(|request: &Recorded| -> Reply {
        match request.path() {
            "/neo/rest/v1/neo/browse" => match request.query_all("page").as_slice() {
                [page] if page == "0" => Reply::json(200, PAGE_ZERO),
                [page] if page == "1" => Reply::json(200, PAGE_ONE),
                _ => Reply::new(404),
            },
            "/neo/rest/v1/neo/2000433" => Reply::json(200, EROS),
            "/neo/rest/v1/neo/2001036" => Reply::json(200, GANYMED),
            _ => Reply::new(404),
        }
    })
    .await;
    let report = inspect_browse(&client(&server)).await;
    assert!(
        report.complete,
        "{:?}; observed paths: {:?}",
        report.error,
        server
            .requests()
            .iter()
            .map(Recorded::path)
            .collect::<Vec<_>>()
    );
    assert!(report.error.is_none());
    assert_eq!(report.pages.len(), 2);
    assert_eq!(report.details.len(), 2);
    assert_eq!(report.pages[0].page.total_elements, 2);
    assert_eq!(report.pages[0].page.total_pages, 2);
    assert_eq!(report.pages[0].page.size, 1);
    assert_eq!(report.pages[1].page.number, 1);
    assert!(!format!("{:?}", report.pages).contains("first-use-key"));
    assert_eq!(
        report.details[0].id,
        report.pages[0].near_earth_objects[0].id
    );
    let requests = server.requests();
    assert_eq!(requests.len(), 4);
    assert_wire(
        &requests[0],
        "/neo/rest/v1/neo/browse",
        &[("page", "0"), ("size", "1")],
    );
    assert_wire(&requests[1], "/neo/rest/v1/neo/2000433", &[]);
    assert_wire(
        &requests[2],
        "/neo/rest/v1/neo/browse",
        &[("page", "1"), ("size", "1")],
    );
    assert_wire(&requests[3], "/neo/rest/v1/neo/2001036", &[]);
    server.shutdown().await;

    // Defaults are omitted on the wire. An empty dataset has no object to inspect.
    let server = TestServer::start(|_: &Recorded| Reply::json(200, EMPTY_PAGE)).await;
    let api = client(&server);
    let page = api.neows().browse().send().await.unwrap();
    assert_eq!(page.page.total_pages, 0);
    assert_eq!(page.page.total_elements, 0);
    assert!(page.near_earth_objects.is_empty());
    assert_wire(&server.single(), "/neo/rest/v1/neo/browse", &[]);
    let report = inspect_browse(&api).await;
    assert!(report.complete);
    assert!(report.error.is_none());
    assert_eq!(report.pages.len(), 1);
    assert!(report.details.is_empty());
    assert_eq!(server.requests().len(), 2);
    assert_wire(
        &server.requests()[1],
        "/neo/rest/v1/neo/browse",
        &[("page", "0"), ("size", "1")],
    );
    server.shutdown().await;

    // Later-page and later-inspection failures retain all previously completed work.
    for (case, kind, status, retained_pages) in [
        ("quota_on_page", ErrorKind::Status, 429, 1),
        ("malformed_page", ErrorKind::Decode, 200, 1),
        ("invalid_key_on_lookup", ErrorKind::Status, 403, 2),
        ("malformed_lookup", ErrorKind::Decode, 200, 2),
    ] {
        let server = TestServer::start(move |request: &Recorded| -> Reply {
            match request.path() {
                "/neo/rest/v1/neo/browse" => match request.query_all("page").as_slice() {
                    [page] if page == "0" => Reply::json(200, PAGE_ZERO),
                    [page] if page == "1" => match case {
                        "quota_on_page" => provider_failure(429, "OVER_RATE_LIMIT"),
                        "malformed_page" => Reply::json(200, &PAGE_ONE[..PAGE_ONE.len() - 1]),
                        _ => Reply::json(200, PAGE_ONE),
                    },
                    _ => Reply::new(404),
                },
                "/neo/rest/v1/neo/2000433" => Reply::json(200, EROS),
                "/neo/rest/v1/neo/2001036" => match case {
                    "invalid_key_on_lookup" => provider_failure(403, "API_KEY_INVALID"),
                    "malformed_lookup" => Reply::json(200, r#"{"id":"2001036","name":false}"#),
                    _ => Reply::json(200, GANYMED),
                },
                _ => Reply::new(404),
            }
        })
        .await;
        let report = inspect_browse(&client(&server)).await;
        assert!(!report.complete, "{case}");
        assert_eq!(report.pages.len(), retained_pages, "{case}");
        assert_eq!(report.details.len(), 1, "{case}");
        assert_eq!(report.details[0].id, "2000433");
        assert_eq!(report.pages[0].page.total_pages, 2);
        assert_safe_error(report.error.as_ref().unwrap(), kind, Some(status));
        let requests = server.requests();
        assert_eq!(requests.len(), if retained_pages == 1 { 3 } else { 4 });
        assert_wire(
            &requests[0],
            "/neo/rest/v1/neo/browse",
            &[("page", "0"), ("size", "1")],
        );
        assert_wire(&requests[1], "/neo/rest/v1/neo/2000433", &[]);
        assert_wire(
            &requests[2],
            "/neo/rest/v1/neo/browse",
            &[("page", "1"), ("size", "1")],
        );
        if retained_pages == 2 {
            assert_wire(&requests[3], "/neo/rest/v1/neo/2001036", &[]);
        }
        server.shutdown().await;
    }
}

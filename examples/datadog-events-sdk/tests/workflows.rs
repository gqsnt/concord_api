//! The two connected workflows end to end against local fixtures, and the required failure:
//! wrong-region keys and a missing application key are authentication errors that print
//! neither key.
//!
//! Every request goes to a loopback stand-in; the real Datadog API is never called. Hosts are
//! the documented API hosts, resolved to the stand-in, so `Host` is observable.

use std::num::NonZeroUsize;

use concord_example_testserver::{Recorded, Reply, Script, TestServer};
use concord_sdk_datadog_events::concord::reqwest;
use concord_sdk_datadog_events::pagination::{PagesError, Resume};
use concord_sdk_datadog_events::{
    AuthError, Credentials, Cursor, DatadogEvents, Error, PageLimit, SearchRequest, Site, Sort,
    TimeRange,
};
use serde_json::{Value, json};

const EU_API_KEY: &str = "synthetic-eu-api-key-00000000000000aa";
const EU_APP_KEY: &str = "synthetic-eu-app-key-0000000000000000000bb";

const LIST: &str = include_str!("../fixtures/list-range.synthetic.json");
const PAGE_1: &str = include_str!("../fixtures/search-page-1.synthetic.json");
const PAGE_2: &str = include_str!("../fixtures/search-page-2.synthetic.json");
const PAGE_3: &str = include_str!("../fixtures/search-page-3.synthetic.json");
const RATE_LIMITED: &str = include_str!("../fixtures/error.doc.json");
const FORBIDDEN: &str = include_str!("../fixtures/error-403-forbidden.synthetic.json");

const US1_HOST: &str = "api.datadoghq.com";
const EU_HOST: &str = "api.datadoghq.eu";

fn http(server: &TestServer) -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .resolve(US1_HOST, server.addr())
        .resolve(EU_HOST, server.addr())
        .build()
        .unwrap()
}

/// A client for `site` whose requests reach the stand-in at the documented API host `host`.
fn client(server: &TestServer, site: Site, host: &str, credentials: Credentials) -> DatadogEvents {
    DatadogEvents::new(http(server), site, credentials)
        .with_base_url(format!("http://{host}:{}", server.addr().port()))
        .unwrap()
}

fn eu_credentials() -> Credentials {
    Credentials::new(EU_API_KEY, EU_APP_KEY).unwrap()
}

fn eu(server: &TestServer) -> DatadogEvents {
    client(server, Site::Eu, EU_HOST, eu_credentials())
}

/// The stand-in's view of one region: only the EU host knows the EU keys.
fn eu_region(request: &Recorded, ok: impl FnOnce() -> Reply) -> Reply {
    let host_is_eu = request.host.split(':').next() == Some(EU_HOST);
    let keys_are_eu = request.header("dd-api-key") == Some(EU_API_KEY)
        && request.header("dd-application-key") == Some(EU_APP_KEY);
    if host_is_eu && keys_are_eu {
        ok()
    } else {
        Reply::json(403, FORBIDDEN)
    }
}

fn body(request: &Recorded) -> Value {
    serde_json::from_slice(&request.body).unwrap()
}

fn assert_hides_keys(text: &str) {
    assert!(!text.contains(EU_API_KEY), "API key printed: {text}");
    assert!(
        !text.contains(EU_APP_KEY),
        "application key printed: {text}"
    );
}

// Workflow 1 ---------------------------------------------------------------------------------

#[tokio::test]
async fn list_a_bounded_time_range_then_inspect_one_event() {
    let server = TestServer::start(|request| eu_region(request, || Reply::json(200, LIST))).await;
    let events = eu(&server);

    // 2026-01-01T00:00:00Z ..= 01:00:00Z, newest first, at most 50.
    let range = TimeRange::from_millis(1_767_225_600_000, 1_767_229_200_000).unwrap();
    let page = events
        .list(range)
        .set_query("service:checkout")
        .set_sort(Sort::Descending)
        .set_limit(PageLimit::new(50).unwrap())
        .send()
        .await
        .unwrap();

    let request = server.single();
    assert_eq!(request.method, "GET");
    assert_eq!(request.path(), "/api/v2/events");
    assert_eq!(request.query_all("filter[from]"), ["1767225600000"]);
    assert_eq!(request.query_all("filter[to]"), ["1767229200000"]);
    assert_eq!(request.query_all("filter[query]"), ["service:checkout"]);
    assert_eq!(request.query_all("sort"), ["-timestamp"]);
    assert_eq!(request.query_all("page[limit]"), ["50"]);

    // The page: two events, no next cursor, no warnings.
    let ids: Vec<_> = page.data.iter().map(|e| e.id.as_deref().unwrap()).collect();
    assert_eq!(ids, ["synthetic-event-0001", "synthetic-event-0002"]);
    assert_eq!(page.after(), None);
    assert!(page.warnings().is_empty());
    assert_eq!(
        page.meta.as_ref().unwrap().request_id.as_deref(),
        Some("synthetic-request-list")
    );

    // Inspect the second event: its common attributes, then its event-specific ones (raw).
    let event = page.event("synthetic-event-0002").unwrap();
    assert_eq!(event.kind.as_deref(), Some("event"));
    assert_eq!(event.message(), Some("Deploy of checkout 1.4.2 finished"));
    assert_eq!(event.tags(), ["env:prod", "team:payments", "version:1.4.2"]);
    assert_eq!(event.timestamp(), Some("2026-01-01T00:05:00.000Z"));
    assert_eq!(event.attribute("title"), Some(&json!("Deploy finished")));
    assert_eq!(event.attribute("status"), Some(&json!("success")));
    assert_eq!(
        event.attribute("evt").and_then(|evt| evt["uid"].as_str()),
        Some("SyntheticUid000000000002")
    );
    server.shutdown().await;
}

// Workflow 2 ---------------------------------------------------------------------------------

fn search_request() -> SearchRequest {
    SearchRequest::new()
        .query("source:datadog-agent")
        .from("now-15m")
        .to("now")
        .timezone("GMT")
        .sort(Sort::Ascending)
        .limit(PageLimit::new(2).unwrap())
}

/// The body every page sends: the same filter, options, sort and limit, plus the cursor.
fn expected_body(cursor: Option<&str>) -> Value {
    let mut page = json!({"limit": 2});
    if let Some(cursor) = cursor {
        page["cursor"] = json!(cursor);
    }
    json!({
        "filter": {"query": "source:datadog-agent", "from": "now-15m", "to": "now"},
        "options": {"timezone": "GMT"},
        "page": page,
        "sort": "timestamp",
    })
}

fn scripted(pages: Vec<Reply>) -> impl Fn(&Recorded) -> Reply + Send + Sync + 'static {
    let script = Script::new(pages);
    move |request: &Recorded| eu_region(request, || script.next())
}

#[tokio::test]
async fn search_follows_the_documented_cursor_to_the_last_page() {
    let server = TestServer::start(scripted(vec![
        Reply::json(200, PAGE_1),
        Reply::json(200, PAGE_2),
        Reply::json(200, PAGE_3),
    ]))
    .await;
    let mut pages = eu(&server).search_pages(search_request());

    let mut ids = Vec::new();
    let mut warnings = Vec::new();
    let mut request_ids = Vec::new();
    while let Some(page) = pages.next_page().await.unwrap() {
        ids.extend(page.items.iter().map(|e| e.id.clone().unwrap()));
        warnings.push(page.meta.warnings.len());
        request_ids.push(page.meta.request_id.clone().unwrap());
    }
    // `Ok(None)`: the last page had no `meta.page.after`.
    assert_eq!(
        ids,
        [
            "synthetic-search-0001",
            "synthetic-search-0002",
            "synthetic-search-0003",
            "synthetic-search-0004",
            "synthetic-search-0005",
        ]
    );
    assert_eq!(warnings, [0, 1, 0], "page 2's warning is kept with page 2");
    assert_eq!(
        request_ids,
        [
            "synthetic-request-search-1",
            "synthetic-request-search-2",
            "synthetic-request-search-3"
        ]
    );
    assert!(pages.checkpoint().is_none());

    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    for (request, cursor) in
        requests
            .iter()
            .zip([None, Some("synthetic-cursor-1"), Some("synthetic-cursor-2")])
    {
        assert_eq!(request.method, "POST");
        assert_eq!(request.target, "/api/v2/events/search");
        assert_eq!(body(request), expected_body(cursor));
    }
    server.shutdown().await;
}

#[tokio::test]
async fn search_items_collect_completely() {
    let server = TestServer::start(scripted(vec![
        Reply::json(200, PAGE_1),
        Reply::json(200, PAGE_2),
        Reply::json(200, PAGE_3),
    ]))
    .await;
    let events = eu(&server)
        .search_pages(search_request())
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(events.len(), 5);
    assert_eq!(
        events[4].message(),
        Some("datadog-agent restarted on host-e")
    );
    server.shutdown().await;
}

#[tokio::test]
async fn a_page_cap_is_not_the_end_and_the_search_resumes() {
    let server = TestServer::start(scripted(vec![
        Reply::json(200, PAGE_1),
        Reply::json(200, PAGE_2),
        Reply::json(200, PAGE_3),
    ]))
    .await;
    let events = eu(&server);
    let cap = NonZeroUsize::new(2).unwrap();
    let incomplete = events
        .search_pages(search_request())
        .max_pages(cap)
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 4);
    let PagesError::Bounded { resume, .. } = &incomplete.error else {
        panic!("{:?}", incomplete.error);
    };
    assert_eq!(resume, &Resume::At(Cursor::new("synthetic-cursor-2")));
    assert_eq!(server.requests().len(), 2, "the cap sends no third request");

    // Later: continue from the checkpoint with the same request.
    let rest = events
        .search_pages(search_request())
        .starting_at(resume.clone())
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(rest.len(), 1);
    assert_eq!(rest[0].id.as_deref(), Some("synthetic-search-0005"));
    let requests = server.requests();
    assert_eq!(
        body(&requests[2]),
        expected_body(Some("synthetic-cursor-2"))
    );
    server.shutdown().await;
}

#[tokio::test]
async fn a_failed_page_keeps_the_prefix_and_the_cursor_to_retry() {
    let server = TestServer::start(scripted(vec![
        Reply::json(200, PAGE_1),
        Reply::json(429, RATE_LIMITED),
    ]))
    .await;
    let incomplete = eu(&server)
        .search_pages(search_request())
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 2);
    assert_eq!(
        incomplete.error.resume(),
        Some(&Resume::At(Cursor::new("synthetic-cursor-1")))
    );
    let error = incomplete.error.provider_error().unwrap();
    assert!(matches!(error, Error::RateLimited(_)), "{error:?}");
    assert_eq!(error.status().map(|s| s.as_u16()), Some(429));
    assert_eq!(server.requests().len(), 2, "no retry");
    server.shutdown().await;
}

// The required failure -----------------------------------------------------------------------

#[tokio::test]
async fn wrong_region_keys_are_an_authentication_error_that_hides_both_keys() {
    let server = TestServer::start(|request| eu_region(request, || Reply::json(200, LIST))).await;
    // EU keys, but the client is configured for US1: requests go to the US1 API host.
    let wrong = client(&server, Site::Us1, US1_HOST, eu_credentials());
    let range = TimeRange::from_millis(1_767_225_600_000, 1_767_229_200_000).unwrap();

    let listed = wrong.list(range).send().await.unwrap_err();
    let searched = wrong
        .search_pages(search_request())
        .next_page()
        .await
        .unwrap_err()
        .into_provider_error()
        .unwrap()
        .0;
    for error in [&listed, &searched] {
        let Error::Authentication(AuthError::Rejected(rejected)) = error else {
            panic!("{error:?}");
        };
        assert_eq!(rejected.site(), Some(Site::Us1));
        assert_eq!(rejected.host(), US1_HOST);
        assert_eq!(rejected.provider().status().as_u16(), 403);
        assert_eq!(
            rejected.provider().messages(),
            Some(&["Forbidden".to_owned()][..])
        );
        assert!(error.is_authentication());
        let shown = error.to_string();
        assert!(shown.contains("US1 (api.datadoghq.com)"), "{shown}");
        assert!(shown.contains("403"), "{shown}");
        assert_hides_keys(&shown);
        assert_hides_keys(&format!("{error:?}"));
    }
    let hosts: Vec<_> = server
        .requests()
        .iter()
        .map(|r| r.host.split(':').next().unwrap().to_owned())
        .collect();
    assert_eq!(hosts, [US1_HOST, US1_HOST]);

    // The same keys on their own site succeed: the region, not the keys, was wrong.
    let right = eu(&server);
    assert_eq!(right.list(range).send().await.unwrap().data.len(), 2);
    server.shutdown().await;
}

#[tokio::test]
async fn an_application_key_the_site_does_not_know_is_an_authentication_error() {
    let server = TestServer::start(|request| eu_region(request, || Reply::json(200, LIST))).await;
    let unknown = Credentials::new(EU_API_KEY, "synthetic-app-key-of-another-org").unwrap();
    let error = client(&server, Site::Eu, EU_HOST, unknown)
        .search(SearchRequest::new())
        .send()
        .await
        .unwrap_err();
    assert!(error.is_authentication(), "{error:?}");
    assert_eq!(error.status().map(|s| s.as_u16()), Some(403));
    assert_hides_keys(&error.to_string());
    assert_hides_keys(&format!("{error:?}"));
    assert!(!error.to_string().contains("another-org"));
    server.shutdown().await;
}

#[tokio::test]
async fn a_missing_application_key_is_an_authentication_error_before_anything_is_sent() {
    let server = TestServer::start(|request| eu_region(request, || Reply::json(200, LIST))).await;
    // The application key was not configured (an unset variable read as empty).
    let error = Credentials::new(EU_API_KEY, String::new()).unwrap_err();
    assert_eq!(error, AuthError::MissingApplicationKey);
    assert!(error.is_local());
    let error = Error::from(error);
    assert!(error.is_authentication());
    assert_eq!(error.status(), None);
    let shown = error.to_string();
    assert!(shown.contains("application key"), "{shown}");
    assert!(shown.contains("nothing was sent"), "{shown}");
    assert_hides_keys(&shown);
    assert_hides_keys(&format!("{error:?}"));
    // No client could be built, so the stand-in saw nothing.
    assert!(server.requests().is_empty());
    server.shutdown().await;
}

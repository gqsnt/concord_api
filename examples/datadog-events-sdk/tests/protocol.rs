//! Wire-level protocol: site hosts, request shapes, every documented error class, malformed,
//! truncated, oversized and wrong-media bodies, and redaction of both keys.
//!
//! Fixtures named `*.doc.json` are copied from the cited documentation; `*.synthetic.json` are
//! authored (see PROVENANCE.md). Expected values below come from those fixtures or from the
//! documentation's text, never from the SDK's output.

use concord_example_testserver::{Body, Reply, TestServer};
use concord_sdk_datadog_events::concord::ErrorKind;
use concord_sdk_datadog_events::concord::reqwest;
use concord_sdk_datadog_events::{
    AuthError, Credentials, DatadogApi, DatadogEvents, Destination, ERROR_BODY_LIMIT, Error,
    ErrorBodyError, InputError, Key, PageLimit, ProtocolError, RESPONSE_LIMIT, SearchRequest, Site,
    Sort, TimeRange,
};
use serde_json::Value;

const API_KEY: &str = "synthetic-dd-api-key-0123456789abcdef";
const APP_KEY: &str = "synthetic-dd-app-key-fedcba98765432100123";

const EVENTS_200: &str = include_str!("../fixtures/events-200.doc.json");
const ERROR_DOC: &str = include_str!("../fixtures/error.doc.json");
const SEARCH_REQUEST_1: &str = include_str!("../fixtures/search-request-1.doc.json");
const SEARCH_REQUEST_2: &str = include_str!("../fixtures/search-request-2.doc.json");
const SITES: &str = include_str!("../fixtures/sites.doc.json");
const PAGE_1: &str = include_str!("../fixtures/search-page-1.synthetic.json");
const EMPTY_CURSOR: &str = include_str!("../fixtures/search-page-empty-cursor.synthetic.json");

fn credentials() -> Credentials {
    Credentials::new(API_KEY, APP_KEY).unwrap()
}

/// A transport like the SDK's own (no redirects, no retries), without system proxies, with
/// `hosts` resolved to the loopback server.
fn http(server: &TestServer, hosts: &[&str]) -> reqwest::Client {
    let mut builder = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never());
    for host in hosts {
        builder = builder.resolve(host, server.addr());
    }
    builder.build().unwrap()
}

/// A client for `site` whose requests reach the loopback server at the API host `host`.
fn at_host(server: &TestServer, site: Site, host: &str) -> DatadogEvents {
    DatadogEvents::new(
        http(server, &[host]),
        Destination::at(site, format!("http://{host}:{}", server.addr().port())).unwrap(),
        credentials(),
    )
}

fn local(server: &TestServer) -> DatadogEvents {
    DatadogEvents::new(
        http(server, &[]),
        Destination::at(Site::Us1, server.url()).unwrap(),
        credentials(),
    )
}

fn range() -> TimeRange {
    TimeRange::from_millis(1_767_225_600_000, 1_767_229_200_000).unwrap()
}

fn assert_no_secret(text: &str) {
    assert!(!text.contains(API_KEY), "API key printed: {text}");
    assert!(!text.contains(APP_KEY), "application key printed: {text}");
}

fn assert_error_hides_keys(error: &Error) {
    assert_no_secret(&error.to_string());
    assert_no_secret(&format!("{error:?}"));
}

fn json(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

#[test]
fn sites_match_the_documented_table() {
    let rows = json(SITES);
    let rows = rows.as_array().unwrap();
    assert_eq!(rows.len(), Site::ALL.len());
    for row in rows {
        let dd_site = row["dd_site"].as_str().unwrap();
        let api_host = row["api_host"].as_str().unwrap();
        let site = Site::from_dd_site(dd_site).unwrap();
        assert_eq!(site.label(), row["label"].as_str().unwrap());
        assert_eq!(site.dd_site(), dd_site);
        assert_eq!(site.api_host(), api_host);
        assert_eq!(Site::from_api_host(api_host), Some(site));
        assert_eq!(dd_site.parse::<Site>(), Ok(site));

        let client = DatadogEvents::new(reqwest::Client::new(), site, credentials());
        assert_eq!(client.site(), site);
        let list = client.events().list().prepare().unwrap();
        assert_eq!(
            list.request().url().as_str(),
            format!("https://{api_host}/api/v2/events")
        );
        let search = client.search(SearchRequest::new()).prepare().unwrap();
        assert_eq!(
            search.request().url().as_str(),
            format!("https://{api_host}/api/v2/events/search")
        );
    }
    assert!(Site::from_dd_site("datadoghq.co").is_err());
    assert!(Site::from_dd_site("app.datadoghq.com").is_err());
    assert_eq!(Site::from_dd_site("DatadogHQ.EU"), Ok(Site::Eu));
}

#[test]
fn header_names_compose() {
    DatadogApi::validate_header_names().unwrap();
}

#[tokio::test]
async fn list_request_on_the_wire() {
    let server = TestServer::start(|_| Reply::json(200, EVENTS_200)).await;
    let client = at_host(&server, Site::Eu, "api.datadoghq.eu");
    let page = client
        .list(range())
        .set_query("source:deploy")
        .set_sort(Sort::Descending)
        .set_limit(PageLimit::new(25).unwrap())
        .set_cursor(concord_sdk_datadog_events::Cursor::new("c-1"))
        .send()
        .await
        .unwrap();
    assert_eq!(page.data.len(), 1);

    let request = server.single();
    assert_eq!(request.method, "GET");
    assert_eq!(request.path(), "/api/v2/events");
    assert_eq!(
        request.host,
        format!("api.datadoghq.eu:{}", server.addr().port())
    );
    let pairs: Vec<(String, String)> = request.query_pairs();
    let expected = [
        ("filter[query]", "source:deploy"),
        ("filter[from]", "1767225600000"),
        ("filter[to]", "1767229200000"),
        ("sort", "-timestamp"),
        ("page[cursor]", "c-1"),
        ("page[limit]", "25"),
    ];
    let expected: Vec<(String, String)> = expected
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect();
    assert_eq!(pairs, expected);
    // Concord form-encodes the brackets of the documented key spelling.
    assert!(
        request
            .query()
            .unwrap()
            .starts_with("filter%5Bquery%5D=source%3Adeploy&filter%5Bfrom%5D=")
    );
    assert_eq!(request.header("dd-api-key"), Some(API_KEY));
    assert_eq!(request.header("dd-application-key"), Some(APP_KEY));
    assert_eq!(request.header("accept"), Some("application/json"));
    assert_eq!(request.header("content-type"), None);
    assert!(request.body.is_empty());
    server.shutdown().await;
}

#[tokio::test]
async fn list_without_inputs_sends_no_query() {
    let server = TestServer::start(|_| Reply::json(200, EVENTS_200)).await;
    local(&server).events().list().send().await.unwrap();
    assert_eq!(server.single().target, "/api/v2/events");
    server.shutdown().await;
}

#[test]
fn search_bodies_match_the_documented_examples() {
    let client = DatadogEvents::new(reqwest::Client::new(), Site::Us1, credentials());
    let first = SearchRequest::new()
        .query("datadog-agent")
        .from("2020-09-17T11:48:36+01:00")
        .to("2020-09-17T12:48:36+01:00")
        .sort(Sort::Ascending)
        .limit(PageLimit::new(5).unwrap());
    let second = SearchRequest::new()
        .from("now-15m")
        .to("now")
        .timezone("GMT")
        .limit(PageLimit::new(2).unwrap())
        .sort(Sort::Ascending);
    for (request, documented) in [(first, SEARCH_REQUEST_1), (second, SEARCH_REQUEST_2)] {
        let prepared = client.search(request).prepare().unwrap();
        let body = prepared
            .request()
            .body()
            .and_then(|b| b.as_bytes())
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(body).unwrap(),
            json(documented)
        );
        let headers = prepared.request().headers();
        assert_eq!(headers["content-type"], "application/json");
        assert_eq!(headers["accept"], "application/json");
    }

    // An empty search sends `{}`: every documented default applies.
    let empty = client.search(SearchRequest::new()).prepare().unwrap();
    assert_eq!(
        empty.request().body().and_then(|b| b.as_bytes()),
        Some(&b"{}"[..])
    );
    // A time offset replaces a timezone: "either ... but not both".
    let offset = SearchRequest::new().timezone("GMT").time_offset(3600);
    let offset = client.search(offset).prepare().unwrap();
    let body: Value =
        serde_json::from_slice(offset.request().body().and_then(|b| b.as_bytes()).unwrap())
            .unwrap();
    assert_eq!(body, serde_json::json!({"options": {"timeOffset": 3600}}));
    // A range in milliseconds is sent as the documented string timestamps.
    let ranged = client
        .search(SearchRequest::new().range(range()))
        .prepare()
        .unwrap();
    let body: Value =
        serde_json::from_slice(ranged.request().body().and_then(|b| b.as_bytes()).unwrap())
            .unwrap();
    assert_eq!(
        body,
        serde_json::json!({"filter": {"from": "1767225600000", "to": "1767229200000"}})
    );
}

#[tokio::test]
async fn search_request_on_the_wire() {
    let server = TestServer::start(|_| Reply::json(200, EVENTS_200)).await;
    let request = SearchRequest::new().query("datadog-agent");
    local(&server).search(request).send().await.unwrap();
    let recorded = server.single();
    assert_eq!(recorded.method, "POST");
    assert_eq!(recorded.target, "/api/v2/events/search");
    assert_eq!(recorded.header("content-type"), Some("application/json"));
    assert_eq!(recorded.header("dd-api-key"), Some(API_KEY));
    assert_eq!(recorded.header("dd-application-key"), Some(APP_KEY));
    assert_eq!(
        serde_json::from_slice::<Value>(&recorded.body).unwrap(),
        serde_json::json!({"filter": {"query": "datadog-agent"}})
    );
    server.shutdown().await;
}

#[test]
fn keys_are_sensitive_and_never_printed() {
    let client = DatadogEvents::new(reqwest::Client::new(), Site::Eu, credentials());
    assert_no_secret(&format!("{client:?}"));
    assert_no_secret(&format!("{:?}", credentials()));
    assert_no_secret(&format!("{:?}", client.api()));

    let prepared = client.list(range()).prepare().unwrap();
    let headers = prepared.request().headers();
    assert!(headers["dd-api-key"].is_sensitive());
    assert!(headers["dd-application-key"].is_sensitive());
    let shown = format!("{prepared:?}");
    assert_no_secret(&shown);
    assert!(shown.contains("dd-api-key: <redacted>"), "{shown}");
    assert!(shown.contains("dd-application-key: <redacted>"), "{shown}");
}

#[test]
fn keys_are_checked_before_any_request_exists() {
    let cases = [
        ("", APP_KEY, AuthError::MissingApiKey),
        (" \t", APP_KEY, AuthError::MissingApiKey),
        (API_KEY, "", AuthError::MissingApplicationKey),
        (API_KEY, "   ", AuthError::MissingApplicationKey),
        (
            API_KEY,
            "synthetic-app-key\n",
            AuthError::MalformedKey(Key::Application),
        ),
        (
            "synthetic\u{7f}key",
            APP_KEY,
            AuthError::MalformedKey(Key::Api),
        ),
    ];
    for (api, app, expected) in cases {
        let error = Credentials::new(api, app).unwrap_err();
        assert_eq!(error, expected);
        assert!(error.is_local());
        let error = Error::from(error);
        assert!(error.is_authentication());
        assert_eq!(error.status(), None);
        assert_error_hides_keys(&error);
        assert!(!error.to_string().contains("synthetic"), "{error}");
    }
}

#[test]
fn inputs_are_checked() {
    assert_eq!(
        TimeRange::from_millis(2, 1).unwrap_err(),
        InputError::InvertedRange
    );
    let point = TimeRange::from_millis(5, 5).unwrap();
    assert_eq!((point.from_ms(), point.to_ms()), (5, 5));
    assert_eq!(PageLimit::new(0).unwrap_err(), InputError::PageLimit);
    assert_eq!(PageLimit::new(1 << 31).unwrap_err(), InputError::PageLimit);
    assert_eq!(
        PageLimit::new(i32::MAX as u32).unwrap().get(),
        2_147_483_647
    );
}

/// Every status the cited pages document (400, 403, 429), with their documented error body.
#[tokio::test]
async fn documented_error_statuses_are_classified() {
    for status in [400u16, 403, 429] {
        let server = TestServer::start(move |_| Reply::json(status, ERROR_DOC)).await;
        for search in [false, true] {
            let client = local(&server);
            let error = if search {
                client
                    .search(SearchRequest::new())
                    .send()
                    .await
                    .unwrap_err()
            } else {
                client.list(range()).send().await.unwrap_err()
            };
            assert_eq!(error.status().map(|s| s.as_u16()), Some(status));
            let provider = error.provider().unwrap();
            assert_eq!(provider.messages(), Some(&["Bad Request".to_owned()][..]));
            match (status, &error) {
                (400, Error::BadRequest(_)) | (429, Error::RateLimited(_)) => {}
                (403, Error::Authentication(AuthError::Rejected(rejected))) => {
                    // The stand-in is not a listed API host: the error names the host.
                    assert_eq!(rejected.site(), None);
                    assert_eq!(rejected.host(), "127.0.0.1");
                    assert_eq!(rejected.provider().status().as_u16(), 403);
                    assert!(
                        error.to_string().contains("Datadog API host 127.0.0.1"),
                        "{error}"
                    );
                }
                _ => panic!("{status} classified as {error:?}"),
            }
            assert!(
                error.to_string().contains("1 provider message(s)"),
                "{error}"
            );
            assert!(!error.to_string().contains("Bad Request\""), "{error}");
            assert_error_hides_keys(&error);
        }
        server.shutdown().await;
    }
}

/// Statuses the cited pages do not list: 401 is authentication by HTTP semantics; the rest are
/// provider errors that keep their status and head.
#[tokio::test]
async fn undocumented_statuses_keep_their_class() {
    for (status, auth) in [(401u16, true), (404, false), (500, false), (503, false)] {
        let server = TestServer::start(move |_| Reply::json(status, ERROR_DOC)).await;
        let error = local(&server).list(range()).send().await.unwrap_err();
        assert_eq!(error.status().map(|s| s.as_u16()), Some(status));
        assert_eq!(error.is_authentication(), auth, "{error:?}");
        if !auth {
            assert!(matches!(error, Error::Provider(_)), "{error:?}");
        }
        assert_eq!(
            error.operation().map(|op| op.name()),
            Some("DatadogApi.events.list")
        );
        assert_error_hides_keys(&error);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn a_redirect_is_not_followed_and_the_keys_stay_home() {
    let elsewhere = TestServer::start(|_| Reply::json(200, EVENTS_200)).await;
    let location = format!("{}/api/v2/events", elsewhere.url());
    let server =
        TestServer::start(move |_| Reply::new(302).header("location", location.clone())).await;
    // The SDK's own transport.
    let client = DatadogEvents::new(
        DatadogEvents::http_client().unwrap(),
        Destination::at(Site::Us1, server.url()).unwrap(),
        credentials(),
    );
    let error = client.list(range()).send().await.unwrap_err();
    assert!(matches!(error, Error::Provider(_)), "{error:?}");
    assert_eq!(error.status().map(|s| s.as_u16()), Some(302));
    assert!(elsewhere.requests().is_empty());
    server.shutdown().await;
    elsewhere.shutdown().await;
}

#[tokio::test]
async fn error_documents_that_cannot_be_read_keep_the_status() {
    let oversized = format!("{{\"errors\": [\"{}\"]}}", "x".repeat(ERROR_BODY_LIMIT + 1));
    let cases: Vec<(u16, Reply, &str)> = vec![
        (
            503,
            Reply::text(503, "text/html", "<html>Service Unavailable</html>"),
            "metadata",
        ),
        (400, Reply::new(400).body(ERROR_DOC), "metadata"),
        (400, Reply::json(400, "{\"errors\": [\"Bad"), "decode"),
        (
            400,
            Reply::json(400, "{\"message\": \"no errors field\"}"),
            "decode",
        ),
        (429, Reply::json(429, oversized), "read"),
        (
            403,
            Reply::new(403)
                .header("content-type", "application/json")
                .with_body(Body::FailAfter(vec![b"{\"errors\": [".to_vec().into()])),
            "read",
        ),
    ];
    for (status, reply, stage) in cases {
        let reply = std::sync::Mutex::new(Some(reply));
        let server = TestServer::start(move |_| reply.lock().unwrap().take().unwrap()).await;
        let error = local(&server).list(range()).send().await.unwrap_err();
        assert_eq!(
            error.status().map(|s| s.as_u16()),
            Some(status),
            "{error:?}"
        );
        let provider = error.provider().unwrap();
        assert_eq!(provider.messages(), None);
        let failure = provider.document().unwrap_err();
        match (stage, failure) {
            ("metadata", ErrorBodyError::Metadata(e)) => assert_eq!(e.kind(), ErrorKind::Metadata),
            ("decode", ErrorBodyError::Decode(e)) => assert_eq!(e.kind(), ErrorKind::Decode),
            ("read", ErrorBodyError::Read(e)) => {
                assert!(matches!(e.kind(), ErrorKind::Limit | ErrorKind::Transport))
            }
            _ => panic!("{stage}: {failure:?}"),
        }
        assert_eq!(status == 403, error.is_authentication());
        assert_error_hides_keys(&error);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn provider_text_echoing_a_key_is_not_printed() {
    let echo = format!("{{\"errors\": [\"invalid key {API_KEY} / {APP_KEY}\"]}}");
    let server = TestServer::start(move |_| Reply::json(403, echo.clone())).await;
    let error = local(&server).list(range()).send().await.unwrap_err();
    assert!(error.is_authentication());
    assert_error_hides_keys(&error);
    // Reading the provider's text is a deliberate act.
    let messages = error.provider().unwrap().messages().unwrap();
    assert!(messages[0].contains(API_KEY));
    server.shutdown().await;
}

/// 2xx bodies that cannot become a page: each is Concord's error, with the head kept.
#[tokio::test]
async fn unusable_success_bodies() {
    let cases: Vec<(Reply, ErrorKind)> = vec![
        (
            Reply::text(200, "text/html", EVENTS_200),
            ErrorKind::Metadata,
        ),
        (Reply::new(200).body(EVENTS_200), ErrorKind::Metadata),
        (Reply::json(200, "{\"data\": ["), ErrorKind::Decode),
        (
            Reply::json(200, "{\"data\": \"not a list\"}"),
            ErrorKind::Decode,
        ),
        (Reply::json(200, ""), ErrorKind::Decode),
        (
            Reply::json(200, vec![b' '; RESPONSE_LIMIT + 1]),
            ErrorKind::Limit,
        ),
        (
            Reply::new(200)
                .header("content-type", "application/json")
                .with_body(Body::FailAfter(vec![
                    EVENTS_200.as_bytes()[..40].to_vec().into(),
                ])),
            ErrorKind::Transport,
        ),
    ];
    for (reply, kind) in cases {
        let reply = std::sync::Mutex::new(Some(reply));
        let server = TestServer::start(move |_| reply.lock().unwrap().take().unwrap()).await;
        let error = local(&server).list(range()).send().await.unwrap_err();
        let Error::Concord { source, meta } = &error else {
            panic!("{error:?}");
        };
        assert_eq!(source.kind(), kind, "{error:?}");
        assert_eq!(meta.as_ref().map(|m| m.status().as_u16()), Some(200));
        assert_eq!(error.status().map(|s| s.as_u16()), Some(200));
        assert_error_hides_keys(&error);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn no_answer_is_a_transport_error_without_a_head() {
    let server = TestServer::start(|_| Reply::hang_up()).await;
    let error = local(&server).list(range()).send().await.unwrap_err();
    let Error::Concord { source, meta } = &error else {
        panic!("{error:?}");
    };
    assert_eq!(source.kind(), ErrorKind::Transport);
    assert!(meta.is_none());
    assert_eq!(error.status(), None);
    assert_error_hides_keys(&error);
    server.shutdown().await;
}

#[test]
fn the_documented_page_decodes_verbatim() {
    // The documented example writes placeholders ("integer") into integer fields of the
    // event-specific attributes; those stay raw JSON, so the page still decodes.
    let page: concord_sdk_datadog_events::EventsPage = serde_json::from_str(EVENTS_200).unwrap();
    let event = page
        .event("AAAAAWgN8Xwgr1vKDQAAAABBV2dOOFh3ZzZobm1mWXJFYTR0OA")
        .unwrap();
    assert_eq!(event.kind.as_deref(), Some("event"));
    assert_eq!(event.message(), Some("string"));
    assert_eq!(event.tags(), ["team:A"]);
    assert_eq!(event.timestamp(), Some("2019-01-02T09:42:36.320Z"));
    assert_eq!(event.attribute("title"), Some(&Value::from("Oh boy!")));
    assert_eq!(
        event.attribute("date_happened"),
        Some(&Value::from("integer"))
    );
    assert_eq!(
        page.after(),
        Some(
            "eyJzdGFydEF0IjoiQVFBQUFYS2tMS3pPbm40NGV3QUFBQUJCV0V0clRFdDZVbG8zY3pCRmNsbHJiVmxDWlEifQ=="
        )
    );
    assert_eq!(page.warnings().len(), 1);
    assert_eq!(page.warnings()[0].code.as_deref(), Some("unknown_index"));
    let meta = page.meta.as_ref().unwrap();
    assert_eq!(meta.status.as_deref(), Some("done"));
    assert_eq!(meta.elapsed, Some(132));
    assert!(
        page.links
            .as_ref()
            .and_then(|l| l.next.as_deref())
            .unwrap()
            .starts_with("https://app.datadoghq.com/api/v2/events?")
    );
    assert!(page.event("no-such-id").is_none());
}

#[tokio::test]
async fn an_empty_cursor_ends_the_traversal_as_a_protocol_error() {
    let server = TestServer::start(|_| Reply::json(200, EMPTY_CURSOR)).await;
    let mut pages = local(&server).search_pages(SearchRequest::new());
    let error = pages.next_page().await.unwrap_err();
    assert!(
        matches!(
            error.provider_error(),
            Some(Error::Protocol(ProtocolError::EmptyCursor))
        ),
        "{error:?}"
    );
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

#[tokio::test]
async fn a_repeated_cursor_ends_the_traversal_before_a_loop() {
    // Every page names `synthetic-cursor-1` as the next one.
    let server = TestServer::start(|_| Reply::json(200, PAGE_1)).await;
    let mut pages = local(&server).search_pages(SearchRequest::new());
    assert_eq!(pages.next_page().await.unwrap().unwrap().items.len(), 2);
    assert_eq!(pages.next_page().await.unwrap().unwrap().items.len(), 2);
    let error = pages.next_page().await.unwrap_err();
    assert!(
        matches!(
            error.provider_error(),
            Some(Error::Protocol(ProtocolError::RepeatedCursor))
        ),
        "{error:?}"
    );
    // The third request was never sent.
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

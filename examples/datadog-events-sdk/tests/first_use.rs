//! Fresh-consumer caller tasks, derived only from public documentation.
//! Live Datadog calls: NOT_RUN. Keys and responses are synthetic local fixtures.

use std::error::Error as StdError;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};

use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_datadog_events::{
    AuthError, Credentials, DatadogEvents, Destination, Error, PageLimit, ProtocolError,
    SearchRequest, Site, Sort, TimeRange, concord::ErrorKind, pagination::PagesError,
};
use serde_json::{Value, json};

const API_KEY: &str = "fixture-eu-api-2bf407c19a063ed8";
const APP_KEY: &str = "fixture-eu-app-7468bc12a09df305";
const FIRST_ID: &str = "AAAAAWgN8Xwgr1vKDQAAAABB-checkout-deploy";
const SECOND_ID: &str = "AAAAAWgN8Xwgr1vKDQAAAABB-agent-restart";
const AFTER: &str = "eyJzdGFydEF0IjoiY2hlY2tvdXQifQ==+/";
const AFTER_EMPTY: &str = "eyJzdGFydEF0IjoiYWdlbnQifQ==/+";

fn range() -> TimeRange {
    TimeRange::from_millis(1_767_225_600_000, 1_767_229_200_000).unwrap()
}

fn client(server: &TestServer) -> DatadogEvents {
    DatadogEvents::new(
        DatadogEvents::http_client().unwrap(),
        Destination::at(Site::Eu, server.url()).unwrap(),
        Credentials::new(API_KEY, APP_KEY).unwrap(),
    )
}

fn page(ids: &[&str], after: Option<&str>, warning: bool) -> Value {
    let data: Vec<_> = ids
        .iter()
        .map(|id| {
            json!({
                "id": id,
                "type": "event",
                "attributes": {
                    "message": "checkout rollout completed on checkout-eu-03",
                    "tags": ["service:checkout", "env:prod", "host:checkout-eu-03"],
                    "timestamp": "2026-01-01T00:42:36.320Z",
                    "attributes": {"title": "Checkout deployment v2026.01.01"}
                }
            })
        })
        .collect();
    let mut body = json!({
        "data": data,
        "meta": {
            "elapsed": 132,
            "request_id": "fixture-request-eu-20260101",
            "status": "done",
            "warnings": []
        }
    });
    if let Some(after) = after {
        body["meta"]["page"] = json!({"after": after});
        // A trap: the SDK must use the cursor on the local API, never this URL.
        body["links"] = json!({"next": "https://app.datadoghq.com/do-not-request"});
    }
    if warning {
        body["meta"]["warnings"] = json!([{
            "code": "unknown_index",
            "detail": "indexes: retired-checkout",
            "title": "One index is missing; other indexes returned results."
        }]);
    }
    body
}

fn assert_common_wire(request: &Recorded, server: &TestServer) {
    assert_eq!(request.host, server.addr().to_string());
    assert_eq!(request.header_all("DD-API-KEY"), [API_KEY]);
    assert_eq!(request.header_all("DD-APPLICATION-KEY"), [APP_KEY]);
    assert_eq!(request.header("accept"), Some("application/json"));
    assert!(request.header("authorization").is_none());
    assert_redacted(&request.target);
}

fn assert_redacted(text: &str) {
    assert!(
        !text.contains(API_KEY),
        "API key appeared in diagnostic text"
    );
    assert!(
        !text.contains(APP_KEY),
        "application key appeared in diagnostic text"
    );
}

fn assert_error_redacted(error: &Error) {
    assert_redacted(&format!("{error} {error:?}"));
    let mut source = error.source();
    while let Some(error) = source {
        assert_redacted(&format!("{error} {error:?}"));
        source = error.source();
    }
    if let Some(provider) = error.provider() {
        assert_redacted(&format!("{provider} {provider:?}"));
    }
}

#[tokio::test]
async fn caller_lists_bounded_events_and_inspects_one() {
    let server = TestServer::start(|request: &Recorded| -> Reply {
        let continued = !request.query_all("page[cursor]").is_empty();
        let body = if continued {
            page(&[SECOND_ID], None, false)
        } else {
            page(&[FIRST_ID], Some(AFTER), true)
        };
        Reply::json(200, body.to_string())
    })
    .await;
    let events = client(&server);
    assert_eq!(events.site(), Site::Eu);

    // Prepare only to inspect the production host and sensitive flags; never send it.
    let credentials = Credentials::new(API_KEY, APP_KEY).unwrap();
    assert_redacted(&format!("{credentials:?}"));
    let production = DatadogEvents::new(
        DatadogEvents::http_client().unwrap(),
        Site::from_dd_site("datadoghq.eu").unwrap(),
        credentials,
    );
    let prepared = production.list(range()).prepare().unwrap();
    assert_eq!(
        prepared.request().url().as_str(),
        "https://api.datadoghq.eu/api/v2/events?filter%5Bfrom%5D=1767225600000&filter%5Bto%5D=1767229200000"
    );
    for name in ["DD-API-KEY", "DD-APPLICATION-KEY"] {
        assert!(
            prepared
                .request()
                .headers()
                .get(name)
                .unwrap()
                .is_sensitive()
        );
    }
    assert_redacted(&format!(
        "{production:?} {prepared:?} {:?}",
        prepared.request()
    ));
    drop(prepared);
    drop(production);

    // The caller path from the README: list once, then inspect the returned page locally.
    let first = events
        .list(range())
        .set_query("service:checkout env:prod")
        .set_sort(Sort::Descending)
        .set_limit(PageLimit::new(2).unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(first.data.len(), 1);
    let id = first.data[0].id.as_deref().unwrap();
    let event = first.event(id).unwrap();
    assert_eq!(id, FIRST_ID);
    assert_eq!(event.kind.as_deref(), Some("event"));
    assert_eq!(
        event.message(),
        Some("checkout rollout completed on checkout-eu-03")
    );
    assert_eq!(event.timestamp(), Some("2026-01-01T00:42:36.320Z"));
    assert_eq!(
        event.tags(),
        ["service:checkout", "env:prod", "host:checkout-eu-03"]
    );
    assert_eq!(
        event.attribute("title"),
        Some(&json!("Checkout deployment v2026.01.01"))
    );
    assert!(first.event("absent-id").is_none());
    assert_eq!(first.warnings()[0].code.as_deref(), Some("unknown_index"));
    let meta = first.meta.as_ref().unwrap();
    assert_eq!(
        meta.request_id.as_deref(),
        Some("fixture-request-eu-20260101")
    );
    assert_eq!(meta.elapsed, Some(132));
    assert_eq!(meta.status.as_deref(), Some("done"));
    assert_eq!(first.after(), Some(AFTER));
    assert_eq!(
        first.links.as_ref().unwrap().next.as_deref(),
        Some("https://app.datadoghq.com/do-not-request")
    );
    assert_eq!(server.requests().len(), 1, "inspection must be local");

    let last = events
        .list(range())
        .set_query("service:checkout env:prod")
        .set_sort(Sort::Descending)
        .set_limit(PageLimit::new(2).unwrap())
        .set_cursor(first.next_cursor().unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(last.data[0].id.as_deref(), Some(SECOND_ID));
    assert!(last.next_cursor().is_none());
    assert!(last.warnings().is_empty());
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    for (index, request) in requests.iter().enumerate() {
        assert_common_wire(request, &server);
        assert_eq!(request.method, "GET");
        assert_eq!(request.path(), "/api/v2/events");
        assert!(request.body.is_empty());
        let mut expected = vec![
            ("filter[from]".to_owned(), "1767225600000".to_owned()),
            ("filter[to]".to_owned(), "1767229200000".to_owned()),
            (
                "filter[query]".to_owned(),
                "service:checkout env:prod".to_owned(),
            ),
            ("sort".to_owned(), "-timestamp".to_owned()),
            ("page[limit]".to_owned(), "2".to_owned()),
        ];
        if index == 1 {
            expected.push(("page[cursor]".to_owned(), AFTER.to_owned()));
        }
        let mut actual = request.query_pairs();
        actual.sort();
        expected.sort();
        assert_eq!(actual, expected);
    }
    server.shutdown().await;

    // Missing application key is a local authentication failure, with no HTTP request.
    let unused = TestServer::start(|_: &Recorded| -> Reply { Reply::new(500) }).await;
    let missing: Error = Credentials::new(API_KEY, "").unwrap_err().into();
    assert!(matches!(
        missing,
        Error::Authentication(AuthError::MissingApplicationKey)
    ));
    assert!(missing.is_authentication());
    assert!(missing.status().is_none());
    assert!(missing.meta().is_none());
    assert_error_redacted(&missing);
    assert!(unused.requests().is_empty());
    unused.shutdown().await;

    // Simulated region/key rejection: echo both synthetic keys in untrusted provider text.
    for status in [401, 403] {
        let rejected = TestServer::start(move |_: &Recorded| -> Reply {
            Reply::json(
                status,
                json!({"errors": [format!("keys rejected: {API_KEY} {APP_KEY}")]}).to_string(),
            )
            .header("x-reflected-credentials", format!("{API_KEY} {APP_KEY}"))
        })
        .await;
        // Fixture convention: these synthetic keys belong to EU, while this client
        // selects US1. Inspect that production host without contacting it, then route
        // a new client with the same region and keys to the local authentication stand-in.
        let wrong_region = DatadogEvents::new(
            DatadogEvents::http_client().unwrap(),
            Site::from_dd_site("datadoghq.com").unwrap(),
            Credentials::new(API_KEY, APP_KEY).unwrap(),
        );
        let prepared = wrong_region.list(range()).prepare().unwrap();
        assert_eq!(
            prepared.request().url().host_str(),
            Some("api.datadoghq.com")
        );
        drop(prepared);
        let wrong_region = DatadogEvents::new(
            DatadogEvents::http_client().unwrap(),
            Destination::at(wrong_region.site(), rejected.url()).unwrap(),
            Credentials::new(API_KEY, APP_KEY).unwrap(),
        );
        assert_eq!(
            wrong_region.site(),
            Site::from_dd_site("datadoghq.com").unwrap()
        );
        let error = wrong_region.list(range()).send().await.unwrap_err();
        assert_eq!(error.status().unwrap().as_u16(), status);
        assert!(error.is_authentication());
        let Error::Authentication(AuthError::Rejected(ref rejection)) = error else {
            panic!("expected authentication rejection");
        };
        assert_eq!(rejection.host(), "127.0.0.1");
        assert_eq!(rejection.site(), None);
        assert!(error.to_string().contains("127.0.0.1"));
        assert!(error.to_string().to_lowercase().contains("site"));
        assert!(error.provider().unwrap().messages().unwrap()[0].contains(API_KEY));
        assert_error_redacted(&error);
        assert_common_wire(&rejected.single(), &rejected);
        rejected.shutdown().await;
    }

    // Status classification survives malformed error documents; no implicit retries.
    for status in [400, 429, 503] {
        let failed = TestServer::start(move |_: &Recorded| -> Reply {
            Reply::json(status, "{truncated").header("retry-after", "3")
        })
        .await;
        let error = client(&failed).list(range()).send().await.unwrap_err();
        assert!(match status {
            400 => matches!(error, Error::BadRequest(_)),
            429 => matches!(error, Error::RateLimited(_)),
            _ => matches!(error, Error::Provider(_)),
        });
        assert_eq!(error.status().unwrap().as_u16(), status);
        assert_eq!(
            error.meta().unwrap().headers().get("retry-after").unwrap(),
            "3"
        );
        assert!(error.provider().unwrap().document().is_err());
        assert!(error.provider().unwrap().messages().is_none());
        assert!(error.operation().is_some());
        assert_error_redacted(&error);
        assert_eq!(failed.requests().len(), 1);
        failed.shutdown().await;
    }

    for (content_type, body, kind) in [
        ("application/json", "{truncated", ErrorKind::Decode),
        ("text/plain", "not JSON", ErrorKind::Metadata),
    ] {
        let malformed = TestServer::start(move |_: &Recorded| -> Reply {
            Reply::text(200, content_type, body).header("x-request-id", "bad-body-01")
        })
        .await;
        let error = client(&malformed).list(range()).send().await.unwrap_err();
        let Error::Concord { ref source, .. } = error else {
            panic!("expected success-body error");
        };
        assert_eq!(source.kind(), kind);
        assert_eq!(error.status().unwrap().as_u16(), 200);
        assert_eq!(
            error.meta().unwrap().headers().get("x-request-id").unwrap(),
            "bad-body-01"
        );
        assert_error_redacted(&error);
        assert_eq!(malformed.requests().len(), 1);
        malformed.shutdown().await;
    }
}

fn search_request() -> SearchRequest {
    SearchRequest::new()
        .query("source:datadog-agent service:checkout")
        .from("2026-01-01T00:00:00Z")
        .to("2026-01-01T01:00:00Z")
        .timezone("GMT")
        .sort(Sort::Ascending)
        .limit(PageLimit::new(2).unwrap())
}

fn search_body(cursor: Option<&str>) -> Value {
    let mut expected = json!({
        "filter": {
            "query": "source:datadog-agent service:checkout",
            "from": "2026-01-01T00:00:00Z",
            "to": "2026-01-01T01:00:00Z"
        },
        "options": {"timezone": "GMT"},
        "sort": "timestamp",
        "page": {"limit": 2}
    });
    if let Some(cursor) = cursor {
        expected["page"]["cursor"] = json!(cursor);
    }
    expected
}

fn request_cursor(request: &Recorded) -> Option<String> {
    let body: Value = serde_json::from_slice(&request.body).unwrap();
    body["page"]["cursor"].as_str().map(str::to_owned)
}

fn assert_search_wire(request: &Recorded, server: &TestServer, cursor: Option<&str>) {
    assert_common_wire(request, server);
    assert_eq!(request.method, "POST");
    assert_eq!(request.target, "/api/v2/events/search");
    assert!(request.query().is_none());
    assert_eq!(request.header("content-type"), Some("application/json"));
    assert_eq!(
        serde_json::from_slice::<Value>(&request.body).unwrap(),
        search_body(cursor)
    );
    assert_redacted(&request.body_text());
}

#[tokio::test]
async fn caller_searches_pages_and_handles_incomplete_results() {
    let server = TestServer::start(|request: &Recorded| -> Reply {
        let (mut body, request_id) = match request_cursor(request).as_deref() {
            None => (page(&[FIRST_ID], Some(AFTER), true), "search-01"),
            Some(AFTER) => (page(&[], Some(AFTER_EMPTY), false), "search-02-empty"),
            Some(AFTER_EMPTY) => (page(&[SECOND_ID], None, false), "search-03"),
            _ => return Reply::json(400, r#"{"errors":["unexpected cursor"]}"#),
        };
        body["meta"]["request_id"] = json!(request_id);
        Reply::json(200, body.to_string())
    })
    .await;
    let events = client(&server);
    let mut pages = events
        .search_pages(search_request())
        .max_pages(NonZeroUsize::new(3).unwrap());
    assert!(server.requests().is_empty(), "traversal must be lazy");
    let mut returned = Vec::new();
    while let Some(page) = pages.next_page().await.unwrap() {
        returned.push(page);
    }
    assert_eq!(returned.len(), 3);
    assert_eq!(returned[0].items[0].id.as_deref(), Some(FIRST_ID));
    assert_eq!(returned[0].meta.request_id.as_deref(), Some("search-01"));
    assert_eq!(returned[0].meta.status.as_deref(), Some("done"));
    assert_eq!(
        returned[0].meta.warnings[0].code.as_deref(),
        Some("unknown_index")
    );
    assert_eq!(returned[0].next.as_ref().unwrap().as_str(), AFTER);
    assert!(
        returned[1].items.is_empty(),
        "empty page is not necessarily the end"
    );
    assert_eq!(
        returned[1].meta.request_id.as_deref(),
        Some("search-02-empty")
    );
    assert_eq!(returned[1].next.as_ref().unwrap().as_str(), AFTER_EMPTY);
    assert_eq!(returned[2].items[0].id.as_deref(), Some(SECOND_ID));
    assert_eq!(returned[2].meta.request_id.as_deref(), Some("search-03"));
    assert!(returned[2].meta.warnings.is_empty());
    assert!(returned[2].next.is_none());
    assert!(pages.checkpoint().is_none());
    assert!(pages.next_page().await.unwrap().is_none());
    assert_eq!(pages.progress().admitted, 3);
    assert_eq!(pages.progress().succeeded, 3);
    assert_eq!(pages.progress().failed, 0);
    assert_eq!(
        server.requests().len(),
        3,
        "no request after terminal page or to links.next"
    );
    // The cursor chain is complete, but warning-bearing provider data may still be partial.
    assert!(!returned[0].meta.warnings.is_empty());

    // A budget smaller than the cursor chain is an explicit, resumable incomplete result.
    let mut capped = events
        .search_pages(search_request())
        .max_pages(NonZeroUsize::new(1).unwrap());
    let prefix = capped.next_page().await.unwrap().unwrap();
    let bounded = capped.next_page().await.unwrap_err();
    assert!(matches!(bounded, PagesError::Bounded { .. }));
    assert_eq!(bounded.progress().admitted, 1);
    assert_eq!(bounded.progress().succeeded, 1);
    let resume = bounded.resume().unwrap().clone();
    assert_eq!(resume.key().unwrap().as_str(), AFTER);
    assert_eq!(capped.checkpoint(), Some(&resume));
    assert!(matches!(
        capped.next_page().await,
        Err(PagesError::Ended { .. })
    ));
    assert_eq!(
        server.requests().len(),
        4,
        "cap must not fetch its next page"
    );
    let resumed = events
        .search_pages_from(search_request(), resume)
        .max_pages(NonZeroUsize::new(2).unwrap())
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(resumed.len(), 2);
    assert!(resumed[0].items.is_empty());
    let ids: Vec<_> = prefix
        .items
        .iter()
        .chain(resumed.iter().flat_map(|p| p.items.iter()))
        .map(|event| event.id.as_deref().unwrap())
        .collect();
    assert_eq!(ids, [FIRST_ID, SECOND_ID]);
    let requests = server.requests();
    assert_eq!(requests.len(), 6);
    for (request, cursor) in requests.iter().zip([
        None,
        Some(AFTER),
        Some(AFTER_EMPTY),
        None,
        Some(AFTER),
        Some(AFTER_EMPTY),
    ]) {
        assert_search_wire(request, &server, cursor);
    }
    server.shutdown().await;

    // Collecting stops on page 2's 429, preserves page 1 and the failed cursor, and never
    // retries automatically. An explicit new traversal can retry that checkpoint later.
    let attempts = AtomicUsize::new(0);
    let limited = TestServer::start(move |request: &Recorded| -> Reply {
        match request_cursor(request).as_deref() {
            None => Reply::json(200, page(&[FIRST_ID], Some(AFTER), false).to_string()),
            Some(AFTER) if attempts.fetch_add(1, Ordering::SeqCst) == 0 => Reply::json(
                429,
                json!({"errors": [format!("quota for {API_KEY} {APP_KEY}")]}).to_string(),
            )
            .header("retry-after", "3"),
            Some(AFTER) => Reply::json(200, page(&[SECOND_ID], None, false).to_string()),
            _ => Reply::new(500),
        }
    })
    .await;
    let events = client(&limited);
    let incomplete = events
        .search_pages(search_request())
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 1);
    assert_eq!(incomplete.collected[0].id.as_deref(), Some(FIRST_ID));
    assert!(matches!(incomplete.error, PagesError::Provider { .. }));
    let error = incomplete.error.provider_error().unwrap();
    assert!(matches!(error, Error::RateLimited(_)));
    assert_eq!(error.status().unwrap().as_u16(), 429);
    assert_eq!(
        error.meta().unwrap().headers().get("retry-after").unwrap(),
        "3"
    );
    assert!(error.provider().unwrap().messages().unwrap()[0].contains(APP_KEY));
    assert_error_redacted(error);
    assert_redacted(&format!("{:?} {}", incomplete, incomplete.error));
    assert_eq!(incomplete.error.progress().admitted, 2);
    assert_eq!(incomplete.error.progress().succeeded, 1);
    assert_eq!(incomplete.error.progress().failed, 1);
    let resume = incomplete.error.resume().unwrap().clone();
    assert_eq!(resume.key().unwrap().as_str(), AFTER);
    assert_eq!(limited.requests().len(), 2, "429 must not trigger a retry");
    let recovered = events
        .search_pages_from(search_request(), resume)
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].id.as_deref(), Some(SECOND_ID));
    for (request, cursor) in limited
        .requests()
        .iter()
        .zip([None, Some(AFTER), Some(AFTER)])
    {
        assert_search_wire(request, &limited, cursor);
    }
    assert_eq!(limited.requests().len(), 3);
    limited.shutdown().await;

    // Bad continuations and a malformed later response must not silently finish or loop.
    for failure in ["empty", "repeated", "decode"] {
        let broken = TestServer::start(move |request: &Recorded| -> Reply {
            if request_cursor(request).is_none() {
                return Reply::json(200, page(&[FIRST_ID], Some(AFTER), false).to_string());
            }
            match failure {
                "empty" => Reply::json(200, page(&[SECOND_ID], Some(""), false).to_string()),
                "repeated" => Reply::json(200, page(&[SECOND_ID], Some(AFTER), false).to_string()),
                _ => Reply::json(200, "{truncated"),
            }
        })
        .await;
        let events = client(&broken);
        let mut pages = events.search_pages(search_request());
        let first = pages.next_page().await.unwrap().unwrap();
        let result = pages.next_page().await;
        let failed = if failure == "repeated" {
            // Observed timing: a repeated cursor is refused when next requested, after
            // yielding the page that advertised it, but before issuing another HTTP call.
            let repeated = result.unwrap().unwrap();
            assert_eq!(repeated.items[0].id.as_deref(), Some(SECOND_ID));
            assert_eq!(repeated.next.as_ref().unwrap().as_str(), AFTER);
            assert_eq!(broken.requests().len(), 2);
            pages.next_page().await.unwrap_err()
        } else {
            assert!(result.is_err(), "expected {failure} error, got {result:?}");
            result.unwrap_err()
        };
        let error = failed.provider_error().unwrap();
        match failure {
            "empty" => assert!(matches!(error, Error::Protocol(ProtocolError::EmptyCursor))),
            "repeated" => assert!(matches!(
                error,
                Error::Protocol(ProtocolError::RepeatedCursor)
            )),
            _ => {
                let Error::Concord { source, .. } = error else {
                    panic!("expected decode error");
                };
                assert_eq!(source.kind(), ErrorKind::Decode);
                assert_eq!(error.status().unwrap().as_u16(), 200);
                assert!(error.meta().is_some());
            }
        }
        assert_eq!(first.items[0].id.as_deref(), Some(FIRST_ID));
        assert_eq!(failed.resume().unwrap().key().unwrap().as_str(), AFTER);
        assert!(matches!(
            pages.next_page().await,
            Err(PagesError::Ended { .. })
        ));
        assert_eq!(broken.requests().len(), 2);
        for (request, cursor) in broken.requests().iter().zip([None, Some(AFTER)]) {
            assert_search_wire(request, &broken, cursor);
        }
        broken.shutdown().await;
    }
}

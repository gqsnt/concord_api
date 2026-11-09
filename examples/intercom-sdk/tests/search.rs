//! Exact-email search wire and failure regressions, using only loopback fixtures.

use std::{
    num::NonZeroUsize,
    time::{Duration, SystemTime},
};

use concord_example_testserver::{Recorded, Reply, TestServer, paused};
use concord_sdk_intercom::{
    EmailSearch, Error, ErrorCode, HeaderNumber, Intercom, PageSize, http_client,
    pagination::{PagesError, Resume},
};
use serde_json::{Value, json};

const EMAIL: &str = "Exact+tag&value@example.test";
const CURSOR: &str = "cursor+/=?&fixture";
const LIST_FIRST: &str = include_str!("../fixtures/list-first.json");
const LIST_LAST: &str = include_str!("../fixtures/list-last.json");

fn api(server: &TestServer) -> Intercom {
    Intercom::with_token(http_client().unwrap(), "SYNTHETIC_SECRET")
        .unwrap()
        .with_base_url(server.url())
        .unwrap()
}

fn cap(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}

fn assert_search(request: &Recorded, size: u16, cursor: Option<&str>) {
    assert_eq!(request.method, "POST");
    assert_eq!(request.target, "/contacts/search");
    assert_eq!(request.query(), None);
    assert_eq!(
        request.header("authorization"),
        Some("Bearer SYNTHETIC_SECRET")
    );
    assert_eq!(request.header("intercom-version"), Some("2.17"));
    assert_eq!(request.header("accept"), Some("application/json"));
    assert_eq!(request.header("content-type"), Some("application/json"));
    let mut expected = json!({
        "query": {"field": "email", "operator": "=", "value": EMAIL},
        "pagination": {"per_page": size}
    });
    if let Some(cursor) = cursor {
        expected["pagination"]["starting_after"] = json!(cursor);
    }
    assert_eq!(
        serde_json::from_slice::<Value>(&request.body).unwrap(),
        expected
    );
}

#[tokio::test]
async fn search_body_uses_equality_and_nested_pagination_without_url_parameters() {
    let server = TestServer::start(|_: &Recorded| -> Reply { Reply::json(200, LIST_LAST) }).await;
    let api = api(&server);
    let input = EmailSearch::new(EMAIL);
    assert!(!format!("{input:?}").contains(EMAIL));
    let op = api.contacts().search(input);
    assert!(!format!("{op:?} {:?}", op.clone().prepare().unwrap()).contains(EMAIL));
    let response = op.send().await.unwrap();
    assert_eq!(response.value.data.len(), 1);
    assert_eq!(response.meta.status().as_u16(), 200);
    api.contacts()
        .search(
            EmailSearch::new(EMAIL)
                .with_per_page(PageSize::new(150).unwrap())
                .with_starting_after(Some(CURSOR.into())),
        )
        .send()
        .await
        .unwrap();
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_search(&requests[0], 50, None);
    assert_search(&requests[1], 150, Some(CURSOR));
    server.shutdown().await;
}

#[tokio::test]
async fn headerless_search_429_keeps_prefix_and_failed_cursor_without_retry_or_reset() {
    let server = TestServer::start(|request: &Recorded| -> Reply {
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        if body["pagination"].get("starting_after").is_none() {
            Reply::json(200, LIST_FIRST)
        } else {
            Reply::json(429, include_str!("../fixtures/rate-error.json"))
        }
    })
    .await;
    // Clock frozen while the call runs: the bound fails only a call that waits for something
    // that never comes (then reported as stuck).
    let incomplete = tokio::time::timeout(
        Duration::from_millis(500),
        paused::frozen(
            api(&server)
                .contacts()
                .search(EmailSearch::new(EMAIL).with_per_page(PageSize::new(1).unwrap()))
                .pages(Resume::Start, cap(20))
                .items()
                .collect_complete(),
        ),
    )
    .await
    .expect("search must stop without waiting or looping")
    .unwrap_err();
    assert_eq!(incomplete.collected.len(), 1);
    assert_eq!(incomplete.collected[0].id, "111111111111111111111111");
    assert_eq!(incomplete.error.resume(), Some(&Resume::At(CURSOR.into())));
    let Error::Provider(provider) = incomplete.error.provider_error().unwrap() else {
        panic!("expected a provider failure")
    };
    assert_eq!(provider.meta.status().as_u16(), 429);
    assert_eq!(provider.rate_limits().limit, HeaderNumber::Missing);
    assert_eq!(provider.rate_limits().remaining, HeaderNumber::Missing);
    assert_eq!(
        provider.rate_limits().reset_unix_seconds,
        HeaderNumber::Missing
    );
    assert_eq!(provider.retry_after(SystemTime::UNIX_EPOCH).unwrap(), None);
    assert_eq!(
        provider.document.as_ref().unwrap().errors[0].code,
        ErrorCode::Text("rate_limit_exceeded".into())
    );
    let (_, failed, _) = incomplete.error.into_provider_error().unwrap();
    assert_eq!(failed, Resume::At(CURSOR.into()));
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_search(&requests[0], 1, None);
    assert_search(&requests[1], 1, Some(CURSOR));
    server.shutdown().await;
}

#[tokio::test]
async fn search_continues_empty_pages_and_stops_on_null_pages() {
    let server = TestServer::start(|request: &Recorded| -> Reply {
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        if body["pagination"].get("starting_after").is_none() {
            Reply::json(200, include_str!("../fixtures/list-empty-next.json"))
        } else {
            Reply::json(200, include_str!("../fixtures/list-null-pages.json"))
        }
    })
    .await;
    let mut pages = api(&server)
        .contacts()
        .search(EmailSearch::new(EMAIL))
        .pages(Resume::Start, cap(2));
    assert!(pages.next_page().await.unwrap().unwrap().items.is_empty());
    assert!(pages.next_page().await.unwrap().unwrap().items.is_empty());
    assert!(pages.next_page().await.unwrap().is_none());
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_search(&requests[0], 50, None);
    assert_search(&requests[1], 50, Some("after-empty"));
    server.shutdown().await;
}

#[tokio::test]
async fn invalid_search_continuations_fail_and_never_fetch_an_extra_page() {
    for (body, from, invalid_cursor) in [
        (
            include_str!("../fixtures/list-empty-cursor.json"),
            Resume::Start,
            true,
        ),
        (LIST_FIRST, Resume::At(CURSOR.into()), true),
        (
            include_str!("../fixtures/list-missing-pages.json"),
            Resume::Start,
            false,
        ),
    ] {
        let server =
            TestServer::start(move |_: &Recorded| -> Reply { Reply::json(200, body) }).await;
        let mut pages = api(&server)
            .contacts()
            .search(EmailSearch::new(EMAIL))
            .pages(from.clone(), cap(10));
        let error = pages.next_page().await.unwrap_err();
        assert_eq!(error.resume(), Some(&from));
        let provider = error.provider_error().unwrap();
        if invalid_cursor {
            assert!(matches!(provider, Error::InvalidCursor(_)));
        } else {
            assert!(matches!(provider, Error::Concord(_)));
        }
        assert_eq!(provider.status().unwrap().as_u16(), 200);
        assert!(matches!(
            pages.next_page().await,
            Err(PagesError::Ended { .. })
        ));
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

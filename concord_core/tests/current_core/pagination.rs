use super::common::*;
use bytes::Bytes;
use concord_core::advanced::{
    AuthPlacement, PageAdvance, PageDecision, PageInit, PageRequest, PaginationController,
    ProgressKey,
};
use concord_core::internal::PaginationPlan;
use concord_core::prelude::{ApiClientError, CursorPagination};
use http::{HeaderValue, StatusCode};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Default)]
struct HeaderTokenPagination;

#[derive(Default)]
struct HeaderTokenState {
    token: u64,
}

impl PaginationController<Vec<String>> for HeaderTokenPagination {
    type State = HeaderTokenState;

    fn init(&self, _ctx: PageInit<'_>) -> Result<Self::State, ApiClientError> {
        Ok(HeaderTokenState::default())
    }

    fn apply(
        &self,
        state: &Self::State,
        request: &mut PageRequest<'_>,
    ) -> Result<(), ApiClientError> {
        request.set_query("cursor", state.token);
        request.set_header(
            "x-page-token",
            HeaderValue::from_str(&state.token.to_string()).unwrap(),
        )?;
        Ok(())
    }

    fn advance(
        &self,
        state: &mut Self::State,
        page: &Vec<String>,
        _ctx: PageAdvance<'_>,
    ) -> Result<PageDecision, ApiClientError> {
        if page.len() < 2 {
            return Ok(PageDecision::Stop);
        }
        state.token += 1;
        Ok(PageDecision::Continue)
    }

    fn progress_key(&self, state: &Self::State) -> Option<ProgressKey> {
        Some(ProgressKey::U64(state.token))
    }
}

#[derive(Default)]
struct InvalidHeaderPagination;

impl PaginationController<Vec<String>> for InvalidHeaderPagination {
    type State = ();

    fn init(&self, _ctx: PageInit<'_>) -> Result<Self::State, ApiClientError> {
        Ok(())
    }

    fn apply(
        &self,
        _state: &Self::State,
        request: &mut PageRequest<'_>,
    ) -> Result<(), ApiClientError> {
        request.set_header("bad header name", HeaderValue::from_static("value"))
    }

    fn advance(
        &self,
        _state: &mut Self::State,
        _page: &Vec<String>,
        _ctx: PageAdvance<'_>,
    ) -> Result<PageDecision, ApiClientError> {
        Ok(PageDecision::Stop)
    }

    fn progress_key(&self, _state: &Self::State) -> Option<ProgressKey> {
        None
    }
}

#[derive(Default)]
struct DynamicRequestMutationPagination;

struct DynamicRequestMutationState {
    query_key: String,
    header_name: String,
}

impl PaginationController<Vec<String>> for DynamicRequestMutationPagination {
    type State = DynamicRequestMutationState;

    fn init(&self, _ctx: PageInit<'_>) -> Result<Self::State, ApiClientError> {
        Ok(DynamicRequestMutationState {
            query_key: "dynamic_page".to_string(),
            header_name: "x-dynamic-page".to_string(),
        })
    }

    fn apply(
        &self,
        state: &Self::State,
        request: &mut PageRequest<'_>,
    ) -> Result<(), ApiClientError> {
        request.set_query(state.query_key.clone(), 7);
        request.set_header(
            state.header_name.as_str(),
            HeaderValue::from_static("dynamic-header-value"),
        )?;
        Ok(())
    }

    fn advance(
        &self,
        _state: &mut Self::State,
        _page: &Vec<String>,
        _ctx: PageAdvance<'_>,
    ) -> Result<PageDecision, ApiClientError> {
        Ok(PageDecision::Stop)
    }

    fn progress_key(&self, _state: &Self::State) -> Option<ProgressKey> {
        None
    }
}

#[derive(Default)]
struct StopAfterFirstNoHintPagination;

impl PaginationController<NoHintItems> for StopAfterFirstNoHintPagination {
    type State = ();

    fn init(&self, _ctx: PageInit<'_>) -> Result<Self::State, ApiClientError> {
        Ok(())
    }

    fn apply(
        &self,
        _state: &Self::State,
        _request: &mut PageRequest<'_>,
    ) -> Result<(), ApiClientError> {
        Ok(())
    }

    fn advance(
        &self,
        _state: &mut Self::State,
        _page: &NoHintItems,
        _ctx: PageAdvance<'_>,
    ) -> Result<PageDecision, ApiClientError> {
        Ok(PageDecision::Stop)
    }

    fn progress_key(&self, _state: &Self::State) -> Option<ProgressKey> {
        None
    }
}

#[tokio::test]
async fn custom_pagination_controller_drives_query_headers_and_stop() -> Result<(), ApiClientError>
{
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "a,b"),
            MockResponse::text(StatusCode::OK, "c"),
        ],
    );
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let endpoint = ItemsEndpoint {
        policy: Default::default(),
        pagination: PaginationPlan::custom::<HeaderTokenPagination, Vec<String>>(),
    };

    let items = client.request(endpoint).paginate().collect().await?;

    assert_eq!(
        items,
        vec!["a".to_string(), "b".to_string(), "c".to_string()]
    );
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 2);
    assert_eq!(
        query_value(&requests[0].url, "cursor"),
        Some("0".to_string())
    );
    assert_eq!(
        query_value(&requests[1].url, "cursor"),
        Some("1".to_string())
    );
    assert_eq!(
        requests[0]
            .headers
            .get("x-page-token")
            .and_then(|v| v.to_str().ok()),
        Some("0")
    );
    assert_eq!(
        requests[1]
            .headers
            .get("x-page-token")
            .and_then(|v| v.to_str().ok()),
        Some("1")
    );
    Ok(())
}

#[tokio::test]
async fn invalid_pagination_header_name_returns_typed_error_without_transport() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, "unused")]);
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let endpoint = ItemsEndpoint {
        policy: Default::default(),
        pagination: PaginationPlan::custom::<InvalidHeaderPagination, Vec<String>>(),
    };

    let err = client
        .request(endpoint)
        .paginate()
        .collect()
        .await
        .expect_err("invalid pagination header should be a typed error");

    assert!(matches!(err, ApiClientError::Pagination { .. }));
    let msg = err.to_string();
    assert!(msg.contains("Items"));
    assert!(msg.contains("invalid pagination header name"));
    assert_eq!(sent.sent_count().await, 0);
}

#[tokio::test]
async fn dynamic_pagination_query_and_header_names_work() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, "a")]);
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let endpoint = ItemsEndpoint {
        policy: Default::default(),
        pagination: PaginationPlan::custom::<DynamicRequestMutationPagination, Vec<String>>(),
    };

    let items = client.request(endpoint).paginate().collect().await?;

    assert_eq!(items, vec!["a".to_string()]);
    let requests = sent.requests().await;
    assert_eq!(
        query_value(&requests[0].url, "dynamic_page"),
        Some("7".to_string())
    );
    assert_eq!(
        requests[0]
            .headers
            .get("x-dynamic-page")
            .and_then(|v| v.to_str().ok()),
        Some("dynamic-header-value")
    );
    Ok(())
}

#[tokio::test]
async fn retry_on_page_n_does_not_advance_page_state() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, "retry-me"),
            MockResponse::text(StatusCode::OK, "a,b"),
            MockResponse::text(StatusCode::OK, "c"),
        ],
    );
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let endpoint = ItemsEndpoint {
        policy: retry_policy(2),
        pagination: PaginationPlan::OffsetLimit {
            offset_key: "offset".to_string(),
            limit_key: "limit".to_string(),
            offset: 0,
            limit: 2,
            stop_on_short_page: true,
            stop: Default::default(),
        },
    };

    let items = client
        .request(endpoint)
        .paginate()
        .max_pages(4)
        .collect()
        .await?;

    assert_eq!(
        items,
        vec!["a".to_string(), "b".to_string(), "c".to_string()]
    );
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].meta.page_index, 0);
    assert_eq!(requests[1].meta.page_index, 0);
    assert_eq!(
        query_value(&requests[0].url, "offset"),
        Some("0".to_string())
    );
    assert_eq!(
        query_value(&requests[1].url, "offset"),
        Some("0".to_string())
    );
    assert_eq!(requests[2].meta.page_index, 1);
    assert_eq!(
        query_value(&requests[2].url, "offset"),
        Some("2".to_string())
    );
    Ok(())
}

#[tokio::test]
async fn offset_pagination_collects_page_items_without_has_next_cursor()
-> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "a,b"),
            MockResponse::text(StatusCode::OK, "c"),
        ],
    );
    let client = client(TestAuthVars::default(), transport);

    let endpoint = PageOnlyItemsEndpoint {
        policy: Default::default(),
        pagination: PaginationPlan::OffsetLimit {
            offset_key: "offset".to_string(),
            limit_key: "limit".to_string(),
            offset: 0,
            limit: 2,
            stop_on_short_page: true,
            stop: Default::default(),
        },
    };

    let items = client.request(endpoint).paginate().collect().await?;

    assert_eq!(
        items,
        vec!["a".to_string(), "b".to_string(), "c".to_string()]
    );
    Ok(())
}

#[tokio::test]
async fn cursor_pagination_collects_until_cursor_missing() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "a,b|next=next-1"),
            MockResponse::text(StatusCode::OK, "c|next="),
        ],
    );
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let endpoint = CursorItemsEndpoint {
        policy: Default::default(),
        pagination: PaginationPlan::cursor::<CursorItems>(CursorPagination {
            cursor_key: "cursor".into(),
            per_page_key: "limit".into(),
            cursor: Some("start".to_string()),
            per_page: 2,
            send_cursor_on_first: true,
            stop_when_cursor_missing: true,
            stop: Default::default(),
        }),
    };

    let items = client.request(endpoint).paginate().collect().await?;

    assert_eq!(
        items,
        vec!["a".to_string(), "b".to_string(), "c".to_string()]
    );
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 2);
    assert_eq!(
        query_value(&requests[0].url, "cursor"),
        Some("start".to_string())
    );
    assert_eq!(
        query_value(&requests[1].url, "cursor"),
        Some("next-1".to_string())
    );
    Ok(())
}

#[tokio::test]
async fn paged_pagination_collects_page_items_without_has_next_cursor() -> Result<(), ApiClientError>
{
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "a,b"),
            MockResponse::text(StatusCode::OK, "c"),
        ],
    );
    let client = client(TestAuthVars::default(), transport);

    let endpoint = PageOnlyItemsEndpoint {
        policy: Default::default(),
        pagination: PaginationPlan::Paged {
            page_key: "page".to_string(),
            per_page_key: "per_page".to_string(),
            page: 1,
            per_page: 2,
            stop_on_short_page: true,
            stop: Default::default(),
        },
    };

    let items = client.request(endpoint).paginate().collect().await?;

    assert_eq!(
        items,
        vec!["a".to_string(), "b".to_string(), "c".to_string()]
    );
    Ok(())
}

#[tokio::test]
async fn paged_pagination_uses_page_numbers() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "a,b"),
            MockResponse::text(StatusCode::OK, "c"),
        ],
    );
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let endpoint = ItemsEndpoint {
        policy: Default::default(),
        pagination: PaginationPlan::Paged {
            page_key: "page".to_string(),
            per_page_key: "per_page".to_string(),
            page: 1,
            per_page: 2,
            stop_on_short_page: true,
            stop: Default::default(),
        },
    };

    let items = client.request(endpoint).paginate().collect().await?;

    assert_eq!(
        items,
        vec!["a".to_string(), "b".to_string(), "c".to_string()]
    );
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 2);
    assert_eq!(query_value(&requests[0].url, "page"), Some("1".to_string()));
    assert_eq!(query_value(&requests[1].url, "page"), Some("2".to_string()));
    Ok(())
}

#[tokio::test]
async fn pagination_max_pages_zero_returns_typed_error_before_transport() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, "unused")]);
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let endpoint = ItemsEndpoint {
        policy: Default::default(),
        pagination: PaginationPlan::OffsetLimit {
            offset_key: "offset".to_string(),
            limit_key: "limit".to_string(),
            offset: 0,
            limit: 2,
            stop_on_short_page: true,
            stop: Default::default(),
        },
    };

    let err = client
        .request(endpoint)
        .paginate()
        .max_pages(0)
        .collect()
        .await
        .expect_err("zero max_pages should fail before transport");

    assert!(matches!(err, ApiClientError::Pagination { .. }));
    assert!(err.to_string().contains("max_pages"));
    assert_eq!(sent.sent_count().await, 0);
}

#[tokio::test]
async fn pagination_max_items_zero_returns_typed_error_before_transport() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, "unused")]);
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let endpoint = ItemsEndpoint {
        policy: Default::default(),
        pagination: PaginationPlan::OffsetLimit {
            offset_key: "offset".to_string(),
            limit_key: "limit".to_string(),
            offset: 0,
            limit: 2,
            stop_on_short_page: true,
            stop: Default::default(),
        },
    };

    let err = client
        .request(endpoint)
        .paginate()
        .max_items(0)
        .collect()
        .await
        .expect_err("zero max_items should fail before transport");

    assert!(matches!(err, ApiClientError::Pagination { .. }));
    assert!(err.to_string().contains("max_items"));
    assert_eq!(sent.sent_count().await, 0);
}

#[tokio::test]
async fn for_each_page_receives_decoded_pages() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "a,b"),
            MockResponse::text(StatusCode::OK, "c"),
        ],
    );
    let client = client(TestAuthVars::default(), transport);

    let endpoint = ItemsEndpoint {
        policy: Default::default(),
        pagination: PaginationPlan::OffsetLimit {
            offset_key: "offset".to_string(),
            limit_key: "limit".to_string(),
            offset: 0,
            limit: 2,
            stop_on_short_page: true,
            stop: Default::default(),
        },
    };
    let pages = Arc::new(Mutex::new(Vec::new()));
    let pages_seen = pages.clone();

    client
        .request(endpoint)
        .paginate()
        .for_each_page(move |page| {
            let pages_seen = pages_seen.clone();
            async move {
                pages_seen.lock().await.push(page.value);
                Ok(())
            }
        })
        .await?;

    assert_eq!(
        pages.lock().await.clone(),
        vec![
            vec!["a".to_string(), "b".to_string()],
            vec!["c".to_string()]
        ]
    );
    Ok(())
}

#[tokio::test]
async fn max_items_error_includes_page_context() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, "a,b,c")]);
    let client = client(TestAuthVars::default(), transport);

    let endpoint = ItemsEndpoint {
        policy: Default::default(),
        pagination: PaginationPlan::OffsetLimit {
            offset_key: "offset".to_string(),
            limit_key: "limit".to_string(),
            offset: 0,
            limit: 3,
            stop_on_short_page: true,
            stop: Default::default(),
        },
    };

    let err = client
        .request(endpoint)
        .paginate()
        .max_items(2)
        .collect()
        .await
        .expect_err("max_items should stop pagination");

    let msg = err.to_string();
    assert!(msg.contains("max_items"));
    assert!(msg.contains("Items"));
    assert!(msg.contains("page_index=0"));
}

#[tokio::test]
async fn collect_enforces_max_items_from_actual_items_without_hint() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, "a,b")]);
    let client = client(TestAuthVars::default(), transport);

    let endpoint = NoHintItemsEndpoint {
        policy: Default::default(),
        pagination: PaginationPlan::custom::<StopAfterFirstNoHintPagination, NoHintItems>(),
    };

    let items = client
        .request(endpoint)
        .paginate()
        .max_items(2)
        .collect()
        .await?;

    assert_eq!(items, vec!["a".to_string(), "b".to_string()]);
    Ok(())
}

#[tokio::test]
async fn collect_rejects_page_exceeding_max_items_without_hint() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, "a,b")]);
    let client = client(TestAuthVars::default(), transport);

    let endpoint = NoHintItemsEndpoint {
        policy: Default::default(),
        pagination: PaginationPlan::custom::<StopAfterFirstNoHintPagination, NoHintItems>(),
    };

    let err = client
        .request(endpoint)
        .paginate()
        .max_items(1)
        .collect()
        .await
        .expect_err("actual collected item count must enforce max_items");

    assert!(matches!(err, ApiClientError::PaginationLimit { .. }));
    let msg = err.to_string();
    assert!(msg.contains("max_items"));
    assert!(msg.contains("NoHintItems"));
    assert!(msg.contains("page_index=0"));
}

#[tokio::test]
async fn max_pages_error_includes_seen_items() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "a,b"),
            MockResponse::text(StatusCode::OK, "c,d"),
        ],
    );
    let client = client(TestAuthVars::default(), transport);

    let endpoint = ItemsEndpoint {
        policy: Default::default(),
        pagination: PaginationPlan::OffsetLimit {
            offset_key: "offset".to_string(),
            limit_key: "limit".to_string(),
            offset: 0,
            limit: 2,
            stop_on_short_page: false,
            stop: Default::default(),
        },
    };

    let err = client
        .request(endpoint)
        .paginate()
        .max_pages(1)
        .collect()
        .await
        .expect_err("max_pages should stop pagination");

    let msg = err.to_string();
    assert!(msg.contains("max_pages"));
    assert!(msg.contains("seen_items=2"));
    assert!(msg.contains("page_index=1"));
}

#[tokio::test]
async fn auth_refresh_on_page_n_preserves_offset() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "a,b"),
            MockResponse::text(StatusCode::UNAUTHORIZED, "expired"),
            MockResponse::text(StatusCode::OK, "c"),
        ],
    );
    let sent = transport.clone();
    let client = client(
        TestAuthVars {
            token: Some("refreshable".to_string()),
            identity: "refresh",
        },
        transport,
    );

    let endpoint = ItemsEndpoint {
        policy: auth_policy(AuthPlacement::Bearer),
        pagination: PaginationPlan::OffsetLimit {
            offset_key: "offset".to_string(),
            limit_key: "limit".to_string(),
            offset: 0,
            limit: 2,
            stop_on_short_page: true,
            stop: Default::default(),
        },
    };

    let items = client.request(endpoint).paginate().collect().await?;

    assert_eq!(
        items,
        vec!["a".to_string(), "b".to_string(), "c".to_string()]
    );
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[1].meta.page_index, 1);
    assert_eq!(requests[2].meta.page_index, 1);
    assert_eq!(
        query_value(&requests[1].url, "offset"),
        Some("2".to_string())
    );
    assert_eq!(
        query_value(&requests[2].url, "offset"),
        Some("2".to_string())
    );
    Ok(())
}

#[tokio::test]
async fn stale_decode_failure_does_not_advance_page_state() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let stale = built_response("Items", StatusCode::OK, Bytes::from_static(b"\xff"));
    let cache = Arc::new(RecordingCache::revalidate_stale_on_error(
        events.clone(),
        stale,
    ));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, "retry-me"),
            MockResponse::text(StatusCode::OK, "should-not-request-next-page"),
        ],
    );
    let sent = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    configure_runtime(&mut client, Some(cache), None);

    let endpoint = ItemsEndpoint {
        policy: cache_policy(),
        pagination: PaginationPlan::OffsetLimit {
            offset_key: "offset".to_string(),
            limit_key: "limit".to_string(),
            offset: 0,
            limit: 2,
            stop_on_short_page: true,
            stop: Default::default(),
        },
    };

    let err = client
        .request(endpoint)
        .paginate()
        .max_pages(4)
        .collect()
        .await
        .expect_err("invalid stale page should fail before page advance");

    assert!(err.to_string().contains("decode error"));
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].meta.page_index, 0);
    assert_eq!(
        query_value(&requests[0].url, "offset"),
        Some("0".to_string())
    );
}

fn query_value(url: &url::Url, name: &str) -> Option<String> {
    url.query_pairs()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
}

use super::common::*;
use bytes::Bytes;
use concord_core::advanced::{
    AuthPlacement, BuiltRequest, BuiltResponse, CacheAfter, CacheBefore, CacheFuture,
    CacheRevalidation, CacheStore, PageAdvance, PageDecision, PageInit, PageRequest,
    PaginationController, ProgressKey, default_cache_key,
};
use concord_core::internal::PaginationPlan;
use concord_core::prelude::{ApiClientError, CursorPagination, PaginationTermination};
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

#[derive(Clone, Default)]
struct RecordingKeyCache {
    keys: Arc<Mutex<Vec<String>>>,
}

impl RecordingKeyCache {
    fn new(keys: Arc<Mutex<Vec<String>>>) -> Self {
        Self { keys }
    }
}

impl CacheStore for RecordingKeyCache {
    fn before_request<'a>(&'a self, request: &'a BuiltRequest) -> CacheFuture<'a, CacheBefore> {
        Box::pin(async move {
            self.keys
                .lock()
                .await
                .push(default_cache_key(request).as_str().to_string());
            CacheBefore::Miss
        })
    }

    fn after_response<'a>(
        &'a self,
        _request: &'a BuiltRequest,
        _response: &'a BuiltResponse,
        _revalidation: Option<CacheRevalidation>,
    ) -> CacheFuture<'a, CacheAfter> {
        Box::pin(async move { CacheAfter::Stored })
    }

    fn after_error<'a>(
        &'a self,
        _request: &'a BuiltRequest,
        _error: &'a ApiClientError,
        _revalidation: Option<CacheRevalidation>,
    ) -> CacheFuture<'a, Option<BuiltResponse>> {
        Box::pin(async move { None })
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

#[derive(Default)]
struct ConstantProgressChangingQueryPagination;

struct ConstantProgressChangingQueryState {
    page: u64,
}

impl PaginationController<Vec<String>> for ConstantProgressChangingQueryPagination {
    type State = ConstantProgressChangingQueryState;

    fn init(&self, _ctx: PageInit<'_>) -> Result<Self::State, ApiClientError> {
        Ok(ConstantProgressChangingQueryState { page: 0 })
    }

    fn apply(
        &self,
        state: &Self::State,
        request: &mut PageRequest<'_>,
    ) -> Result<(), ApiClientError> {
        request.set_query("page", state.page);
        Ok(())
    }

    fn advance(
        &self,
        state: &mut Self::State,
        _page: &Vec<String>,
        _ctx: PageAdvance<'_>,
    ) -> Result<PageDecision, ApiClientError> {
        state.page += 1;
        Ok(PageDecision::Continue)
    }

    fn progress_key(&self, _state: &Self::State) -> Option<ProgressKey> {
        Some(ProgressKey::Str("constant-loop-key".to_string()))
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

    let items = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(100))
        .collect()
        .await?;

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
        .paginate(PaginationTermination::hard_page_cap(100))
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

    let items = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(100))
        .collect()
        .await?;

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
        .paginate(PaginationTermination::hard_page_cap(4))
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

    let items = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(100))
        .collect()
        .await?;

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

    let items = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(100))
        .collect()
        .await?;

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
async fn cursor_pagination_repeated_cursor_returns_non_progress_error() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "a,b|next=next-1"),
            MockResponse::text(StatusCode::OK, "c|next=next-1"),
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

    let err = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(100))
        .detect_loops(false)
        .collect()
        .await
        .expect_err("repeated cursor should stop with a typed pagination error");

    assert!(matches!(err, ApiClientError::Pagination { .. }));
    assert!(err.to_string().contains("non-progress"));
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
}

#[tokio::test]
async fn cursor_pagination_cyclic_cursor_returns_non_progress_error() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "a,b|next=start-b"),
            MockResponse::text(StatusCode::OK, "c|next=start-a"),
        ],
    );
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let endpoint = CursorItemsEndpoint {
        policy: Default::default(),
        pagination: PaginationPlan::cursor::<CursorItems>(CursorPagination {
            cursor_key: "cursor".into(),
            per_page_key: "limit".into(),
            cursor: Some("start-a".to_string()),
            per_page: 2,
            send_cursor_on_first: true,
            stop_when_cursor_missing: true,
            stop: Default::default(),
        }),
    };

    let err = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(100))
        .detect_loops(false)
        .collect()
        .await
        .expect_err("cyclic cursor should stop with a typed pagination error");

    assert!(matches!(err, ApiClientError::Pagination { .. }));
    assert!(err.to_string().contains("non-progress"));
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 2);
    assert_eq!(
        query_value(&requests[0].url, "cursor"),
        Some("start-a".to_string())
    );
    assert_eq!(
        query_value(&requests[1].url, "cursor"),
        Some("start-b".to_string())
    );
}

#[tokio::test]
async fn cursor_pagination_missing_cursor_without_stop_is_non_progress_error() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![MockResponse::text(StatusCode::OK, "a,b|next=")],
    );
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let endpoint = CursorItemsEndpoint {
        policy: Default::default(),
        pagination: PaginationPlan::cursor::<CursorItems>(CursorPagination {
            cursor_key: "cursor".into(),
            per_page_key: "limit".into(),
            cursor: None,
            per_page: 2,
            send_cursor_on_first: false,
            stop_when_cursor_missing: false,
            stop: Default::default(),
        }),
    };

    let err = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(100))
        .detect_loops(false)
        .collect()
        .await
        .expect_err("missing cursor without stop should not loop forever");

    assert!(matches!(err, ApiClientError::Pagination { .. }));
    assert!(err.to_string().contains("non-progress"));
    assert_eq!(sent.sent_count().await, 1);
    let requests = sent.requests().await;
    assert!(query_value(&requests[0].url, "cursor").is_none());
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

    let items = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(100))
        .collect()
        .await?;

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

    let items = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(100))
        .collect()
        .await?;

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
async fn hard_page_cap_zero_errors_before_transport() {
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
        .paginate(PaginationTermination::hard_page_cap(0))
        .collect()
        .await
        .expect_err("zero hard page cap should fail before transport");

    assert!(matches!(err, ApiClientError::Pagination { .. }));
    assert!(err.to_string().contains("hard pagination page cap"));
    assert_eq!(sent.sent_count().await, 0);
}

#[tokio::test]
async fn hard_item_cap_zero_errors_before_transport() {
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
        .paginate(PaginationTermination::hard_item_cap(0))
        .collect()
        .await
        .expect_err("zero hard item cap should fail before transport");

    assert!(matches!(err, ApiClientError::Pagination { .. }));
    assert!(err.to_string().contains("hard pagination item cap"));
    assert_eq!(sent.sent_count().await, 0);
}

#[tokio::test]
async fn take_pages_zero_returns_empty_without_transport() -> Result<(), ApiClientError> {
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
            stop_on_short_page: false,
            stop: Default::default(),
        },
    };

    let items = client
        .request(endpoint)
        .paginate(PaginationTermination::take_pages(0))
        .collect()
        .await?;

    assert!(items.is_empty());
    assert_eq!(sent.sent_count().await, 0);
    Ok(())
}

#[tokio::test]
async fn take_items_zero_returns_empty_without_transport() -> Result<(), ApiClientError> {
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
            stop_on_short_page: false,
            stop: Default::default(),
        },
    };

    let items = client
        .request(endpoint)
        .paginate(PaginationTermination::take_items(0))
        .collect()
        .await?;

    assert!(items.is_empty());
    assert_eq!(sent.sent_count().await, 0);
    Ok(())
}

#[tokio::test]
async fn take_items_truncates_final_page() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, numbered_items(0, 20)),
            MockResponse::text(StatusCode::OK, numbered_items(20, 20)),
            MockResponse::text(StatusCode::OK, numbered_items(40, 20)),
            MockResponse::text(StatusCode::OK, "should-not-send"),
        ],
    );
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let endpoint = ItemsEndpoint {
        policy: Default::default(),
        pagination: PaginationPlan::OffsetLimit {
            offset_key: "offset".to_string(),
            limit_key: "limit".to_string(),
            offset: 0,
            limit: 20,
            stop_on_short_page: false,
            stop: Default::default(),
        },
    };

    let items = client
        .request(endpoint)
        .paginate(PaginationTermination::take_items(50))
        .collect()
        .await?;

    assert_eq!(items.len(), 50);
    assert_eq!(items.first().map(String::as_str), Some("item-0"));
    assert_eq!(items.last().map(String::as_str), Some("item-49"));
    assert_eq!(sent.sent_count().await, 3);
    Ok(())
}

#[tokio::test]
async fn take_items_less_than_first_page_sends_one_page() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, numbered_items(0, 20)),
            MockResponse::text(StatusCode::OK, "should-not-send"),
        ],
    );
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let endpoint = ItemsEndpoint {
        policy: Default::default(),
        pagination: PaginationPlan::OffsetLimit {
            offset_key: "offset".to_string(),
            limit_key: "limit".to_string(),
            offset: 0,
            limit: 20,
            stop_on_short_page: false,
            stop: Default::default(),
        },
    };

    let items = client
        .request(endpoint)
        .paginate(PaginationTermination::take_items(10))
        .collect()
        .await?;

    assert_eq!(items.len(), 10);
    assert_eq!(items.last().map(String::as_str), Some("item-9"));
    assert_eq!(sent.sent_count().await, 1);
    Ok(())
}

#[tokio::test]
async fn take_items_exact_boundary_stops_without_extra_page() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, numbered_items(0, 20)),
            MockResponse::text(StatusCode::OK, numbered_items(20, 20)),
            MockResponse::text(StatusCode::OK, "should-not-send"),
        ],
    );
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let endpoint = ItemsEndpoint {
        policy: Default::default(),
        pagination: PaginationPlan::OffsetLimit {
            offset_key: "offset".to_string(),
            limit_key: "limit".to_string(),
            offset: 0,
            limit: 20,
            stop_on_short_page: false,
            stop: Default::default(),
        },
    };

    let items = client
        .request(endpoint)
        .paginate(PaginationTermination::take_items(40))
        .collect()
        .await?;

    assert_eq!(items.len(), 40);
    assert_eq!(items.last().map(String::as_str), Some("item-39"));
    assert_eq!(sent.sent_count().await, 2);
    Ok(())
}

#[tokio::test]
async fn take_pages_stops_without_error() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "a,b"),
            MockResponse::text(StatusCode::OK, "c,d"),
            MockResponse::text(StatusCode::OK, "should-not-send"),
        ],
    );
    let sent = transport.clone();
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

    let items = client
        .request(endpoint)
        .paginate(PaginationTermination::take_pages(2))
        .collect()
        .await?;

    assert_eq!(
        items,
        vec![
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string()
        ]
    );
    assert_eq!(sent.sent_count().await, 2);
    Ok(())
}

#[tokio::test]
async fn hard_page_cap_errors_without_fetching_extra_page() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "a,b"),
            MockResponse::text(StatusCode::OK, "c,d"),
            MockResponse::text(StatusCode::OK, "should-not-send"),
        ],
    );
    let sent = transport.clone();
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
        .paginate(PaginationTermination::hard_page_cap(2))
        .collect()
        .await
        .expect_err("hard page cap should fail before fetching page 3");

    assert!(matches!(err, ApiClientError::PaginationLimit { .. }));
    assert!(err.to_string().contains("hard page cap"));
    assert_eq!(sent.sent_count().await, 2);
}

#[tokio::test]
async fn hard_item_cap_errors_without_truncating() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, numbered_items(0, 20)),
            MockResponse::text(StatusCode::OK, numbered_items(20, 20)),
            MockResponse::text(StatusCode::OK, "should-not-send"),
        ],
    );
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let endpoint = ItemsEndpoint {
        policy: Default::default(),
        pagination: PaginationPlan::OffsetLimit {
            offset_key: "offset".to_string(),
            limit_key: "limit".to_string(),
            offset: 0,
            limit: 20,
            stop_on_short_page: false,
            stop: Default::default(),
        },
    };

    let err = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_item_cap(30))
        .collect()
        .await
        .expect_err("hard item cap should fail rather than truncate");

    assert!(matches!(err, ApiClientError::PaginationLimit { .. }));
    assert!(err.to_string().contains("hard item cap"));
    assert_eq!(sent.sent_count().await, 2);
}

#[tokio::test]
async fn loop_detection_still_default_enabled() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "a,b|next=next-1"),
            MockResponse::text(StatusCode::OK, "c|next=next-1"),
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

    let err = client
        .request(endpoint)
        .paginate(PaginationTermination::take_pages(10))
        .collect()
        .await
        .expect_err("loop detection should run before soft page termination");

    assert!(matches!(err, ApiClientError::Pagination { .. }));
    assert!(err.to_string().contains("loop detected"));
    assert_eq!(sent.sent_count().await, 2);
}

#[tokio::test]
async fn detect_loops_false_allows_repeated_progress_until_termination()
-> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "a"),
            MockResponse::text(StatusCode::OK, "b"),
            MockResponse::text(StatusCode::OK, "should-not-send"),
        ],
    );
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let endpoint = ItemsEndpoint {
        policy: Default::default(),
        pagination: PaginationPlan::custom::<ConstantProgressChangingQueryPagination, Vec<String>>(
        ),
    };

    let items = client
        .request(endpoint)
        .paginate(PaginationTermination::take_pages(2))
        .detect_loops(false)
        .collect()
        .await?;

    assert_eq!(items, vec!["a".to_string(), "b".to_string()]);
    assert_eq!(sent.sent_count().await, 2);
    Ok(())
}

#[tokio::test]
async fn for_each_page_take_pages_stops_cleanly() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "a,b"),
            MockResponse::text(StatusCode::OK, "c,d"),
            MockResponse::text(StatusCode::OK, "should-not-send"),
        ],
    );
    let sent = transport.clone();
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
    let pages = Arc::new(Mutex::new(Vec::new()));
    let pages_seen = pages.clone();

    client
        .request(endpoint)
        .paginate(PaginationTermination::take_pages(2))
        .for_each_page(move |page| {
            let pages_seen = pages_seen.clone();
            async move {
                pages_seen.lock().await.push(page.value);
                Ok(())
            }
        })
        .await?;

    assert_eq!(sent.sent_count().await, 2);
    assert_eq!(pages.lock().await.len(), 2);
    Ok(())
}

#[tokio::test]
async fn for_each_page_take_items_rejected_before_transport() {
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
            stop_on_short_page: false,
            stop: Default::default(),
        },
    };

    let err = client
        .request(endpoint)
        .paginate(PaginationTermination::take_items(10))
        .for_each_page(|_| async { Ok(()) })
        .await
        .expect_err("TakeItems cannot be exact for page callbacks");

    assert!(matches!(err, ApiClientError::Pagination { .. }));
    assert!(err.to_string().contains("TakeItems"));
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
        .paginate(PaginationTermination::hard_page_cap(100))
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
        .paginate(PaginationTermination::hard_item_cap(2))
        .collect()
        .await
        .expect_err("hard item cap should stop pagination");

    let msg = err.to_string();
    assert!(msg.contains("hard item cap"));
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
        .paginate(PaginationTermination::hard_item_cap(2))
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
        .paginate(PaginationTermination::hard_item_cap(1))
        .collect()
        .await
        .expect_err("actual collected item count must enforce max_items");

    assert!(matches!(err, ApiClientError::PaginationLimit { .. }));
    let msg = err.to_string();
    assert!(msg.contains("hard item cap"));
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
        .paginate(PaginationTermination::hard_page_cap(1))
        .collect()
        .await
        .expect_err("hard page cap should stop pagination");

    let msg = err.to_string();
    assert!(msg.contains("hard page cap"));
    assert!(msg.contains("seen_items=2"));
    assert!(msg.contains("page_index=1"));
}

#[tokio::test]
async fn pagination_cache_keys_change_per_page_and_keep_auth_partitioning()
-> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let key_events = Arc::new(Mutex::new(Vec::new()));
    let cache = Arc::new(RecordingKeyCache::new(key_events.clone()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "a,b|next=next-1"),
            MockResponse::text(StatusCode::OK, "c|next="),
        ],
    );
    let sent = transport.clone();
    let mut client = client(
        TestAuthVars {
            token: Some("CACHE_PARTITION_TOKEN".to_string()),
            identity: "tenant-a",
        },
        transport,
    );
    configure_runtime(&mut client, Some(cache), None);

    let endpoint = CursorItemsEndpoint {
        policy: {
            let mut policy = auth_policy(AuthPlacement::Bearer);
            policy.cache = concord_core::internal::CacheSetting::Config(
                concord_core::advanced::CacheConfig::new(),
            );
            policy
        },
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

    let items = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(100))
        .collect()
        .await?;

    assert_eq!(
        items,
        vec!["a".to_string(), "b".to_string(), "c".to_string()]
    );
    assert_eq!(sent.sent_count().await, 2);
    let keys = key_events.lock().await.clone();
    assert_eq!(keys.len(), 2);
    assert_ne!(keys[0], keys[1]);
    assert!(keys.iter().all(|key| key.contains("|auth=")));
    assert!(!keys.iter().any(|key| key.contains("CACHE_PARTITION_TOKEN")));
    Ok(())
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

    let items = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(100))
        .collect()
        .await?;

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
async fn auth_refresh_on_page_n_preserves_page_state_and_does_not_use_stale_fallback()
-> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let stale = built_response(
        "Items",
        StatusCode::OK,
        "STALE_PROTECTED_RESPONSE_MUST_NOT_BE_SERVED_AFTER_AUTH_REJECTION",
    );
    let cache = Arc::new(RecordingCache::revalidate_stale_on_error(
        events.clone(),
        stale,
    ));
    let after_error_count = cache.after_error_count.clone();
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "a,b"),
            MockResponse::text(StatusCode::UNAUTHORIZED, "expired"),
            MockResponse::text(StatusCode::OK, "c"),
        ],
    );
    let sent = transport.clone();
    let mut client = client(
        TestAuthVars {
            token: Some("refreshable".to_string()),
            identity: "refresh",
        },
        transport,
    );
    configure_runtime(&mut client, Some(cache), None);

    let endpoint = ItemsEndpoint {
        policy: {
            let mut policy = auth_policy(AuthPlacement::Bearer);
            policy.cache = concord_core::internal::CacheSetting::Config(
                concord_core::advanced::CacheConfig::new(),
            );
            policy
        },
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
        .paginate(PaginationTermination::hard_page_cap(100))
        .collect()
        .await?;

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
    assert_eq!(*after_error_count.lock().await, 0);
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
        .paginate(PaginationTermination::hard_page_cap(4))
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

fn numbered_items(start: usize, count: usize) -> String {
    (start..start + count)
        .map(|idx| format!("item-{idx}"))
        .collect::<Vec<_>>()
        .join(",")
}

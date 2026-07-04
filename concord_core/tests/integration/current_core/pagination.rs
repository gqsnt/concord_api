use super::common::*;
use crate::support::{RedactionSentinels, assert_error_chain_does_not_contain_any};
use bytes::Bytes;
use concord_core::advanced::{
    AuthPlacement, EndpointPagination, PageAdvance, PageApply, PageDecision, PaginateBinding,
    PaginationRuntime, PaginationRuntimeAdapter, ProgressKey,
};
use concord_core::error::ErrorCategory;
use concord_core::internal::ResolvedPolicy;
use concord_core::prelude::{
    ApiClientError, CursorPagination, Endpoint, PageItems, PagedPagination, PaginatedEndpoint,
    PaginationTermination, ReusableEndpoint,
};
use concord_core::transport::TransportErrorKind;
use http::{HeaderName, HeaderValue, Method, StatusCode};
use std::future::Future;
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::Mutex;

tokio::task_local! {
    static PR64_CUSTOM_PAGINATION_EVENTS: Arc<StdMutex<Vec<&'static str>>>;
}

tokio::task_local! {
    static PR69_MUTATION_EVENTS: Arc<StdMutex<Vec<&'static str>>>;
}

#[derive(Clone)]
struct GeneratedHeaderBoundCustomEndpoint {
    page: u64,
    count: u64,
    load_calls: Arc<AtomicUsize>,
    store_calls: Arc<AtomicUsize>,
}

impl Endpoint<TestCx> for GeneratedHeaderBoundCustomEndpoint {
    type Response = Vec<String>;

    fn execute<'a, T>(
        client: &'a concord_core::prelude::ApiClient<TestCx, T>,
        plan: concord_core::internal::RequestPlan,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Response, ApiClientError>> + Send + 'a>>
    where
        T: concord_core::advanced::Transport + 'a,
    {
        execute_buffered::<_, _, CommaSeparatedItems>(client, plan)
    }
}

impl ReusableEndpoint<TestCx> for GeneratedHeaderBoundCustomEndpoint {
    fn plan(
        &self,
        _ctx: &concord_core::internal::ClientPlanContext<'_, TestCx>,
    ) -> Result<concord_core::internal::RequestPlan, ApiClientError> {
        let mut plan = request_plan(
            "GeneratedHeaderBoundCustom",
            Method::GET,
            "/generated-header-custom",
            Default::default(),
            None,
        );
        plan.endpoint.policy.headers.insert(
            http::header::HeaderName::from_static("x-page"),
            HeaderValue::from_str(&self.page.to_string()).expect("valid header value"),
        );
        plan.endpoint.policy.headers.insert(
            http::header::HeaderName::from_static("x-count"),
            HeaderValue::from_str(&self.count.to_string()).expect("valid header value"),
        );
        Ok(plan)
    }
}

impl PaginatedEndpoint<TestCx> for GeneratedHeaderBoundCustomEndpoint {
    type Pagination = HeaderBoundCustomPagination;

    fn pagination_runtime(
        &self,
    ) -> Option<Box<dyn concord_core::advanced::PaginationRuntime<Self, Self::Response>>> {
        Some(Box::new(PaginationRuntimeAdapter::<
            HeaderBoundCustomPagination,
        >::new()))
    }
}

#[derive(Default)]
struct HeaderBoundCustomPagination {
    page: u64,
    count: u64,
}

impl EndpointPagination<Vec<String>> for HeaderBoundCustomPagination {
    fn apply(&mut self, _ctx: PageApply<'_>) -> Result<(), ApiClientError> {
        if self.count == 0 {
            return Err(ApiClientError::pagination(
                concord_core::advanced::ErrorContext {
                    endpoint: "GeneratedHeaderBoundCustom",
                    method: Method::GET,
                },
                concord_core::error::PaginationErrorKind::InvalidSize,
                "custom pagination page size must be non-zero",
            ));
        }
        Ok(())
    }

    fn expected_items_per_page(&self) -> Option<NonZeroUsize> {
        usize::try_from(self.count).ok().and_then(NonZeroUsize::new)
    }

    fn advance(
        &mut self,
        page: &Vec<String>,
        _ctx: PageAdvance<'_>,
    ) -> Result<PageDecision, ApiClientError> {
        if page.is_empty() {
            return Ok(PageDecision::Stop);
        }
        self.page = self.page.checked_add(1).ok_or_else(|| {
            ApiClientError::pagination(
                concord_core::advanced::ErrorContext {
                    endpoint: "GeneratedHeaderBoundCustom",
                    method: Method::GET,
                },
                concord_core::error::PaginationErrorKind::Overflow,
                "custom pagination page overflow",
            )
        })?;
        Ok(PageDecision::Continue)
    }

    fn progress_key(&self) -> Option<ProgressKey> {
        Some(ProgressKey::U64(self.page))
    }
}

impl PaginateBinding<HeaderBoundCustomPagination> for GeneratedHeaderBoundCustomEndpoint {
    fn load_pagination(&self) -> HeaderBoundCustomPagination {
        self.load_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        HeaderBoundCustomPagination {
            page: self.page,
            count: self.count,
        }
    }

    fn store_pagination(&mut self, pagination: &HeaderBoundCustomPagination) {
        self.store_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.page = pagination.page;
        self.count = pagination.count;
    }
}

#[derive(Clone)]
struct HeaderBoundOffsetLimitEndpoint {
    start: u64,
    count: u64,
}

impl Endpoint<TestCx> for HeaderBoundOffsetLimitEndpoint {
    type Response = Vec<String>;

    fn execute<'a, T>(
        client: &'a concord_core::prelude::ApiClient<TestCx, T>,
        plan: concord_core::internal::RequestPlan,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Response, ApiClientError>> + Send + 'a>>
    where
        T: concord_core::advanced::Transport + 'a,
    {
        execute_buffered::<_, _, CommaSeparatedItems>(client, plan)
    }
}

impl ReusableEndpoint<TestCx> for HeaderBoundOffsetLimitEndpoint {
    fn plan(
        &self,
        _ctx: &concord_core::internal::ClientPlanContext<'_, TestCx>,
    ) -> Result<concord_core::internal::RequestPlan, ApiClientError> {
        let mut plan = request_plan(
            "HeaderBoundOffsetLimit",
            Method::GET,
            "/header-offset-limit",
            Default::default(),
            Some(concord_core::internal::PaginationMarker),
        );
        plan.endpoint.policy.headers.insert(
            http::header::HeaderName::from_static("x-start"),
            HeaderValue::from_str(&self.start.to_string()).expect("valid header value"),
        );
        plan.endpoint.policy.headers.insert(
            http::header::HeaderName::from_static("x-count"),
            HeaderValue::from_str(&self.count.to_string()).expect("valid header value"),
        );
        Ok(plan)
    }
}

impl PaginatedEndpoint<TestCx> for HeaderBoundOffsetLimitEndpoint {
    type Pagination = concord_core::advanced::OffsetLimitPagination;

    fn pagination_runtime(
        &self,
    ) -> Option<Box<dyn concord_core::advanced::PaginationRuntime<Self, Self::Response>>>
    where
        Self: Sized,
        Self::Response: PageItems,
    {
        Some(Box::new(PaginationRuntimeAdapter::<
            concord_core::advanced::OffsetLimitPagination,
        >::new()))
    }
}

impl PaginateBinding<concord_core::advanced::OffsetLimitPagination>
    for HeaderBoundOffsetLimitEndpoint
{
    fn load_pagination(&self) -> concord_core::advanced::OffsetLimitPagination {
        concord_core::advanced::OffsetLimitPagination {
            offset: self.start,
            limit: self.count,
        }
    }

    fn store_pagination(&mut self, pagination: &concord_core::advanced::OffsetLimitPagination) {
        self.start = pagination.offset;
        self.count = pagination.limit;
    }
}

#[derive(Clone)]
struct HeaderBoundPagedEndpoint {
    page: u64,
    count: u64,
}

impl Endpoint<TestCx> for HeaderBoundPagedEndpoint {
    type Response = Vec<String>;

    fn execute<'a, T>(
        client: &'a concord_core::prelude::ApiClient<TestCx, T>,
        plan: concord_core::internal::RequestPlan,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Response, ApiClientError>> + Send + 'a>>
    where
        T: concord_core::advanced::Transport + 'a,
    {
        execute_buffered::<_, _, CommaSeparatedItems>(client, plan)
    }
}

impl ReusableEndpoint<TestCx> for HeaderBoundPagedEndpoint {
    fn plan(
        &self,
        _ctx: &concord_core::internal::ClientPlanContext<'_, TestCx>,
    ) -> Result<concord_core::internal::RequestPlan, ApiClientError> {
        let mut plan = request_plan(
            "HeaderBoundPaged",
            Method::GET,
            "/header-paged",
            Default::default(),
            Some(concord_core::internal::PaginationMarker),
        );
        plan.endpoint.policy.headers.insert(
            http::header::HeaderName::from_static("x-page"),
            HeaderValue::from_str(&self.page.to_string()).expect("valid header value"),
        );
        plan.endpoint.policy.headers.insert(
            http::header::HeaderName::from_static("x-count"),
            HeaderValue::from_str(&self.count.to_string()).expect("valid header value"),
        );
        Ok(plan)
    }
}

impl PaginatedEndpoint<TestCx> for HeaderBoundPagedEndpoint {
    type Pagination = concord_core::advanced::PagedPagination;

    fn pagination_runtime(
        &self,
    ) -> Option<Box<dyn concord_core::advanced::PaginationRuntime<Self, Self::Response>>>
    where
        Self: Sized,
        Self::Response: PageItems,
    {
        Some(Box::new(PaginationRuntimeAdapter::<
            concord_core::advanced::PagedPagination,
        >::new()))
    }
}

impl PaginateBinding<concord_core::advanced::PagedPagination> for HeaderBoundPagedEndpoint {
    fn load_pagination(&self) -> concord_core::advanced::PagedPagination {
        concord_core::advanced::PagedPagination {
            page: self.page,
            per_page: self.count,
        }
    }

    fn store_pagination(&mut self, pagination: &concord_core::advanced::PagedPagination) {
        self.page = pagination.page;
        self.count = pagination.per_page;
    }
}

#[derive(Clone)]
struct HeaderBoundCursorEndpoint {
    cursor: Option<String>,
    count: u64,
    send_cursor_on_first: bool,
    stop_when_cursor_missing: bool,
}

impl Endpoint<TestCx> for HeaderBoundCursorEndpoint {
    type Response = CursorItems;

    fn execute<'a, T>(
        client: &'a concord_core::prelude::ApiClient<TestCx, T>,
        plan: concord_core::internal::RequestPlan,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Response, ApiClientError>> + Send + 'a>>
    where
        T: concord_core::advanced::Transport + 'a,
    {
        execute_buffered::<_, _, CursorItemsCodec>(client, plan)
    }
}

impl ReusableEndpoint<TestCx> for HeaderBoundCursorEndpoint {
    fn plan(
        &self,
        _ctx: &concord_core::internal::ClientPlanContext<'_, TestCx>,
    ) -> Result<concord_core::internal::RequestPlan, ApiClientError> {
        let mut plan = request_plan(
            "HeaderBoundCursor",
            Method::GET,
            "/header-cursor",
            Default::default(),
            Some(concord_core::internal::PaginationMarker),
        );
        if let Some(cursor) = &self.cursor {
            plan.endpoint.policy.headers.insert(
                http::header::HeaderName::from_static("x-cursor"),
                HeaderValue::from_str(cursor).expect("valid header value"),
            );
        }
        plan.endpoint.policy.headers.insert(
            http::header::HeaderName::from_static("x-count"),
            HeaderValue::from_str(&self.count.to_string()).expect("valid header value"),
        );
        Ok(plan)
    }
}

impl PaginatedEndpoint<TestCx> for HeaderBoundCursorEndpoint {
    type Pagination = concord_core::advanced::CursorPagination<String>;

    fn pagination_runtime(
        &self,
    ) -> Option<Box<dyn concord_core::advanced::PaginationRuntime<Self, Self::Response>>>
    where
        Self: Sized,
        Self::Response: PageItems,
    {
        Some(Box::new(PaginationRuntimeAdapter::<
            concord_core::advanced::CursorPagination<String>,
        >::new()))
    }
}

impl PaginateBinding<concord_core::advanced::CursorPagination<String>>
    for HeaderBoundCursorEndpoint
{
    fn load_pagination(&self) -> concord_core::advanced::CursorPagination<String> {
        concord_core::advanced::CursorPagination {
            cursor: self.cursor.clone(),
            per_page: self.count,
            send_cursor_on_first: self.send_cursor_on_first,
            stop_when_cursor_missing: self.stop_when_cursor_missing,
        }
    }

    fn store_pagination(&mut self, pagination: &concord_core::advanced::CursorPagination<String>) {
        self.cursor = pagination.cursor.clone();
        self.count = pagination.per_page;
    }
}

#[derive(Clone)]
struct QueryBoundPagedEndpoint {
    page: u64,
    count: u64,
}

impl Endpoint<TestCx> for QueryBoundPagedEndpoint {
    type Response = Vec<String>;

    fn execute<'a, T>(
        client: &'a concord_core::prelude::ApiClient<TestCx, T>,
        plan: concord_core::internal::RequestPlan,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Response, ApiClientError>> + Send + 'a>>
    where
        T: concord_core::advanced::Transport + 'a,
    {
        execute_buffered::<_, _, CommaSeparatedItems>(client, plan)
    }
}

impl ReusableEndpoint<TestCx> for QueryBoundPagedEndpoint {
    fn plan(
        &self,
        _ctx: &concord_core::internal::ClientPlanContext<'_, TestCx>,
    ) -> Result<concord_core::internal::RequestPlan, ApiClientError> {
        let mut plan = request_plan(
            "QueryBoundPaged",
            Method::GET,
            "/query-paged",
            Default::default(),
            Some(concord_core::internal::PaginationMarker),
        );
        plan.endpoint
            .policy
            .query
            .push(("pageNo".into(), self.page.to_string()));
        plan.endpoint
            .policy
            .query
            .push(("pageSize".into(), self.count.to_string()));
        Ok(plan)
    }
}

impl PaginatedEndpoint<TestCx> for QueryBoundPagedEndpoint {
    type Pagination = concord_core::advanced::PagedPagination;

    fn pagination_runtime(
        &self,
    ) -> Option<Box<dyn concord_core::advanced::PaginationRuntime<Self, Self::Response>>> {
        None
    }
}

impl PaginateBinding<concord_core::advanced::PagedPagination> for QueryBoundPagedEndpoint {
    fn load_pagination(&self) -> concord_core::advanced::PagedPagination {
        concord_core::advanced::PagedPagination {
            page: self.page,
            per_page: self.count,
        }
    }

    fn store_pagination(&mut self, pagination: &concord_core::advanced::PagedPagination) {
        self.page = pagination.page;
        self.count = pagination.per_page;
    }
}

#[derive(Clone)]
struct PaginationEndpoint {
    name: &'static str,
    path: &'static str,
    policy: concord_core::internal::ResolvedPolicy,
    pagination: Option<PaginationVariant>,
}

impl<Cx: concord_core::prelude::ClientContext> concord_core::prelude::Endpoint<Cx>
    for PaginationEndpoint
{
    type Response = Vec<String>;

    fn execute<'a, T>(
        client: &'a concord_core::prelude::ApiClient<Cx, T>,
        plan: concord_core::internal::RequestPlan,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Response, ApiClientError>> + Send + 'a>>
    where
        T: concord_core::advanced::Transport + 'a,
    {
        execute_buffered::<_, _, CommaSeparatedItems>(client, plan)
    }
}

impl<Cx: concord_core::prelude::ClientContext> ReusableEndpoint<Cx> for PaginationEndpoint {
    fn plan(
        &self,
        _ctx: &concord_core::internal::ClientPlanContext<'_, Cx>,
    ) -> Result<concord_core::internal::RequestPlan, ApiClientError> {
        Ok(request_plan(
            self.name,
            Method::GET,
            self.path,
            self.policy.clone(),
            self.pagination
                .as_ref()
                .map(|_| concord_core::internal::PaginationMarker),
        ))
    }
}

impl<Cx: concord_core::prelude::ClientContext> concord_core::prelude::PaginatedEndpoint<Cx>
    for PaginationEndpoint
{
    type Pagination = concord_core::advanced::OffsetLimitPagination;

    fn pagination_runtime(
        &self,
    ) -> Option<Box<dyn concord_core::advanced::PaginationRuntime<Self, Self::Response>>> {
        None
    }
}

impl concord_core::advanced::PaginateBinding<concord_core::advanced::OffsetLimitPagination>
    for PaginationEndpoint
{
    fn load_pagination(&self) -> concord_core::advanced::OffsetLimitPagination {
        concord_core::advanced::OffsetLimitPagination::default()
    }

    fn store_pagination(&mut self, _pagination: &concord_core::advanced::OffsetLimitPagination) {}
}

#[test]
fn custom_expected_items_per_page_zero_is_unrepresentable() {
    assert!(NonZeroUsize::new(0).is_none());
}

#[tokio::test]

async fn pagination_runtime_state_drives_endpoint_planning_order() -> Result<(), ApiClientError> {
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

    let load_calls = Arc::new(AtomicUsize::new(0));
    let store_calls = Arc::new(AtomicUsize::new(0));
    let endpoint = GeneratedHeaderBoundCustomEndpoint {
        page: 1,
        count: 2,
        load_calls: load_calls.clone(),
        store_calls: store_calls.clone(),
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
        requests[0]
            .headers
            .get("x-page")
            .and_then(|v| v.to_str().ok()),
        Some("1")
    );
    assert_eq!(
        requests[0]
            .headers
            .get("x-count")
            .and_then(|v| v.to_str().ok()),
        Some("2")
    );
    assert_eq!(
        requests[1]
            .headers
            .get("x-page")
            .and_then(|v| v.to_str().ok()),
        Some("2")
    );
    assert_eq!(
        requests[1]
            .headers
            .get("x-count")
            .and_then(|v| v.to_str().ok()),
        Some("2")
    );
    assert_eq!(load_calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(store_calls.load(std::sync::atomic::Ordering::SeqCst), 3);
    Ok(())
}

#[tokio::test]
async fn pagination_runtime_loads_and_stores_endpoint_state() -> Result<(), ApiClientError> {
    let ctx = concord_core::error::ErrorContext {
        endpoint: "HeaderBoundCustom",
        method: Method::GET,
    };
    let load_calls = Arc::new(AtomicUsize::new(0));
    let store_calls = Arc::new(AtomicUsize::new(0));
    let endpoint = GeneratedHeaderBoundCustomEndpoint {
        page: 1,
        count: 2,
        load_calls: load_calls.clone(),
        store_calls: store_calls.clone(),
    };
    let mut runtime = PaginationRuntimeAdapter::<HeaderBoundCustomPagination>::new();
    type Runtime = PaginationRuntimeAdapter<HeaderBoundCustomPagination>;
    <Runtime as PaginationRuntime<GeneratedHeaderBoundCustomEndpoint, Vec<String>>>::init(
        &mut runtime,
        &endpoint,
        PageApply {
            endpoint: "HeaderBoundCustom",
            page_index: 0,
            ctx: &ctx,
        },
    )?;
    assert_eq!(load_calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(
        <Runtime as PaginationRuntime<GeneratedHeaderBoundCustomEndpoint, Vec<String>>>::progress_key(
            &runtime
        ),
        Some(ProgressKey::U64(1))
    );

    let mut endpoint = endpoint;
    <Runtime as PaginationRuntime<GeneratedHeaderBoundCustomEndpoint, Vec<String>>>::apply(
        &mut runtime,
        &mut endpoint,
        PageApply {
            endpoint: "HeaderBoundCustom",
            page_index: 0,
            ctx: &ctx,
        },
    )?;
    assert_eq!(
        <Runtime as PaginationRuntime<GeneratedHeaderBoundCustomEndpoint, Vec<String>>>::expected_items_per_page(
            &runtime
        ),
        NonZeroUsize::new(2)
    );
    assert_eq!(endpoint.page, 1);
    assert_eq!(endpoint.count, 2);
    assert_eq!(store_calls.load(std::sync::atomic::Ordering::SeqCst), 1);

    let decision =
        <Runtime as PaginationRuntime<GeneratedHeaderBoundCustomEndpoint, Vec<String>>>::advance(
            &mut runtime,
            &mut endpoint,
            &ctx,
            &vec!["a".to_string()],
            PageAdvance {
                endpoint: "HeaderBoundCustom",
                page_index: 0,
                item_count: 1,
            },
        )?;
    assert_eq!(decision, PageDecision::Continue);
    assert_eq!(endpoint.page, 2);
    assert_eq!(
        <Runtime as PaginationRuntime<GeneratedHeaderBoundCustomEndpoint, Vec<String>>>::progress_key(
            &runtime
        ),
        Some(ProgressKey::U64(2))
    );
    assert_eq!(store_calls.load(std::sync::atomic::Ordering::SeqCst), 2);
    Ok(())
}

#[tokio::test]

async fn generated_custom_endpoint_state_collect_renders_endpoint_fields()
-> Result<(), ApiClientError> {
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

    let load_calls = Arc::new(AtomicUsize::new(0));
    let store_calls = Arc::new(AtomicUsize::new(0));
    let endpoint = GeneratedHeaderBoundCustomEndpoint {
        page: 1,
        count: 2,
        load_calls: load_calls.clone(),
        store_calls: store_calls.clone(),
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
        requests[0]
            .headers
            .get("x-page")
            .and_then(|v| v.to_str().ok()),
        Some("1")
    );
    assert_eq!(
        requests[0]
            .headers
            .get("x-count")
            .and_then(|v| v.to_str().ok()),
        Some("2")
    );
    assert_eq!(
        requests[1]
            .headers
            .get("x-page")
            .and_then(|v| v.to_str().ok()),
        Some("2")
    );
    assert_eq!(
        requests[1]
            .headers
            .get("x-count")
            .and_then(|v| v.to_str().ok()),
        Some("2")
    );
    assert_eq!(load_calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(store_calls.load(std::sync::atomic::Ordering::SeqCst), 3);
    Ok(())
}

#[tokio::test]

async fn offset_limit_pagination_runtime_advances_offset() -> Result<(), ApiClientError> {
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

    let endpoint = HeaderBoundOffsetLimitEndpoint { start: 0, count: 2 };

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
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0]
            .headers
            .get("x-start")
            .and_then(|v| v.to_str().ok()),
        Some("0")
    );
    assert_eq!(
        requests[0]
            .headers
            .get("x-count")
            .and_then(|v| v.to_str().ok()),
        Some("2")
    );
    assert_eq!(
        requests[1]
            .headers
            .get("x-start")
            .and_then(|v| v.to_str().ok()),
        Some("2")
    );
    assert_eq!(
        requests[1]
            .headers
            .get("x-count")
            .and_then(|v| v.to_str().ok()),
        Some("2")
    );
    assert_eq!(query_value(&requests[0].url, "offset"), None);
    assert_eq!(query_value(&requests[1].url, "offset"), None);
    Ok(())
}

#[tokio::test]

async fn paged_pagination_runtime_advances_page() -> Result<(), ApiClientError> {
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

    let endpoint = HeaderBoundPagedEndpoint { page: 1, count: 2 };

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
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0]
            .headers
            .get("x-page")
            .and_then(|v| v.to_str().ok()),
        Some("1")
    );
    assert_eq!(
        requests[0]
            .headers
            .get("x-count")
            .and_then(|v| v.to_str().ok()),
        Some("2")
    );
    assert_eq!(
        requests[1]
            .headers
            .get("x-page")
            .and_then(|v| v.to_str().ok()),
        Some("2")
    );
    assert_eq!(
        requests[1]
            .headers
            .get("x-count")
            .and_then(|v| v.to_str().ok()),
        Some("2")
    );
    assert_eq!(query_value(&requests[0].url, "pageNo"), None);
    assert_eq!(query_value(&requests[1].url, "pageNo"), None);
    Ok(())
}

#[tokio::test]
async fn execute_raw_paginated_endpoint_sends_only_one_request() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "raw-page-1"),
            MockResponse::text(StatusCode::OK, "should-not-send"),
        ],
    );
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let endpoint = ItemsEndpoint {
        policy: Default::default(),
        start: 7,
        count: 11,
        pagination: PaginationVariant::OffsetLimit {
            offset: 7,
            limit: 11,
        },
        ..Default::default()
    };

    let raw = client.request(endpoint).execute_raw().await?;

    assert_eq!(raw.body, Bytes::from_static(b"raw-page-1"));
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].meta.page_index, 0);
    assert_eq!(
        query_value(&requests[0].url, "offset"),
        Some("7".to_string())
    );
    assert_eq!(
        query_value(&requests[0].url, "limit"),
        Some("11".to_string())
    );
    Ok(())
}

#[tokio::test]

async fn pagination_runtime_requires_runtime_support_when_missing() -> Result<(), ApiClientError> {
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

    let endpoint = QueryBoundPagedEndpoint { page: 1, count: 2 };

    let err = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(4))
        .collect()
        .await
        .expect_err("built-in pagination without runtime support must be rejected");
    assert!(matches!(err, ApiClientError::Pagination { .. }));
    assert!(sent.requests().await.is_empty());
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
        pagination: PaginationVariant::OffsetLimit {
            offset: 0,
            limit: 2,
        },
        ..Default::default()
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
        pagination: PaginationVariant::OffsetLimit {
            offset: 0,
            limit: 2,
        },
        ..Default::default()
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

async fn cursor_pagination_runtime_omits_initial_cursor_when_send_cursor_on_first_false()
-> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "a,b|next=next-1"),
            MockResponse::text(StatusCode::OK, "c,d|next=next-2"),
        ],
    );
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let endpoint = HeaderBoundCursorEndpoint {
        cursor: Some("start".to_string()),
        count: 2,
        send_cursor_on_first: false,
        stop_when_cursor_missing: true,
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
            "d".to_string(),
        ]
    );
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 2);
    assert!(query_value(&requests[0].url, "cursor").is_none());
    assert_eq!(
        header_value(&requests[0].headers, "x-count"),
        Some("2".to_string())
    );
    assert_eq!(
        header_value(&requests[1].headers, "x-cursor"),
        Some("next-1".to_string())
    );
    assert_eq!(
        header_value(&requests[1].headers, "x-count"),
        Some("2".to_string())
    );
    Ok(())
}

#[tokio::test]

async fn cursor_pagination_runtime_sends_initial_cursor_when_send_cursor_on_first_true()
-> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "a,b|next=next-1"),
            MockResponse::text(StatusCode::OK, "c,d|next=next-2"),
        ],
    );
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let endpoint = HeaderBoundCursorEndpoint {
        cursor: Some("start".to_string()),
        count: 2,
        send_cursor_on_first: true,
        stop_when_cursor_missing: true,
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
            "d".to_string(),
        ]
    );
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 2);
    assert_eq!(
        header_value(&requests[0].headers, "x-cursor"),
        Some("start".to_string())
    );
    assert_eq!(
        header_value(&requests[0].headers, "x-count"),
        Some("2".to_string())
    );
    assert_eq!(
        header_value(&requests[1].headers, "x-cursor"),
        Some("next-1".to_string())
    );
    assert_eq!(
        header_value(&requests[1].headers, "x-count"),
        Some("2".to_string())
    );
    Ok(())
}

#[tokio::test]

async fn cursor_string_pagination_runtime_advances_cursor() -> Result<(), ApiClientError> {
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

    let endpoint = HeaderBoundCursorEndpoint {
        cursor: Some("start".to_string()),
        count: 2,
        send_cursor_on_first: false,
        stop_when_cursor_missing: true,
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
    assert!(query_value(&requests[0].url, "cursor").is_none());
    assert_eq!(
        header_value(&requests[0].headers, "x-count"),
        Some("2".to_string())
    );
    assert_eq!(
        header_value(&requests[1].headers, "x-cursor"),
        Some("next-1".to_string())
    );
    assert_eq!(
        header_value(&requests[1].headers, "x-count"),
        Some("2".to_string())
    );
    Ok(())
}

#[tokio::test]

async fn cursor_string_pagination_runtime_preserves_empty_cursor() -> Result<(), ApiClientError> {
    let ctx = concord_core::error::ErrorContext {
        endpoint: "CursorItemsEndpoint",
        method: Method::GET,
    };
    let mut endpoint = CursorItemsEndpoint {
        cursor: Some("start".to_string()),
        count: 2,
        policy: Default::default(),
        pagination: PaginationVariant::cursor::<CursorItems>(CursorPagination {
            cursor: Some("start".to_string()),
            per_page: 2,
            send_cursor_on_first: true,
            stop_when_cursor_missing: true,
        }),
    };
    let mut runtime = PaginationRuntimeAdapter::<CursorPagination<String>>::new();
    type Runtime = PaginationRuntimeAdapter<CursorPagination<String>>;
    <Runtime as PaginationRuntime<CursorItemsEndpoint, CursorItems>>::init(
        &mut runtime,
        &endpoint,
        PageApply {
            endpoint: "CursorItemsEndpoint",
            page_index: 0,
            ctx: &ctx,
        },
    )?;
    let _ = <Runtime as PaginationRuntime<CursorItemsEndpoint, CursorItems>>::apply(
        &mut runtime,
        &mut endpoint,
        PageApply {
            endpoint: "CursorItemsEndpoint",
            page_index: 0,
            ctx: &ctx,
        },
    )?;
    let page = CursorItems {
        items: vec!["a".to_string()],
        next: Some(String::new()),
    };
    let decision = <Runtime as PaginationRuntime<CursorItemsEndpoint, CursorItems>>::advance(
        &mut runtime,
        &mut endpoint,
        &ctx,
        &page,
        PageAdvance {
            endpoint: "CursorItemsEndpoint",
            page_index: 0,
            item_count: 1,
        },
    )?;
    assert_eq!(decision, PageDecision::Continue);
    assert_eq!(endpoint.cursor, Some(String::new()));
    assert_eq!(
        <Runtime as PaginationRuntime<CursorItemsEndpoint, CursorItems>>::progress_key(&runtime),
        Some(ProgressKey::Str(String::new()))
    );
    Ok(())
}

#[tokio::test]

async fn cursor_string_pagination_runtime_requires_runtime_support_when_missing()
-> Result<(), ApiClientError> {
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

    let endpoint = ItemsEndpoint {
        start: 0,
        count: 2,
        policy: Default::default(),
        pagination: PaginationVariant::Cursor {
            cursor: Some("start".to_string()),
            per_page: 2,
            send_cursor_on_first: true,
            stop_when_cursor_missing: true,
        },
    };

    let err = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(100))
        .collect()
        .await
        .expect_err("built-in pagination without runtime support must be rejected");
    assert!(matches!(err, ApiClientError::Pagination { .. }));
    assert_eq!(
        err.pagination_error_kind(),
        Some(concord_core::error::PaginationErrorKind::UnsupportedPagination)
    );
    assert!(sent.requests().await.is_empty());
    Ok(())
}

#[tokio::test]

async fn cursor_pagination_repeated_cursor_returns_non_progress_error() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "a,b|next=next-1"),
            MockResponse::text(StatusCode::OK, "c,d|next=next-1"),
        ],
    );
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let endpoint = CursorItemsEndpoint {
        policy: Default::default(),
        cursor: Some("start".to_string()),
        count: 2,
        pagination: PaginationVariant::cursor::<CursorItems>(CursorPagination {
            cursor: Some("start".to_string()),
            per_page: 2,
            send_cursor_on_first: true,
            stop_when_cursor_missing: true,
        }),
        ..Default::default()
    };

    let err = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(100))
        .detect_loops(false)
        .collect()
        .await
        .expect_err("repeated cursor should stop with a typed pagination error");

    assert!(matches!(err, ApiClientError::Pagination { .. }));
    assert_eq!(
        err.pagination_error_kind(),
        Some(concord_core::error::PaginationErrorKind::NonProgress)
    );
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
            MockResponse::text(StatusCode::OK, "c,d|next=start-a"),
        ],
    );
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let endpoint = CursorItemsEndpoint {
        policy: Default::default(),
        cursor: Some("start-a".to_string()),
        count: 2,
        pagination: PaginationVariant::cursor::<CursorItems>(CursorPagination {
            cursor: Some("start-a".to_string()),
            per_page: 2,
            send_cursor_on_first: true,
            stop_when_cursor_missing: true,
        }),
        ..Default::default()
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
    let transport = MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, "a,b")]);
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let endpoint = CursorItemsEndpoint {
        policy: Default::default(),
        cursor: None,
        count: 2,
        pagination: PaginationVariant::cursor::<CursorItems>(CursorPagination {
            cursor: None,
            per_page: 2,
            send_cursor_on_first: false,
            stop_when_cursor_missing: false,
        }),
        ..Default::default()
    };

    let err = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(100))
        .detect_loops(false)
        .collect()
        .await
        .expect_err("missing cursor without stop should not silently terminate");

    assert!(matches!(err, ApiClientError::Pagination { .. }));
    assert!(err.to_string().contains("non-progress"));
    assert_eq!(sent.sent_count().await, 1);
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
        page: 1,
        count: 2,
        pagination: PaginationVariant::Paged {
            page: 1,
            per_page: 2,
        },
        ..Default::default()
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
        start: 1,
        count: 2,
        pagination: PaginationVariant::Paged {
            page: 1,
            per_page: 2,
        },
        ..Default::default()
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
        start: 0,
        count: 20,
        pagination: PaginationVariant::OffsetLimit {
            offset: 0,
            limit: 2,
        },
        ..Default::default()
    };

    let err = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(0))
        .collect()
        .await
        .expect_err("zero hard page cap should fail before transport");

    assert!(matches!(err, ApiClientError::Pagination { .. }));
    assert_eq!(
        err.pagination_error_kind(),
        Some(concord_core::error::PaginationErrorKind::InvalidSize)
    );
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
        start: 0,
        count: 20,
        pagination: PaginationVariant::OffsetLimit {
            offset: 0,
            limit: 2,
        },
        ..Default::default()
    };

    let err = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_item_cap(0))
        .collect()
        .await
        .expect_err("zero hard item cap should fail before transport");

    assert!(matches!(err, ApiClientError::Pagination { .. }));
    assert_eq!(
        err.pagination_error_kind(),
        Some(concord_core::error::PaginationErrorKind::InvalidSize)
    );
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
        start: 0,
        count: 2,
        pagination: PaginationVariant::OffsetLimit {
            offset: 0,
            limit: 2,
        },
        ..Default::default()
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
        start: 0,
        count: 2,
        pagination: PaginationVariant::OffsetLimit {
            offset: 0,
            limit: 2,
        },
        ..Default::default()
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
        start: 0,
        count: 20,
        pagination: PaginationVariant::OffsetLimit {
            offset: 0,
            limit: 20,
        },
        ..Default::default()
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
        start: 0,
        count: 20,
        pagination: PaginationVariant::OffsetLimit {
            offset: 0,
            limit: 20,
        },
        ..Default::default()
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
        start: 0,
        count: 2,
        pagination: PaginationVariant::OffsetLimit {
            offset: 0,
            limit: 20,
        },
        ..Default::default()
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

#[test]
fn paged_pagination_runtime_rejects_zero_page() {
    let mut pagination = PagedPagination {
        page: 0,
        per_page: 20,
    };
    let ctx = concord_core::error::ErrorContext {
        endpoint: "PagedPagination",
        method: Method::GET,
    };

    let err = <PagedPagination as EndpointPagination<Vec<String>>>::apply(
        &mut pagination,
        PageApply {
            endpoint: "PagedPagination",
            page_index: 0,
            ctx: &ctx,
        },
    )
    .expect_err("page zero must be rejected");

    assert!(matches!(err, ApiClientError::Pagination { .. }));
    assert_eq!(
        err.pagination_error_kind(),
        Some(concord_core::error::PaginationErrorKind::InvalidSize)
    );
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
        start: 0,
        count: 2,
        pagination: PaginationVariant::OffsetLimit {
            offset: 0,
            limit: 2,
        },
        ..Default::default()
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
        start: 0,
        count: 2,
        pagination: PaginationVariant::OffsetLimit {
            offset: 0,
            limit: 2,
        },
        ..Default::default()
    };

    let err = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(2))
        .collect()
        .await
        .expect_err("hard page cap should fail before fetching page 3");

    assert!(matches!(err, ApiClientError::PaginationLimit { .. }));
    assert_eq!(
        err.pagination_error_kind(),
        Some(concord_core::error::PaginationErrorKind::PageLimitExceeded)
    );
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
        pagination: PaginationVariant::OffsetLimit {
            offset: 0,
            limit: 20,
        },
        ..Default::default()
    };

    let err = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_item_cap(30))
        .collect()
        .await
        .expect_err("hard item cap should fail rather than truncate");

    assert!(matches!(err, ApiClientError::PaginationLimit { .. }));
    assert_eq!(
        err.pagination_error_kind(),
        Some(concord_core::error::PaginationErrorKind::ItemLimitExceeded)
    );
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
            MockResponse::text(StatusCode::OK, "c,d|next=next-1"),
        ],
    );
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let endpoint = CursorItemsEndpoint {
        policy: Default::default(),
        cursor: Some("start".to_string()),
        count: 2,
        pagination: PaginationVariant::cursor::<CursorItems>(CursorPagination {
            cursor: Some("start".to_string()),
            per_page: 2,
            send_cursor_on_first: true,
            stop_when_cursor_missing: true,
        }),
        ..Default::default()
    };

    let err = client
        .request(endpoint)
        .paginate(PaginationTermination::take_pages(10))
        .collect()
        .await
        .expect_err("loop detection should run before soft page termination");

    assert!(matches!(err, ApiClientError::Pagination { .. }));
    assert_eq!(
        err.pagination_error_kind(),
        Some(concord_core::error::PaginationErrorKind::NonProgress)
    );
    assert!(err.to_string().contains("loop detected"));
    assert_eq!(sent.sent_count().await, 2);
}

fn pagination_sentinels() -> RedactionSentinels {
    RedactionSentinels::new(
        "PAGINATION_AUTH_SENTINEL_PR16",
        "PAGINATION_BODY_SENTINEL_PR16",
        "PAGINATION_RESPONSE_SENTINEL_PR16",
    )
}

fn policy_with_request_sentinel(sentinel: &'static str) -> ResolvedPolicy {
    let mut policy = ResolvedPolicy::default();
    policy.headers.insert(
        HeaderName::from_static("x-pagination-sentinel"),
        HeaderValue::from_static(sentinel),
    );
    policy
}

#[tokio::test]
async fn later_page_http_status_failure_is_typed_and_redacted() -> Result<(), ApiClientError> {
    let sentinels = pagination_sentinels();
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "a,b"),
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, sentinels.response),
        ],
    );
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let endpoint = ItemsEndpoint {
        policy: policy_with_request_sentinel(sentinels.auth),
        start: 0,
        count: 2,
        pagination: PaginationVariant::OffsetLimit {
            offset: 0,
            limit: 2,
        },
        ..Default::default()
    };

    let err = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(100))
        .collect()
        .await
        .expect_err("later HTTP status should surface as a typed pagination response error");

    assert!(matches!(err, ApiClientError::HttpStatus { .. }));
    assert_eq!(err.category(), ErrorCategory::HttpStatus);
    assert_eq!(err.http_status(), Some(StatusCode::INTERNAL_SERVER_ERROR));
    assert_eq!(err.context().endpoint, "Items");
    assert_eq!(err.context().method, Method::GET);
    assert_eq!(sent.sent_count().await, 2);
    let sentinel_values = [sentinels.auth, sentinels.response];
    assert_error_chain_does_not_contain_any(&err, &sentinel_values);
    Ok(())
}

#[tokio::test]
async fn later_page_transport_failure_is_typed_and_redacted() -> Result<(), ApiClientError> {
    let sentinels = pagination_sentinels();
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::with_outcomes(
        events,
        vec![
            MockOutcome::Response(MockResponse::text(
                StatusCode::OK,
                format!("{},{}", sentinels.response, sentinels.body),
            )),
            MockOutcome::TransportError(TransportErrorKind::Connect),
        ],
    );
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let endpoint = ItemsEndpoint {
        policy: policy_with_request_sentinel(sentinels.auth),
        start: 0,
        count: 2,
        pagination: PaginationVariant::OffsetLimit {
            offset: 0,
            limit: 2,
        },
        ..Default::default()
    };

    let err = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(100))
        .collect()
        .await
        .expect_err("later transport failure should surface as a typed pagination transport error");

    assert!(matches!(err, ApiClientError::Transport { .. }));
    assert_eq!(err.category(), ErrorCategory::Transport);
    assert_eq!(err.context().endpoint, "Items");
    assert_eq!(err.context().method, Method::GET);
    assert_eq!(sent.sent_count().await, 2);
    let sentinel_values = [sentinels.auth];
    assert_error_chain_does_not_contain_any(&err, &sentinel_values);
    Ok(())
}

#[tokio::test]
async fn later_page_decode_failure_is_typed_and_redacted() -> Result<(), ApiClientError> {
    let sentinels = pagination_sentinels();
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "a,b"),
            MockResponse::text(StatusCode::OK, Bytes::from_static(b"\xff\xfe")),
        ],
    );
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let endpoint = ItemsEndpoint {
        policy: policy_with_request_sentinel(sentinels.auth),
        start: 0,
        count: 2,
        pagination: PaginationVariant::OffsetLimit {
            offset: 0,
            limit: 2,
        },
        ..Default::default()
    };

    let err = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(100))
        .collect()
        .await
        .expect_err("later decode failure should surface as a typed pagination decode error");

    assert!(matches!(err, ApiClientError::Decode { .. }));
    assert_eq!(err.category(), ErrorCategory::Decode);
    assert_eq!(err.context().endpoint, "Items");
    assert_eq!(err.context().method, Method::GET);
    assert_eq!(sent.sent_count().await, 2);
    let sentinel_values = [sentinels.auth];
    assert_error_chain_does_not_contain_any(&err, &sentinel_values);
    Ok(())
}

#[tokio::test]

async fn max_items_error_includes_page_context() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, "a,b,c")]);
    let client = client(TestAuthVars::default(), transport);

    let endpoint = ItemsEndpoint {
        policy: Default::default(),
        pagination: PaginationVariant::OffsetLimit {
            offset: 0,
            limit: 3,
        },
        ..Default::default()
    };

    let err = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_item_cap(2))
        .collect()
        .await
        .expect_err("hard item cap should stop pagination");

    let msg = err.to_string();
    assert_eq!(
        err.pagination_error_kind(),
        Some(concord_core::error::PaginationErrorKind::ItemLimitExceeded)
    );
    assert!(msg.contains("hard item cap"));
    assert!(msg.contains("Items"));
    assert!(msg.contains("page_index=0"));
}

#[tokio::test]

async fn collect_offset_short_first_page_stops_via_runtime() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, "a,b,c")]);
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let endpoint = ItemsEndpoint {
        policy: Default::default(),
        start: 0,
        count: 100,
        pagination: PaginationVariant::OffsetLimit {
            offset: 0,
            limit: 100,
        },
        ..Default::default()
    };

    let items = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(100))
        .collect()
        .await?;

    assert_eq!(items, vec!["a", "b", "c"]);
    assert_eq!(sent.sent_count().await, 1);
    Ok(())
}

#[tokio::test]

async fn collect_offset_empty_first_page_stops_via_runtime() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, "")]);
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let endpoint = ItemsEndpoint {
        policy: Default::default(),
        start: 0,
        count: 100,
        pagination: PaginationVariant::OffsetLimit {
            offset: 0,
            limit: 100,
        },
        ..Default::default()
    };

    let items = client
        .request(endpoint)
        .paginate(PaginationTermination::take_pages(100))
        .collect()
        .await?;

    assert!(items.is_empty());
    assert_eq!(sent.sent_count().await, 1);
    Ok(())
}

#[tokio::test]

async fn collect_paged_short_page_stops_via_runtime() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, "a,b")]);
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let endpoint = PageOnlyItemsEndpoint {
        policy: Default::default(),
        page: 1,
        count: 100,
        pagination: PaginationVariant::Paged {
            page: 1,
            per_page: 100,
        },
        ..Default::default()
    };

    let items = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(100))
        .collect()
        .await?;

    assert_eq!(items, vec!["a", "b"]);
    assert_eq!(sent.sent_count().await, 1);
    Ok(())
}

#[tokio::test]

async fn collect_cursor_short_page_stops_even_with_next_cursor() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![MockResponse::text(StatusCode::OK, "a,b|next=next-page")],
    );
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let endpoint = CursorItemsEndpoint {
        policy: Default::default(),
        cursor: Some("start".to_string()),
        count: 100,
        pagination: PaginationVariant::cursor::<CursorItems>(CursorPagination {
            cursor: Some("start".to_string()),
            per_page: 100,
            send_cursor_on_first: true,
            stop_when_cursor_missing: true,
        }),
        ..Default::default()
    };

    let items = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(100))
        .collect()
        .await?;

    assert_eq!(items, vec!["a", "b"]);
    assert_eq!(sent.sent_count().await, 1);
    Ok(())
}

#[tokio::test]

async fn collect_cursor_empty_page_stops_even_with_next_cursor() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![MockResponse::text(StatusCode::OK, "|next=next-page")],
    );
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let endpoint = CursorItemsEndpoint {
        policy: Default::default(),
        cursor: Some("start".to_string()),
        count: 100,
        pagination: PaginationVariant::cursor::<CursorItems>(CursorPagination {
            cursor: Some("start".to_string()),
            per_page: 100,
            send_cursor_on_first: true,
            stop_when_cursor_missing: true,
        }),
        ..Default::default()
    };

    let items = client
        .request(endpoint)
        .paginate(PaginationTermination::take_pages(100))
        .collect()
        .await?;

    assert!(items.is_empty());
    assert_eq!(sent.sent_count().await, 1);
    Ok(())
}

#[tokio::test]

async fn collect_exact_count_empty_page_stops_after_consumption() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, "")]);
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let endpoint = PageOnlyItemsEndpoint {
        policy: Default::default(),
        pagination: PaginationVariant::OffsetLimit {
            offset: 0,
            limit: 2,
        },
        ..Default::default()
    };

    let items = client
        .request(endpoint)
        .paginate(PaginationTermination::take_pages(100))
        .collect()
        .await?;

    assert!(items.is_empty());
    assert_eq!(sent.sent_count().await, 1);
    Ok(())
}

#[tokio::test]

async fn take_items_short_page_returns_short_page_under_limit() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, "a,b,c")]);
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let endpoint = ItemsEndpoint {
        policy: Default::default(),
        start: 0,
        count: 100,
        pagination: PaginationVariant::OffsetLimit {
            offset: 0,
            limit: 100,
        },
        ..Default::default()
    };

    let items = client
        .request(endpoint)
        .paginate(PaginationTermination::take_items(500))
        .collect()
        .await?;

    assert_eq!(items, vec!["a", "b", "c"]);
    assert_eq!(sent.sent_count().await, 1);
    Ok(())
}

#[tokio::test]

async fn hard_item_cap_short_page_success_under_cap() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, "a,b,c")]);
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let endpoint = ItemsEndpoint {
        policy: Default::default(),
        start: 0,
        count: 100,
        pagination: PaginationVariant::OffsetLimit {
            offset: 0,
            limit: 100,
        },
        ..Default::default()
    };

    let items = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_item_cap(10_000))
        .collect()
        .await?;

    assert_eq!(items, vec!["a", "b", "c"]);
    assert_eq!(sent.sent_count().await, 1);
    Ok(())
}

#[tokio::test]

async fn hard_item_cap_still_errors_before_short_stop_when_page_exceeds_cap() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, numbered_items(0, 90)),
            MockResponse::text(StatusCode::OK, numbered_items(90, 48)),
        ],
    );
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let endpoint = ItemsEndpoint {
        policy: Default::default(),
        pagination: PaginationVariant::OffsetLimit {
            offset: 0,
            limit: 90,
        },
        ..Default::default()
    };

    let err = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_item_cap(100))
        .collect()
        .await
        .expect_err("hard item cap must not be hidden by short-page stop");

    assert!(matches!(err, ApiClientError::PaginationLimit { .. }));
    assert_eq!(
        err.pagination_error_kind(),
        Some(concord_core::error::PaginationErrorKind::ItemLimitExceeded)
    );
    assert_eq!(sent.sent_count().await, 2);
}

#[tokio::test]

async fn take_items_exact_limit_still_wins_before_short_stop() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, "a,b,c")]);
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let endpoint = ItemsEndpoint {
        policy: Default::default(),
        pagination: PaginationVariant::OffsetLimit {
            offset: 0,
            limit: 100,
        },
        ..Default::default()
    };

    let items = client
        .request(endpoint)
        .paginate(PaginationTermination::take_items(2))
        .collect()
        .await?;

    assert_eq!(items, vec!["a", "b"]);
    assert_eq!(sent.sent_count().await, 1);
    Ok(())
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
        pagination: PaginationVariant::OffsetLimit {
            offset: 0,
            limit: 2,
        },
        ..Default::default()
    };

    let err = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(1))
        .collect()
        .await
        .expect_err("hard page cap should stop pagination");

    let msg = err.to_string();
    assert_eq!(
        err.pagination_error_kind(),
        Some(concord_core::error::PaginationErrorKind::PageLimitExceeded)
    );
    assert!(msg.contains("hard page cap"));
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
        pagination: PaginationVariant::OffsetLimit {
            offset: 0,
            limit: 2,
        },
        ..Default::default()
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

async fn execute_raw_static_auth_collision_validates_before_rate_limit_and_transport() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let rate_limiter = Arc::new(RecordingRateLimiter::new(events.clone()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "should-not-send")],
    );
    let sent = transport.clone();
    let mut client = client(
        TestAuthVars {
            token: Some("RAW_EXECUTE_RAW_QUERY_SENTINEL".to_string()),
            identity: "query",
        },
        transport,
    );
    configure_runtime(&mut client, Some(rate_limiter));
    let mut policy = auth_policy(AuthPlacement::Query("api_key"));
    policy
        .query
        .push(("api_key".to_string(), "public-value".to_string()));
    let endpoint = PaginationEndpoint {
        name: "Items",
        path: "/items",
        policy,
        pagination: None,
    };

    let err = client
        .request(endpoint)
        .execute_raw()
        .await
        .expect_err("execute_raw should still validate auth collisions before transport");

    assert!(matches!(err, ApiClientError::Auth { .. }));
    assert_eq!(sent.sent_count().await, 0);
    let events = events.lock().await.clone();
    assert!(!events.iter().any(|event| event == "rate_acquire"));
    assert!(!events.iter().any(|event| event == "transport"));
}

fn query_value(url: &url::Url, name: &str) -> Option<String> {
    url.query_pairs()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
}

fn header_value(headers: &http::HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string())
}

fn numbered_items(start: usize, count: usize) -> String {
    (start..start + count)
        .map(|idx| format!("item-{idx}"))
        .collect::<Vec<_>>()
        .join(",")
}

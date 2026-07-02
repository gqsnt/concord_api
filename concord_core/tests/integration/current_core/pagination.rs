use super::common::*;
use concord_core::advanced::{
    AuthPlacement, EndpointPagination, PageAdvance, PageApply, PageApplyResult, PageDecision,
    PaginateBinding, ProgressKey, RateLimitContext, RateLimitFuture, RateLimitPermit,
    RateLimitResponseAction, RateLimitResponseContext, RateLimiter, SingleObjectPaginationRuntime,
    SingleObjectPaginationRuntimeAdapter,
};
use concord_core::prelude::{
    ApiClientError, CursorPagination, Endpoint, PageItems, PagedPagination, PaginatedEndpoint,
    PaginationTermination,
};
use http::{HeaderValue, Method, StatusCode};
use std::num::NonZeroUsize;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::Mutex;

tokio::task_local! {
    static PR64_CUSTOM_PAGINATION_EVENTS: Arc<StdMutex<Vec<&'static str>>>;
}

tokio::task_local! {
    static PR69_MUTATION_EVENTS: Arc<StdMutex<Vec<&'static str>>>;
}

#[derive(Default)]
struct HeaderTokenPagination;

#[derive(Default)]
struct HeaderTokenState {
    token: u64,
}

#[derive(Default)]
struct InvalidHeaderPagination;

#[derive(Default)]
struct InvalidHeaderValuePagination;

#[derive(Default)]
struct DynamicRequestMutationPagination;

struct DynamicRequestMutationState {
    query_key: String,
    header_name: String,
}

#[derive(Clone)]
struct HeaderBoundCustomEndpoint {
    page: u64,
    count: u64,
    pagination: Option<PaginationVariant>,
}

impl Endpoint<TestCx> for HeaderBoundCustomEndpoint {
    type Response = Vec<String>;

    fn plan(
        &self,
        _ctx: &concord_core::internal::ClientPlanContext<'_, TestCx>,
    ) -> Result<concord_core::internal::RequestPlan, ApiClientError> {
        let mut plan = request_plan(
            "HeaderBoundCustom",
            Method::GET,
            "/header-custom",
            Default::default(),
            Some(concord_core::internal::PaginationMarker),
            decode_items,
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

#[derive(Clone)]
struct GeneratedHeaderBoundCustomEndpoint {
    page: u64,
    count: u64,
    load_calls: Arc<AtomicUsize>,
    store_calls: Arc<AtomicUsize>,
}

impl Endpoint<TestCx> for GeneratedHeaderBoundCustomEndpoint {
    type Response = Vec<String>;

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
            decode_items,
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
    fn single_object_pagination(
        &self,
    ) -> Option<Box<dyn concord_core::advanced::SingleObjectPaginationRuntime<Self, Self::Response>>>
    {
        Some(Box::new(SingleObjectPaginationRuntimeAdapter::<
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
    fn apply(&mut self, _ctx: PageApply<'_>) -> Result<PageApplyResult, ApiClientError> {
        if self.count == 0 {
            return Err(ApiClientError::Pagination {
                ctx: concord_core::advanced::ErrorContext {
                    endpoint: "GeneratedHeaderBoundCustom",
                    method: Method::GET,
                },
                msg: "custom pagination page size must be non-zero".into(),
            });
        }
        Ok(PageApplyResult {
            expected_items_per_page: NonZeroUsize::new(self.count as usize),
        })
    }

    fn advance(
        &mut self,
        page: &Vec<String>,
        _ctx: PageAdvance<'_>,
    ) -> Result<PageDecision, ApiClientError> {
        if page.is_empty() {
            return Ok(PageDecision::Stop);
        }
        self.page = self
            .page
            .checked_add(1)
            .ok_or_else(|| ApiClientError::Pagination {
                ctx: concord_core::advanced::ErrorContext {
                    endpoint: "GeneratedHeaderBoundCustom",
                    method: Method::GET,
                },
                msg: "custom pagination page overflow".into(),
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

impl PaginatedEndpoint<TestCx> for HeaderBoundCustomEndpoint {
    fn single_object_pagination(
        &self,
    ) -> Option<Box<dyn concord_core::advanced::SingleObjectPaginationRuntime<Self, Self::Response>>>
    where
        Self: Sized,
        Self::Response: PageItems,
    {
        Some(Box::new(SingleObjectPaginationRuntimeAdapter::<
            HeaderBoundCustomPagination,
        >::new()))
    }
}

impl PaginateBinding<HeaderBoundCustomPagination> for HeaderBoundCustomEndpoint {
    fn load_pagination(&self) -> HeaderBoundCustomPagination {
        HeaderBoundCustomPagination {
            page: self.page,
            count: self.count,
        }
    }

    fn store_pagination(&mut self, pagination: &HeaderBoundCustomPagination) {
        self.page = pagination.page;
        self.count = pagination.count;
    }
}

#[derive(Clone)]
struct HeaderBoundOffsetLimitEndpoint {
    start: u64,
    count: u64,
    pagination: PaginationVariant,
}

impl Endpoint<TestCx> for HeaderBoundOffsetLimitEndpoint {
    type Response = Vec<String>;

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
            decode_items,
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
    fn single_object_pagination(
        &self,
    ) -> Option<Box<dyn concord_core::advanced::SingleObjectPaginationRuntime<Self, Self::Response>>>
    where
        Self: Sized,
        Self::Response: PageItems,
    {
        Some(Box::new(SingleObjectPaginationRuntimeAdapter::<
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
    pagination: PaginationVariant,
}

impl Endpoint<TestCx> for HeaderBoundPagedEndpoint {
    type Response = Vec<String>;

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
            decode_items,
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
    fn single_object_pagination(
        &self,
    ) -> Option<Box<dyn concord_core::advanced::SingleObjectPaginationRuntime<Self, Self::Response>>>
    where
        Self: Sized,
        Self::Response: PageItems,
    {
        Some(Box::new(SingleObjectPaginationRuntimeAdapter::<
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
    pagination: PaginationVariant,
}

impl Endpoint<TestCx> for HeaderBoundCursorEndpoint {
    type Response = CursorItems;

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
            decode_cursor_items,
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
    fn single_object_pagination(
        &self,
    ) -> Option<Box<dyn concord_core::advanced::SingleObjectPaginationRuntime<Self, Self::Response>>>
    where
        Self: Sized,
        Self::Response: PageItems,
    {
        Some(Box::new(SingleObjectPaginationRuntimeAdapter::<
            concord_core::advanced::CursorPagination<String>,
        >::new()))
    }
}

impl PaginateBinding<concord_core::advanced::CursorPagination<String>>
    for HeaderBoundCursorEndpoint
{
    fn load_pagination(&self) -> concord_core::advanced::CursorPagination<String> {
        let (send_cursor_on_first, stop_when_cursor_missing) = match &self.pagination {
            PaginationVariant::Cursor {
                send_cursor_on_first,
                stop_when_cursor_missing,
                ..
            } => (*send_cursor_on_first, *stop_when_cursor_missing),
            _ => (false, true),
        };
        concord_core::advanced::CursorPagination {
            cursor: self.cursor.clone(),
            per_page: self.count,
            send_cursor_on_first,
            stop_when_cursor_missing,
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
    pagination: PaginationVariant,
}

impl Endpoint<TestCx> for QueryBoundPagedEndpoint {
    type Response = Vec<String>;

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
            decode_items,
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

impl PaginatedEndpoint<TestCx> for QueryBoundPagedEndpoint {}

#[derive(Default)]
struct AuthQueryCollisionPagination;

#[derive(Default)]
struct TracedAuthQueryCollisionPagination;

#[derive(Default)]
struct AuthHeaderCollisionPagination;

#[derive(Default)]
struct AuthorizationCollisionPagination;

#[derive(Default)]
struct PublicMutationPagination;

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
            decode_items,
        ))
    }
}

impl<Cx: concord_core::prelude::ClientContext> concord_core::prelude::PaginatedEndpoint<Cx>
    for PaginationEndpoint
{
}

#[derive(Default)]
struct RecordingSanitizedUrlRateLimiter {
    acquires: Arc<Mutex<Vec<String>>>,
}

impl RecordingSanitizedUrlRateLimiter {
    fn new(acquires: Arc<Mutex<Vec<String>>>) -> Self {
        Self { acquires }
    }
}

impl RateLimiter for RecordingSanitizedUrlRateLimiter {
    fn acquire<'a>(
        &'a self,
        ctx: RateLimitContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitPermit, ApiClientError>> {
        let acquires = self.acquires.clone();
        Box::pin(async move {
            acquires.lock().await.push(ctx.url.to_string());
            Ok(RateLimitPermit)
        })
    }

    fn on_response<'a>(
        &'a self,
        _ctx: RateLimitResponseContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitResponseAction, ApiClientError>> {
        Box::pin(async move { Ok(RateLimitResponseAction::Continue) })
    }
}

#[derive(Default)]
struct StopAfterFirstNoHintPagination;

#[derive(Default)]
struct ConstantProgressChangingQueryPagination;

struct ConstantProgressChangingQueryState {
    page: u64,
}

#[derive(Default)]
struct AlwaysContinueExpectedPagination;

struct AlwaysContinueExpectedState {
    page: u64,
}

macro_rules! counting_expected_controller {
    ($controller:ident, $state:ident, $counter:ident) => {
        static $counter: AtomicUsize = AtomicUsize::new(0);

        #[derive(Default)]
        struct $controller;

        struct $state {
            page: u64,
        }
    };
}

counting_expected_controller!(
    EmptyHintCountingPagination,
    EmptyHintState,
    EMPTY_HINT_ADVANCES
);
counting_expected_controller!(
    ShortHintCountingPagination,
    ShortHintState,
    SHORT_HINT_ADVANCES
);
counting_expected_controller!(HardCapCountingPagination, HardCapState, HARD_CAP_ADVANCES);
counting_expected_controller!(
    TakeItemsCountingPagination,
    TakeItemsState,
    TAKE_ITEMS_ADVANCES
);

#[test]

fn custom_expected_items_per_page_zero_is_unrepresentable() {
    assert!(NonZeroUsize::new(0).is_none());
}

static NO_HINT_ADVANCES: AtomicUsize = AtomicUsize::new(0);

#[derive(Default)]
struct NoHintCountingPagination;

struct NoHintCountingState;

#[derive(Default)]
struct Pr64RuntimeOwnedShortPagePagination;

struct Pr64RuntimeOwnedShortPageState {
    page: u64,
}

static PR63_PAGINATED_DECODE_FAILURE_ADVANCES: AtomicUsize = AtomicUsize::new(0);

#[derive(Default)]
struct Pr63PaginatedDecodeFailurePagination;

struct Pr63PaginatedDecodeFailureState;

#[derive(Default)]
struct AlwaysContinueNoExpectedPagination;

struct AlwaysContinueNoExpectedState {
    page: u64,
}

#[derive(Default)]
struct AlwaysContinueNeverExpectedPagination;

struct AlwaysContinueNeverExpectedState {
    page: u64,
}

#[tokio::test]

async fn single_object_pagination_state_drives_endpoint_planning_order()
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
async fn single_object_paginate_binding_loads_and_stores_endpoint_state()
-> Result<(), ApiClientError> {
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
    let mut runtime = SingleObjectPaginationRuntimeAdapter::<HeaderBoundCustomPagination>::new();
    type Runtime = SingleObjectPaginationRuntimeAdapter<HeaderBoundCustomPagination>;
    <Runtime as SingleObjectPaginationRuntime<GeneratedHeaderBoundCustomEndpoint, Vec<String>>>::init(
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
        <Runtime as SingleObjectPaginationRuntime<
            GeneratedHeaderBoundCustomEndpoint,
            Vec<String>,
        >>::progress_key(&runtime),
        Some(ProgressKey::U64(1))
    );

    let mut endpoint = endpoint;
    let applied = <Runtime as SingleObjectPaginationRuntime<
        GeneratedHeaderBoundCustomEndpoint,
        Vec<String>,
    >>::apply(
        &mut runtime,
        &mut endpoint,
        PageApply {
            endpoint: "HeaderBoundCustom",
            page_index: 0,
            ctx: &ctx,
        },
    )?;
    assert_eq!(applied.expected_items_per_page, NonZeroUsize::new(2));
    assert_eq!(endpoint.page, 1);
    assert_eq!(endpoint.count, 2);
    assert_eq!(store_calls.load(std::sync::atomic::Ordering::SeqCst), 1);

    let decision = <Runtime as SingleObjectPaginationRuntime<
        GeneratedHeaderBoundCustomEndpoint,
        Vec<String>,
    >>::advance(
        &mut runtime,
        &mut endpoint,
        &ctx,
        &vec!["a".to_string()],
        PageAdvance {
            endpoint: "HeaderBoundCustom",
            page_index: 0,
            received_items: 1,
        },
    )?;
    assert_eq!(decision, PageDecision::Continue);
    assert_eq!(endpoint.page, 2);
    assert_eq!(
        <Runtime as SingleObjectPaginationRuntime<
            GeneratedHeaderBoundCustomEndpoint,
            Vec<String>,
        >>::progress_key(&runtime),
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

async fn offset_limit_single_object_pagination_advances_offset() -> Result<(), ApiClientError> {
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

    let endpoint = HeaderBoundOffsetLimitEndpoint {
        start: 0,
        count: 2,
        pagination: PaginationVariant::OffsetLimit {
            offset: 0,
            limit: 2,
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

async fn paged_single_object_pagination_advances_page() -> Result<(), ApiClientError> {
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

    let endpoint = HeaderBoundPagedEndpoint {
        page: 1,
        count: 2,
        pagination: PaginationVariant::Paged {
            page: 1,
            per_page: 2,
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

async fn single_object_pagination_requires_runtime_support_when_missing()
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

    let endpoint = QueryBoundPagedEndpoint {
        page: 1,
        count: 2,
        pagination: PaginationVariant::Paged {
            page: 1,
            per_page: 2,
        },
    };

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

async fn cursor_single_object_omits_initial_cursor_when_send_cursor_on_first_false()
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
        pagination: PaginationVariant::cursor::<CursorItems>(CursorPagination {
            cursor: Some("start".to_string()),
            per_page: 2,
            send_cursor_on_first: false,
            stop_when_cursor_missing: true,
        }),
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

async fn cursor_single_object_sends_initial_cursor_when_send_cursor_on_first_true()
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
        pagination: PaginationVariant::cursor::<CursorItems>(CursorPagination {
            cursor: Some("start".to_string()),
            per_page: 2,
            send_cursor_on_first: true,
            stop_when_cursor_missing: true,
        }),
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

async fn cursor_string_single_object_pagination_advances_cursor() -> Result<(), ApiClientError> {
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
        pagination: PaginationVariant::cursor::<CursorItems>(CursorPagination {
            cursor: Some("start".to_string()),
            per_page: 2,
            send_cursor_on_first: false,
            stop_when_cursor_missing: true,
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

async fn cursor_string_single_object_pagination_preserves_empty_cursor()
-> Result<(), ApiClientError> {
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
    let mut runtime = SingleObjectPaginationRuntimeAdapter::<CursorPagination<String>>::new();
    type Runtime = SingleObjectPaginationRuntimeAdapter<CursorPagination<String>>;
    <Runtime as SingleObjectPaginationRuntime<CursorItemsEndpoint, CursorItems>>::init(
        &mut runtime,
        &endpoint,
        PageApply {
            endpoint: "CursorItemsEndpoint",
            page_index: 0,
            ctx: &ctx,
        },
    )?;
    let _ = <Runtime as SingleObjectPaginationRuntime<CursorItemsEndpoint, CursorItems>>::apply(
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
    let decision =
        <Runtime as SingleObjectPaginationRuntime<CursorItemsEndpoint, CursorItems>>::advance(
            &mut runtime,
            &mut endpoint,
            &ctx,
            &page,
            PageAdvance {
                endpoint: "CursorItemsEndpoint",
                page_index: 0,
                received_items: 1,
            },
        )?;
    assert_eq!(decision, PageDecision::Continue);
    assert_eq!(endpoint.cursor, Some(String::new()));
    assert_eq!(
        <Runtime as SingleObjectPaginationRuntime<CursorItemsEndpoint, CursorItems>>::progress_key(
            &runtime
        ),
        Some(ProgressKey::Str(String::new()))
    );
    Ok(())
}

#[tokio::test]

async fn cursor_string_single_object_pagination_requires_runtime_support_when_missing()
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

    let endpoint = NoHintItemsEndpoint {
        policy: Default::default(),
        pagination: PaginationVariant::Paged {
            page: 1,
            per_page: 2,
        },
        ..Default::default()
    };

    let err = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(100))
        .collect()
        .await
        .expect_err("built-in pagination without runtime support must be rejected");
    assert!(matches!(err, ApiClientError::Pagination { .. }));
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
fn paged_single_object_pagination_rejects_zero_page() {
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
    assert!(err.to_string().contains("loop detected"));
    assert_eq!(sent.sent_count().await, 2);
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

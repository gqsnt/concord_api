use super::common::*;
use bytes::Bytes;
use concord_core::advanced::{
    AuthPlacement, CursorBindings, EndpointField, EndpointPaginationController,
    EndpointPaginationRuntime, EndpointPaginationRuntimeAdapter, OffsetLimitBindings, PageAdvance,
    PageApply, PageApplyResult, PageDecision, PageInit, PageRequest, PagedBindings,
    PaginationController, ProgressKey, RateLimitContext, RateLimitFuture, RateLimitPermit,
    RateLimitResponseAction, RateLimitResponseContext, RateLimiter,
};
use concord_core::internal::PaginationPlan;
use concord_core::prelude::{
    ApiClientError, CursorPagination, Endpoint, PaginatedEndpoint, PaginationTermination,
};
use http::{HeaderValue, Method, StatusCode};
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
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
struct InvalidHeaderValuePagination;

impl PaginationController<Vec<String>> for InvalidHeaderValuePagination {
    type State = ();

    fn init(&self, _ctx: PageInit<'_>) -> Result<Self::State, ApiClientError> {
        Ok(())
    }

    fn apply(
        &self,
        _state: &Self::State,
        _request: &mut PageRequest<'_>,
    ) -> Result<(), ApiClientError> {
        let ctx = concord_core::error::ErrorContext {
            endpoint: "Items",
            method: Method::GET,
        };
        let value =
            HeaderValue::from_str("bad\nheader").map_err(|source| ApiClientError::Pagination {
                ctx,
                msg: format!("invalid pagination header value: {source}").into(),
            })?;
        let _ = value;
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

#[derive(Clone)]
struct HeaderBoundCustomEndpoint {
    page: u64,
    count: u64,
    pagination: PaginationPlan,
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
            Some(self.pagination.clone()),
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
    fn endpoint_state_pagination(
        &self,
    ) -> Option<Box<dyn EndpointPaginationRuntime<Self, Self::Response>>> {
        Some(Box::new(EndpointPaginationRuntimeAdapter::new(
            HeaderBoundCustomPagination,
            HeaderBoundCustomBindings {
                page: EndpointField::new(
                    |ep: &GeneratedHeaderBoundCustomEndpoint| ep.page,
                    |ep: &mut GeneratedHeaderBoundCustomEndpoint, value| ep.page = value,
                ),
                count: EndpointField::new(
                    |ep: &GeneratedHeaderBoundCustomEndpoint| ep.count,
                    |ep: &mut GeneratedHeaderBoundCustomEndpoint, value| ep.count = value,
                ),
            },
        )))
    }
}

#[derive(Default)]
struct HeaderBoundCustomPagination;

#[derive(Clone)]
struct HeaderBoundCustomBindings<E> {
    page: EndpointField<E, u64>,
    count: EndpointField<E, u64>,
}

#[derive(Clone, Debug)]
struct HeaderBoundCustomState {
    page: u64,
    count: u64,
}

impl<E> EndpointPaginationController<E, Vec<String>> for HeaderBoundCustomPagination
where
    E: 'static,
{
    type Bindings = HeaderBoundCustomBindings<E>;
    type State = HeaderBoundCustomState;

    fn init(
        &self,
        bindings: &Self::Bindings,
        endpoint: &E,
        ctx: PageApply<'_>,
    ) -> Result<Self::State, ApiClientError> {
        let count = bindings.count.get(endpoint);
        if count == 0 {
            return Err(ApiClientError::Pagination {
                ctx: ctx.ctx.clone(),
                msg: "custom pagination page size must be non-zero".into(),
            });
        }
        Ok(HeaderBoundCustomState {
            page: bindings.page.get(endpoint),
            count,
        })
    }

    fn apply(
        &self,
        bindings: &Self::Bindings,
        state: &mut Self::State,
        endpoint: &mut E,
        ctx: PageApply<'_>,
    ) -> Result<PageApplyResult, ApiClientError> {
        if state.count == 0 {
            return Err(ApiClientError::Pagination {
                ctx: ctx.ctx.clone(),
                msg: "custom pagination page size must be non-zero".into(),
            });
        }
        bindings.page.set(endpoint, state.page);
        bindings.count.set(endpoint, state.count);
        Ok(PageApplyResult {
            expected_items_per_page: NonZeroUsize::new(state.count as usize),
        })
    }

    fn advance(
        &self,
        _bindings: &Self::Bindings,
        state: &mut Self::State,
        _page: &Vec<String>,
        _ctx: PageAdvance<'_>,
    ) -> Result<PageDecision, ApiClientError> {
        state.page += 1;
        Ok(PageDecision::Continue)
    }

    fn progress_key(&self, state: &Self::State) -> Option<ProgressKey> {
        Some(ProgressKey::U64(state.page))
    }
}

impl PaginatedEndpoint<TestCx> for HeaderBoundCustomEndpoint {
    fn endpoint_state_pagination(
        &self,
    ) -> Option<Box<dyn EndpointPaginationRuntime<Self, Self::Response>>> {
        Some(Box::new(EndpointPaginationRuntimeAdapter::new(
            HeaderBoundCustomPagination,
            HeaderBoundCustomBindings {
                page: EndpointField::new(
                    |ep: &Self| ep.page,
                    |ep: &mut Self, value| ep.page = value,
                ),
                count: EndpointField::new(
                    |ep: &Self| ep.count,
                    |ep: &mut Self, value| ep.count = value,
                ),
            },
        )))
    }
}

#[derive(Clone)]
struct HeaderBoundOffsetLimitEndpoint {
    start: u64,
    count: u64,
    pagination: PaginationPlan,
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
            Some(self.pagination.clone()),
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
    fn endpoint_state_pagination(
        &self,
    ) -> Option<Box<dyn concord_core::internal::EndpointPaginationRuntime<Self, Self::Response>>>
    {
        Some(Box::new(EndpointPaginationRuntimeAdapter::new(
            concord_core::advanced::OffsetLimitPagination::default(),
            OffsetLimitBindings {
                offset: EndpointField::new(
                    |ep: &Self| ep.start,
                    |ep: &mut Self, value| ep.start = value,
                ),
                limit: EndpointField::new(
                    |ep: &Self| ep.count,
                    |ep: &mut Self, value| ep.count = value,
                ),
            },
        )))
    }
}

#[derive(Clone)]
struct HeaderBoundPagedEndpoint {
    page: u64,
    count: u64,
    pagination: PaginationPlan,
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
            Some(self.pagination.clone()),
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
    fn endpoint_state_pagination(
        &self,
    ) -> Option<Box<dyn concord_core::internal::EndpointPaginationRuntime<Self, Self::Response>>>
    {
        Some(Box::new(EndpointPaginationRuntimeAdapter::new(
            concord_core::advanced::PagedPagination::default(),
            PagedBindings {
                page: EndpointField::new(
                    |ep: &Self| ep.page,
                    |ep: &mut Self, value| ep.page = value,
                ),
                per_page: EndpointField::new(
                    |ep: &Self| ep.count,
                    |ep: &mut Self, value| ep.count = value,
                ),
            },
        )))
    }
}

#[derive(Clone)]
struct HeaderBoundCursorEndpoint {
    cursor: Option<String>,
    count: u64,
    pagination: PaginationPlan,
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
            Some(self.pagination.clone()),
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
    fn endpoint_state_pagination(
        &self,
    ) -> Option<Box<dyn concord_core::internal::EndpointPaginationRuntime<Self, Self::Response>>>
    {
        Some(Box::new(EndpointPaginationRuntimeAdapter::new(
            concord_core::advanced::CursorPagination::default(),
            CursorBindings {
                cursor: EndpointField::new(
                    |ep: &Self| ep.cursor.clone(),
                    |ep: &mut Self, value| ep.cursor = value,
                ),
                per_page: EndpointField::new(
                    |ep: &Self| ep.count,
                    |ep: &mut Self, value| ep.count = value,
                ),
            },
        )))
    }
}

#[derive(Clone)]
struct QueryBoundPagedEndpoint {
    page: u64,
    count: u64,
    pagination: PaginationPlan,
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
            Some(self.pagination.clone()),
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

impl PaginationController<Vec<String>> for AuthQueryCollisionPagination {
    type State = ();

    fn init(&self, _ctx: PageInit<'_>) -> Result<Self::State, ApiClientError> {
        Ok(())
    }

    fn apply(
        &self,
        _state: &Self::State,
        request: &mut PageRequest<'_>,
    ) -> Result<(), ApiClientError> {
        request.set_query("api_key", "public-value");
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
struct TracedAuthQueryCollisionPagination;

impl PaginationController<Vec<String>> for TracedAuthQueryCollisionPagination {
    type State = ();

    fn init(&self, _ctx: PageInit<'_>) -> Result<Self::State, ApiClientError> {
        Ok(())
    }

    fn apply(
        &self,
        _state: &Self::State,
        request: &mut PageRequest<'_>,
    ) -> Result<(), ApiClientError> {
        PR69_MUTATION_EVENTS.with(|events| {
            events
                .lock()
                .expect("mutation events lock")
                .push("mutation");
        });
        request.set_query("api_key", "public-value");
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
struct AuthHeaderCollisionPagination;

impl PaginationController<Vec<String>> for AuthHeaderCollisionPagination {
    type State = ();

    fn init(&self, _ctx: PageInit<'_>) -> Result<Self::State, ApiClientError> {
        Ok(())
    }

    fn apply(
        &self,
        _state: &Self::State,
        request: &mut PageRequest<'_>,
    ) -> Result<(), ApiClientError> {
        request.set_header("x-api-key", HeaderValue::from_static("public-value"))?;
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
struct AuthorizationCollisionPagination;

impl PaginationController<Vec<String>> for AuthorizationCollisionPagination {
    type State = ();

    fn init(&self, _ctx: PageInit<'_>) -> Result<Self::State, ApiClientError> {
        Ok(())
    }

    fn apply(
        &self,
        _state: &Self::State,
        request: &mut PageRequest<'_>,
    ) -> Result<(), ApiClientError> {
        request.set_header(
            http::header::AUTHORIZATION,
            HeaderValue::from_static("public-value"),
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
struct PublicMutationPagination;

impl PaginationController<Vec<String>> for PublicMutationPagination {
    type State = ();

    fn init(&self, _ctx: PageInit<'_>) -> Result<Self::State, ApiClientError> {
        Ok(())
    }

    fn apply(
        &self,
        _state: &Self::State,
        request: &mut PageRequest<'_>,
    ) -> Result<(), ApiClientError> {
        request.set_query("page", 7);
        request.set_header("x-page-token", HeaderValue::from_static("7"))?;
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

#[derive(Clone)]
struct PaginationEndpoint {
    name: &'static str,
    path: &'static str,
    policy: concord_core::internal::ResolvedPolicy,
    pagination: Option<PaginationPlan>,
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
            self.pagination.clone(),
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

#[derive(Default)]
struct AlwaysContinueExpectedPagination;

struct AlwaysContinueExpectedState {
    page: u64,
}

impl PaginationController<Vec<String>> for AlwaysContinueExpectedPagination {
    type State = AlwaysContinueExpectedState;

    fn init(&self, _ctx: PageInit<'_>) -> Result<Self::State, ApiClientError> {
        Ok(AlwaysContinueExpectedState { page: 0 })
    }

    fn apply(
        &self,
        state: &Self::State,
        request: &mut PageRequest<'_>,
    ) -> Result<(), ApiClientError> {
        request.set_query("page", state.page);
        request.set_expected_items_per_page(NonZeroUsize::new(2).expect("test page size"));
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

    fn progress_key(&self, state: &Self::State) -> Option<ProgressKey> {
        Some(ProgressKey::U64(state.page))
    }
}

macro_rules! counting_expected_controller {
    ($controller:ident, $state:ident, $counter:ident) => {
        static $counter: AtomicUsize = AtomicUsize::new(0);

        #[derive(Default)]
        struct $controller;

        struct $state {
            page: u64,
        }

        impl PaginationController<Vec<String>> for $controller {
            type State = $state;

            fn init(&self, _ctx: PageInit<'_>) -> Result<Self::State, ApiClientError> {
                Ok($state { page: 0 })
            }

            fn apply(
                &self,
                state: &Self::State,
                request: &mut PageRequest<'_>,
            ) -> Result<(), ApiClientError> {
                request.set_query("page", state.page);
                request.set_expected_items_per_page(
                    NonZeroUsize::new(2).expect("test page size is non-zero"),
                );
                Ok(())
            }

            fn advance(
                &self,
                state: &mut Self::State,
                _page: &Vec<String>,
                _ctx: PageAdvance<'_>,
            ) -> Result<PageDecision, ApiClientError> {
                $counter.fetch_add(1, AtomicOrdering::SeqCst);
                state.page += 1;
                Ok(PageDecision::Continue)
            }

            fn progress_key(&self, state: &Self::State) -> Option<ProgressKey> {
                Some(ProgressKey::U64(state.page))
            }
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

impl PaginationController<NoHintItems> for NoHintCountingPagination {
    type State = NoHintCountingState;

    fn init(&self, _ctx: PageInit<'_>) -> Result<Self::State, ApiClientError> {
        Ok(NoHintCountingState)
    }

    fn apply(
        &self,
        _state: &Self::State,
        request: &mut PageRequest<'_>,
    ) -> Result<(), ApiClientError> {
        request
            .set_expected_items_per_page(NonZeroUsize::new(2).expect("test page size is non-zero"));
        Ok(())
    }

    fn advance(
        &self,
        _state: &mut Self::State,
        _page: &NoHintItems,
        _ctx: PageAdvance<'_>,
    ) -> Result<PageDecision, ApiClientError> {
        NO_HINT_ADVANCES.fetch_add(1, AtomicOrdering::SeqCst);
        Ok(PageDecision::Stop)
    }

    fn progress_key(&self, _state: &Self::State) -> Option<ProgressKey> {
        None
    }
}

#[derive(Default)]
struct Pr64RuntimeOwnedShortPagePagination;

struct Pr64RuntimeOwnedShortPageState {
    page: u64,
}

impl PaginationController<PageOnlyItems> for Pr64RuntimeOwnedShortPagePagination {
    type State = Pr64RuntimeOwnedShortPageState;

    fn init(&self, _ctx: PageInit<'_>) -> Result<Self::State, ApiClientError> {
        Ok(Pr64RuntimeOwnedShortPageState { page: 0 })
    }

    fn apply(
        &self,
        state: &Self::State,
        request: &mut PageRequest<'_>,
    ) -> Result<(), ApiClientError> {
        request.set_query("page", state.page);
        request.set_query("limit", 2);
        request
            .set_expected_items_per_page(NonZeroUsize::new(2).expect("test page size is non-zero"));
        Ok(())
    }

    fn advance(
        &self,
        state: &mut Self::State,
        _page: &PageOnlyItems,
        _ctx: PageAdvance<'_>,
    ) -> Result<PageDecision, ApiClientError> {
        PR64_CUSTOM_PAGINATION_EVENTS.with(|events| {
            events
                .lock()
                .expect("custom pagination events lock")
                .push("advance");
        });
        state.page += 1;
        Ok(PageDecision::Continue)
    }

    fn progress_key(&self, state: &Self::State) -> Option<ProgressKey> {
        Some(ProgressKey::U64(state.page))
    }
}

static PR63_PAGINATED_DECODE_FAILURE_ADVANCES: AtomicUsize = AtomicUsize::new(0);

#[derive(Default)]
struct Pr63PaginatedDecodeFailurePagination;

struct Pr63PaginatedDecodeFailureState;

impl PaginationController<NoHintItems> for Pr63PaginatedDecodeFailurePagination {
    type State = Pr63PaginatedDecodeFailureState;

    fn init(&self, _ctx: PageInit<'_>) -> Result<Self::State, ApiClientError> {
        Ok(Pr63PaginatedDecodeFailureState)
    }

    fn apply(
        &self,
        _state: &Self::State,
        request: &mut PageRequest<'_>,
    ) -> Result<(), ApiClientError> {
        request
            .set_expected_items_per_page(NonZeroUsize::new(2).expect("test page size is non-zero"));
        Ok(())
    }

    fn advance(
        &self,
        _state: &mut Self::State,
        _page: &NoHintItems,
        _ctx: PageAdvance<'_>,
    ) -> Result<PageDecision, ApiClientError> {
        PR63_PAGINATED_DECODE_FAILURE_ADVANCES.fetch_add(1, AtomicOrdering::SeqCst);
        Ok(PageDecision::Stop)
    }

    fn progress_key(&self, _state: &Self::State) -> Option<ProgressKey> {
        None
    }
}

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

impl PaginationController<Vec<String>> for AlwaysContinueNeverExpectedPagination {
    type State = AlwaysContinueNeverExpectedState;

    fn init(&self, _ctx: PageInit<'_>) -> Result<Self::State, ApiClientError> {
        Ok(AlwaysContinueNeverExpectedState { page: 0 })
    }

    fn apply(
        &self,
        state: &Self::State,
        request: &mut PageRequest<'_>,
    ) -> Result<(), ApiClientError> {
        request.set_query("page", state.page);
        request.clear_expected_items_per_page();
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

    fn progress_key(&self, state: &Self::State) -> Option<ProgressKey> {
        Some(ProgressKey::U64(state.page))
    }
}

impl PaginationController<Vec<String>> for AlwaysContinueNoExpectedPagination {
    type State = AlwaysContinueNoExpectedState;

    fn init(&self, _ctx: PageInit<'_>) -> Result<Self::State, ApiClientError> {
        Ok(AlwaysContinueNoExpectedState { page: 0 })
    }

    fn apply(
        &self,
        state: &Self::State,
        request: &mut PageRequest<'_>,
    ) -> Result<(), ApiClientError> {
        request.set_query("page", state.page);
        if state.page == 0 {
            request.set_expected_items_per_page(NonZeroUsize::new(2).expect("test page size"));
        } else {
            request.clear_expected_items_per_page();
        }
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

    fn progress_key(&self, state: &Self::State) -> Option<ProgressKey> {
        Some(ProgressKey::U64(state.page))
    }
}

#[tokio::test]
async fn pending_request_pages_surface_remains_available() -> Result<(), ApiClientError> {
    let transport = MockTransport::new(
        Arc::new(Mutex::new(Vec::new())),
        vec![MockResponse::text(StatusCode::OK, "a,b")],
    );
    let client = client(TestAuthVars::default(), transport);

    let endpoint = ItemsEndpoint {
        policy: Default::default(),
        pagination: PaginationPlan::custom::<HeaderTokenPagination, Vec<String>>(),
        ..Default::default()
    };

    let items = client
        .request(endpoint)
        .pages(PaginationTermination::take_pages(1))
        .collect()
        .await?;

    assert_eq!(items, vec!["a".to_string(), "b".to_string()]);
    Ok(())
}

#[tokio::test]
async fn custom_endpoint_state_pagination_renders_endpoint_fields() -> Result<(), ApiClientError> {
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

    let endpoint = HeaderBoundCustomEndpoint {
        page: 1,
        count: 2,
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

    let endpoint = GeneratedHeaderBoundCustomEndpoint { page: 1, count: 2 };

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
    Ok(())
}

#[test]
fn custom_endpoint_state_pagination_reports_progress_and_typed_errors() {
    let controller = HeaderBoundCustomPagination;
    let bindings = HeaderBoundCustomBindings {
        page: EndpointField::new(
            |ep: &HeaderBoundCustomEndpoint| ep.page,
            |ep: &mut HeaderBoundCustomEndpoint, value| ep.page = value,
        ),
        count: EndpointField::new(
            |ep: &HeaderBoundCustomEndpoint| ep.count,
            |ep: &mut HeaderBoundCustomEndpoint, value| ep.count = value,
        ),
    };
    let mut adapter = EndpointPaginationRuntimeAdapter::new(controller, bindings);
    let ctx = concord_core::error::ErrorContext {
        endpoint: "HeaderBoundCustom",
        method: Method::GET,
    };
    let mut endpoint = HeaderBoundCustomEndpoint {
        page: 1,
        count: 2,
        pagination: PaginationPlan::custom::<HeaderTokenPagination, Vec<String>>(),
    };

    assert_eq!(adapter.progress_key(), None);
    adapter
        .init(
            &endpoint,
            PageApply {
                endpoint: "HeaderBoundCustom",
                page_index: 0,
                ctx: &ctx,
            },
        )
        .unwrap();
    assert_eq!(adapter.progress_key(), Some(ProgressKey::U64(1)));

    let applied = adapter
        .apply(
            &mut endpoint,
            PageApply {
                endpoint: "HeaderBoundCustom",
                page_index: 1,
                ctx: &ctx,
            },
        )
        .unwrap();
    assert_eq!(applied.expected_items_per_page, NonZeroUsize::new(2));
    assert_eq!(endpoint.page, 1);
    assert_eq!(endpoint.count, 2);

    let decision = adapter
        .advance(
            &ctx,
            &vec!["a".to_string(), "b".to_string()],
            PageAdvance {
                endpoint: "HeaderBoundCustom",
                page_index: 1,
                received_items: 2,
            },
        )
        .unwrap();
    assert_eq!(decision, PageDecision::Continue);
    assert_eq!(adapter.progress_key(), Some(ProgressKey::U64(2)));

    let bad_bindings = HeaderBoundCustomBindings {
        page: EndpointField::new(
            |ep: &HeaderBoundCustomEndpoint| ep.page,
            |ep: &mut HeaderBoundCustomEndpoint, value| ep.page = value,
        ),
        count: EndpointField::new(
            |ep: &HeaderBoundCustomEndpoint| ep.count,
            |ep: &mut HeaderBoundCustomEndpoint, value| ep.count = value,
        ),
    };
    let mut bad_adapter =
        EndpointPaginationRuntimeAdapter::new(HeaderBoundCustomPagination, bad_bindings);
    let bad_endpoint = HeaderBoundCustomEndpoint {
        page: 1,
        count: 0,
        pagination: PaginationPlan::custom::<HeaderTokenPagination, Vec<String>>(),
    };
    let err = bad_adapter
        .init(
            &bad_endpoint,
            PageApply {
                endpoint: "HeaderBoundCustom",
                page_index: 0,
                ctx: &ctx,
            },
        )
        .expect_err("zero page size must be rejected by the controller");
    assert!(matches!(err, ApiClientError::Pagination { .. }));
}

#[tokio::test]
async fn custom_pagination_query_request_fallback_still_works() -> Result<(), ApiClientError> {
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
        ..Default::default()
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
        ..Default::default()
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
async fn offset_limit_endpoint_state_mutation_renders_endpoint_fields() -> Result<(), ApiClientError>
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

    let endpoint = HeaderBoundOffsetLimitEndpoint {
        start: 0,
        count: 2,
        pagination: PaginationPlan::OffsetLimit {
            offset_key: "offset".to_string(),
            limit_key: "limit".to_string(),
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
async fn paged_endpoint_state_mutation_renders_endpoint_fields() -> Result<(), ApiClientError> {
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
        pagination: PaginationPlan::Paged {
            page_key: "page".to_string(),
            per_page_key: "per_page".to_string(),
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
async fn built_in_pagination_without_endpoint_state_hook_is_rejected() -> Result<(), ApiClientError>
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

    let endpoint = QueryBoundPagedEndpoint {
        page: 1,
        count: 2,
        pagination: PaginationPlan::Paged {
            page_key: "pageNo".to_string(),
            per_page_key: "pageSize".to_string(),
            page: 1,
            per_page: 2,
        },
    };

    let err = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(4))
        .collect()
        .await
        .expect_err("built-in pagination without endpoint-state support must be rejected");
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
        pagination: PaginationPlan::OffsetLimit {
            offset_key: "offset".to_string(),
            limit_key: "limit".to_string(),
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
        pagination: PaginationPlan::OffsetLimit {
            offset_key: "offset".to_string(),
            limit_key: "limit".to_string(),
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
        cursor: Some("start".to_string()),
        count: 2,
        pagination: PaginationPlan::cursor::<CursorItems>(CursorPagination {
            cursor_key: "cursor".into(),
            per_page_key: "limit".into(),
            cursor: Some("start".to_string()),
            per_page: 2,
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
async fn cursor_endpoint_state_mutation_renders_endpoint_fields() -> Result<(), ApiClientError> {
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
        pagination: PaginationPlan::cursor::<CursorItems>(CursorPagination {
            cursor_key: "cursor".into(),
            per_page_key: "limit".into(),
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
async fn cursor_built_in_pagination_without_endpoint_state_hook_is_rejected()
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
        pagination: PaginationPlan::Paged {
            page_key: "cursorToken".into(),
            per_page_key: "pageSize".into(),
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
        .expect_err("built-in pagination without endpoint-state support must be rejected");
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
        pagination: PaginationPlan::cursor::<CursorItems>(CursorPagination {
            cursor_key: "cursor".into(),
            per_page_key: "limit".into(),
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
        pagination: PaginationPlan::cursor::<CursorItems>(CursorPagination {
            cursor_key: "cursor".into(),
            per_page_key: "limit".into(),
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
    let transport = MockTransport::new(
        events,
        vec![MockResponse::text(StatusCode::OK, "a,b|next=")],
    );
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let endpoint = CursorItemsEndpoint {
        policy: Default::default(),
        cursor: None,
        count: 2,
        pagination: PaginationPlan::cursor::<CursorItems>(CursorPagination {
            cursor_key: "cursor".into(),
            per_page_key: "limit".into(),
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
        page: 1,
        count: 2,
        pagination: PaginationPlan::Paged {
            page_key: "page".to_string(),
            per_page_key: "per_page".to_string(),
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
        pagination: PaginationPlan::Paged {
            page_key: "page".to_string(),
            per_page_key: "per_page".to_string(),
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
        pagination: PaginationPlan::OffsetLimit {
            offset_key: "offset".to_string(),
            limit_key: "limit".to_string(),
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
        pagination: PaginationPlan::OffsetLimit {
            offset_key: "offset".to_string(),
            limit_key: "limit".to_string(),
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
        pagination: PaginationPlan::OffsetLimit {
            offset_key: "offset".to_string(),
            limit_key: "limit".to_string(),
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
        pagination: PaginationPlan::OffsetLimit {
            offset_key: "offset".to_string(),
            limit_key: "limit".to_string(),
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
        pagination: PaginationPlan::OffsetLimit {
            offset_key: "offset".to_string(),
            limit_key: "limit".to_string(),
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
        pagination: PaginationPlan::OffsetLimit {
            offset_key: "offset".to_string(),
            limit_key: "limit".to_string(),
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
        pagination: PaginationPlan::OffsetLimit {
            offset_key: "offset".to_string(),
            limit_key: "limit".to_string(),
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
        pagination: PaginationPlan::OffsetLimit {
            offset_key: "offset".to_string(),
            limit_key: "limit".to_string(),
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
        pagination: PaginationPlan::OffsetLimit {
            offset_key: "offset".to_string(),
            limit_key: "limit".to_string(),
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
        pagination: PaginationPlan::OffsetLimit {
            offset_key: "offset".to_string(),
            limit_key: "limit".to_string(),
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
        pagination: PaginationPlan::cursor::<CursorItems>(CursorPagination {
            cursor_key: "cursor".into(),
            per_page_key: "limit".into(),
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
        ..Default::default()
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
async fn collect_offset_short_first_page_stops_via_runtime() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, "a,b,c")]);
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let endpoint = ItemsEndpoint {
        policy: Default::default(),
        start: 0,
        count: 100,
        pagination: PaginationPlan::OffsetLimit {
            offset_key: "offset".to_string(),
            limit_key: "limit".to_string(),
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
        pagination: PaginationPlan::OffsetLimit {
            offset_key: "offset".to_string(),
            limit_key: "limit".to_string(),
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
        pagination: PaginationPlan::Paged {
            page_key: "page".to_string(),
            per_page_key: "per_page".to_string(),
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
        pagination: PaginationPlan::cursor::<CursorItems>(CursorPagination {
            cursor_key: "cursor".into(),
            per_page_key: "limit".into(),
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
        pagination: PaginationPlan::cursor::<CursorItems>(CursorPagination {
            cursor_key: "cursor".into(),
            per_page_key: "limit".into(),
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
        pagination: PaginationPlan::OffsetLimit {
            offset_key: "offset".to_string(),
            limit_key: "limit".to_string(),
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
        pagination: PaginationPlan::OffsetLimit {
            offset_key: "offset".to_string(),
            limit_key: "limit".to_string(),
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
        pagination: PaginationPlan::OffsetLimit {
            offset_key: "offset".to_string(),
            limit_key: "limit".to_string(),
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
        pagination: PaginationPlan::OffsetLimit {
            offset_key: "offset".to_string(),
            limit_key: "limit".to_string(),
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
async fn custom_collect_empty_page_with_hint_does_not_call_advance() -> Result<(), ApiClientError> {
    EMPTY_HINT_ADVANCES.store(0, AtomicOrdering::SeqCst);
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, "")]);
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let endpoint = ItemsEndpoint {
        policy: Default::default(),
        pagination: PaginationPlan::custom::<EmptyHintCountingPagination, Vec<String>>(),
        ..Default::default()
    };

    let items = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(100))
        .collect()
        .await?;

    assert!(items.is_empty());
    assert_eq!(sent.sent_count().await, 1);
    assert_eq!(EMPTY_HINT_ADVANCES.load(AtomicOrdering::SeqCst), 0);
    Ok(())
}

#[tokio::test]
async fn custom_collect_short_page_with_hint_does_not_call_advance() -> Result<(), ApiClientError> {
    SHORT_HINT_ADVANCES.store(0, AtomicOrdering::SeqCst);
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, "a")]);
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let endpoint = ItemsEndpoint {
        policy: Default::default(),
        pagination: PaginationPlan::custom::<ShortHintCountingPagination, Vec<String>>(),
        ..Default::default()
    };

    let items = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(100))
        .collect()
        .await?;

    assert_eq!(items, vec!["a"]);
    assert_eq!(sent.sent_count().await, 1);
    assert_eq!(SHORT_HINT_ADVANCES.load(AtomicOrdering::SeqCst), 0);
    Ok(())
}

#[tokio::test]
async fn hard_item_cap_hint_exceeded_does_not_call_advance() {
    HARD_CAP_ADVANCES.store(0, AtomicOrdering::SeqCst);
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, "a,b,c")]);
    let client = client(TestAuthVars::default(), transport);
    let endpoint = ItemsEndpoint {
        policy: Default::default(),
        pagination: PaginationPlan::custom::<HardCapCountingPagination, Vec<String>>(),
        ..Default::default()
    };

    let err = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_item_cap(2))
        .collect()
        .await
        .expect_err("the exact hint exceeds the hard item cap");

    assert!(matches!(err, ApiClientError::PaginationLimit { .. }));
    assert_eq!(HARD_CAP_ADVANCES.load(AtomicOrdering::SeqCst), 0);
}

#[tokio::test]
async fn take_items_hint_reached_does_not_call_advance() -> Result<(), ApiClientError> {
    TAKE_ITEMS_ADVANCES.store(0, AtomicOrdering::SeqCst);
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, "a,b,c")]);
    let client = client(TestAuthVars::default(), transport);
    let endpoint = ItemsEndpoint {
        policy: Default::default(),
        pagination: PaginationPlan::custom::<TakeItemsCountingPagination, Vec<String>>(),
        ..Default::default()
    };

    let items = client
        .request(endpoint)
        .paginate(PaginationTermination::take_items(2))
        .collect()
        .await?;

    assert_eq!(items, vec!["a", "b"]);
    assert_eq!(TAKE_ITEMS_ADVANCES.load(AtomicOrdering::SeqCst), 0);
    Ok(())
}

#[tokio::test]
async fn custom_collect_no_hint_empty_page_may_call_advance_before_exact_collection()
-> Result<(), ApiClientError> {
    NO_HINT_ADVANCES.store(0, AtomicOrdering::SeqCst);
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, "")]);
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);
    let endpoint = NoHintItemsEndpoint {
        policy: Default::default(),
        pagination: PaginationPlan::custom::<NoHintCountingPagination, NoHintItems>(),
    };

    let items = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(2))
        .collect()
        .await?;

    assert!(items.is_empty());
    assert_eq!(sent.sent_count().await, 1);
    assert_eq!(NO_HINT_ADVANCES.load(AtomicOrdering::SeqCst), 1);
    Ok(())
}

#[tokio::test]
async fn custom_collect_full_page_continues_when_expected_items_set() -> Result<(), ApiClientError>
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
        pagination: PaginationPlan::custom::<AlwaysContinueExpectedPagination, Vec<String>>(),
        ..Default::default()
    };

    let items = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(100))
        .collect()
        .await?;

    assert_eq!(items, vec!["a", "b", "c"]);
    assert_eq!(sent.sent_count().await, 2);
    Ok(())
}

#[tokio::test]
async fn custom_collect_short_page_does_not_stop_when_expected_items_missing()
-> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "a"),
            MockResponse::text(StatusCode::OK, ""),
        ],
    );
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let endpoint = ItemsEndpoint {
        policy: Default::default(),
        pagination: PaginationPlan::custom::<AlwaysContinueNeverExpectedPagination, Vec<String>>(),
        ..Default::default()
    };

    let items = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(100))
        .collect()
        .await?;

    assert_eq!(items, vec!["a"]);
    assert_eq!(sent.sent_count().await, 2);
    Ok(())
}

#[tokio::test]
async fn custom_expected_items_is_per_page_not_sticky() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "a,b"),
            MockResponse::text(StatusCode::OK, "c"),
            MockResponse::text(StatusCode::OK, ""),
        ],
    );
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let endpoint = ItemsEndpoint {
        policy: Default::default(),
        pagination: PaginationPlan::custom::<AlwaysContinueNoExpectedPagination, Vec<String>>(),
        ..Default::default()
    };

    let items = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(100))
        .collect()
        .await?;

    assert_eq!(items, vec!["a", "b", "c"]);
    assert_eq!(sent.sent_count().await, 3);
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
        pagination: PaginationPlan::OffsetLimit {
            offset_key: "offset".to_string(),
            limit_key: "limit".to_string(),
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
        pagination: PaginationPlan::OffsetLimit {
            offset_key: "offset".to_string(),
            limit_key: "limit".to_string(),
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
async fn paginated_page_decode_failure_does_not_advance_page_state() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(
            StatusCode::OK,
            Bytes::from_static(b"\xff"),
        )],
    );
    let sent = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    configure_runtime(&mut client, None);

    let endpoint = NoHintItemsEndpoint {
        policy: Default::default(),
        pagination: PaginationPlan::custom::<Pr63PaginatedDecodeFailurePagination, NoHintItems>(),
    };

    let err = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(4))
        .collect()
        .await
        .expect_err("invalid page should fail before page advance");

    assert!(err.to_string().contains("decode error"));
    assert_eq!(sent.sent_count().await, 1);
    assert_eq!(
        PR63_PAGINATED_DECODE_FAILURE_ADVANCES.load(AtomicOrdering::SeqCst),
        0
    );
}

#[tokio::test]
async fn paginated_successful_page_decodes_after_custom_pagination() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "a,b")],
    );
    let sent = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    configure_runtime(&mut client, None);

    let endpoint = NoHintItemsEndpoint {
        policy: Default::default(),
        pagination: PaginationPlan::custom::<StopAfterFirstNoHintPagination, NoHintItems>(),
    };

    let items = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(1))
        .collect()
        .await?;

    assert_eq!(items, vec!["a".to_string(), "b".to_string()]);
    assert_eq!(sent.sent_count().await, 1);
    Ok(())
}

#[tokio::test]
async fn custom_pagination_example_short_page_stop_is_runtime_owned() -> Result<(), ApiClientError>
{
    let events = Arc::new(StdMutex::new(Vec::new()));
    let events_for_scope = events.clone();
    let transport = MockTransport::new(
        Arc::new(Mutex::new(Vec::new())),
        vec![
            MockResponse::text(StatusCode::OK, "a,b"),
            MockResponse::text(StatusCode::OK, "c"),
        ],
    );
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let endpoint = PageOnlyItemsEndpoint {
        policy: Default::default(),
        pagination: PaginationPlan::custom::<Pr64RuntimeOwnedShortPagePagination, PageOnlyItems>(),
        ..Default::default()
    };

    let items = PR64_CUSTOM_PAGINATION_EVENTS
        .scope(events_for_scope, async move {
            client
                .request(endpoint)
                .paginate(PaginationTermination::hard_page_cap(10))
                .collect()
                .await
        })
        .await?;

    assert_eq!(
        items,
        vec!["a".to_string(), "b".to_string(), "c".to_string()]
    );
    assert_eq!(sent.sent_count().await, 2);
    assert_eq!(
        &*events.lock().expect("custom pagination events lock"),
        &["advance"]
    );
    Ok(())
}

#[tokio::test]
async fn page_request_set_invalid_header_name_returns_typed_error_before_side_effects() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let rate_limiter = Arc::new(RecordingRateLimiter::new(events.clone()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "should-not-send")],
    );
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);
    let mut client = client;
    configure_runtime(&mut client, Some(rate_limiter));

    let endpoint = PaginationEndpoint {
        name: "Items",
        path: "/items",
        policy: Default::default(),
        pagination: Some(PaginationPlan::custom::<InvalidHeaderPagination, Vec<String>>()),
    };

    let err = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(2))
        .collect()
        .await
        .expect_err("invalid header name should return a typed pagination error");

    assert!(matches!(err, ApiClientError::Pagination { .. }));
    assert_eq!(err.context().endpoint, "Items");
    assert_eq!(err.context().method, Method::GET);
    assert!(err.to_string().contains("invalid pagination header name"));
    assert_eq!(sent.sent_count().await, 0);
    let events = events.lock().await.clone();
    assert!(!events.iter().any(|event| event == "rate_acquire"));
    assert!(!events.iter().any(|event| event == "transport"));
}

#[tokio::test]
async fn controller_invalid_header_value_construction_returns_typed_error_before_side_effects() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let rate_limiter = Arc::new(RecordingRateLimiter::new(events.clone()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "should-not-send")],
    );
    let sent = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    configure_runtime(&mut client, Some(rate_limiter));

    let endpoint = PaginationEndpoint {
        name: "Items",
        path: "/items",
        policy: Default::default(),
        pagination: Some(PaginationPlan::custom::<
            InvalidHeaderValuePagination,
            Vec<String>,
        >()),
    };

    let err = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(2))
        .collect()
        .await
        .expect_err("invalid header value should return a typed pagination error");

    assert!(matches!(err, ApiClientError::Pagination { .. }));
    assert_eq!(err.context().endpoint, "Items");
    assert_eq!(err.context().method, Method::GET);
    assert!(err.to_string().contains("invalid pagination header value"));
    assert!(!err.to_string().contains("should-not-send"));
    assert_eq!(sent.sent_count().await, 0);
    let events = events.lock().await.clone();
    assert!(!events.iter().any(|event| event == "rate_acquire"));
    assert!(!events.iter().any(|event| event == "transport"));
}

#[tokio::test]
async fn custom_pagination_header_error_has_endpoint_context() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let rate_limiter = Arc::new(RecordingRateLimiter::new(events.clone()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "unused")],
    );
    let mut client = client(TestAuthVars::default(), transport);
    configure_runtime(&mut client, Some(rate_limiter));

    let endpoint = PaginationEndpoint {
        name: "Items",
        path: "/items",
        policy: Default::default(),
        pagination: Some(PaginationPlan::custom::<InvalidHeaderPagination, Vec<String>>()),
    };

    let err = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(2))
        .collect()
        .await
        .expect_err("invalid header name should return a typed pagination error");

    assert_eq!(err.context().endpoint, "Items");
    assert_eq!(err.context().method, Method::GET);
}

#[tokio::test]
async fn custom_pagination_cannot_override_pending_auth_query_material() {
    const QUERY_AUTH_SENTINEL: &str = "RAW_QUERY_AUTH_SENTINEL";

    let events = Arc::new(Mutex::new(Vec::new()));
    let rate_limiter = Arc::new(RecordingRateLimiter::new(events.clone()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "should-not-send")],
    );
    let sent = transport.clone();
    let mut client = client(
        TestAuthVars {
            token: Some(QUERY_AUTH_SENTINEL.to_string()),
            identity: "query",
        },
        transport,
    );
    configure_runtime(&mut client, Some(rate_limiter));

    let policy = auth_policy(AuthPlacement::Query("api_key"));
    let endpoint = PaginationEndpoint {
        name: "Items",
        path: "/items",
        policy,
        pagination: Some(PaginationPlan::custom::<
            AuthQueryCollisionPagination,
            Vec<String>,
        >()),
    };

    let err = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(2))
        .collect()
        .await
        .expect_err("auth query collision should fail before rate-limit or transport");

    match err {
        ApiClientError::Auth { source, .. } => {
            assert_eq!(
                source.kind,
                concord_core::advanced::AuthErrorKind::InvalidConfiguration
            );
            let msg = source.to_string();
            assert!(msg.contains("api_key"));
            assert!(!msg.contains(QUERY_AUTH_SENTINEL));
        }
        other => panic!("expected auth error, got {other:?}"),
    }
    assert_eq!(sent.sent_count().await, 0);
    let events = events.lock().await.clone();
    assert!(!events.iter().any(|event| event == "rate_acquire"));
    assert!(!events.iter().any(|event| event == "transport"));
}

#[tokio::test]
async fn custom_pagination_cannot_override_pending_auth_header_material() {
    const HEADER_AUTH_SENTINEL: &str = "RAW_HEADER_AUTH_SENTINEL";

    let events = Arc::new(Mutex::new(Vec::new()));
    let rate_limiter = Arc::new(RecordingRateLimiter::new(events.clone()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "should-not-send")],
    );
    let sent = transport.clone();
    let mut client = client(
        TestAuthVars {
            token: Some(HEADER_AUTH_SENTINEL.to_string()),
            identity: "header",
        },
        transport,
    );
    configure_runtime(&mut client, Some(rate_limiter));

    let policy = auth_policy(AuthPlacement::Header("X-Api-Key"));
    let endpoint = PaginationEndpoint {
        name: "Items",
        path: "/items",
        policy,
        pagination: Some(PaginationPlan::custom::<
            AuthHeaderCollisionPagination,
            Vec<String>,
        >()),
    };

    let err = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(2))
        .collect()
        .await
        .expect_err("auth header collision should fail before rate-limit or transport");

    match err {
        ApiClientError::Auth { source, .. } => {
            assert_eq!(
                source.kind,
                concord_core::advanced::AuthErrorKind::InvalidConfiguration
            );
            let msg = source.to_string();
            assert!(msg.contains("x-api-key"));
            assert!(!msg.contains(HEADER_AUTH_SENTINEL));
        }
        other => panic!("expected auth error, got {other:?}"),
    }
    assert_eq!(sent.sent_count().await, 0);
    let events = events.lock().await.clone();
    assert!(!events.iter().any(|event| event == "rate_acquire"));
    assert!(!events.iter().any(|event| event == "transport"));
}

#[tokio::test]
async fn mutation_error_is_raw_secret_free() {
    const RAW_BEARER_SENTINEL: &str = "RAW_BEARER_SENTINEL";
    const RAW_QUERY_AUTH_SENTINEL: &str = "RAW_QUERY_AUTH_SENTINEL";
    const RAW_HEADER_AUTH_SENTINEL: &str = "RAW_HEADER_AUTH_SENTINEL";
    const RAW_BASIC_USERNAME_SENTINEL: &str = "RAW_BASIC_USERNAME_SENTINEL";
    const RAW_BASIC_PASSWORD_SENTINEL: &str = "RAW_BASIC_PASSWORD_SENTINEL";

    fn assert_secret_free(err: &ApiClientError, secrets: &[&str]) {
        let display = err.to_string();
        let debug = format!("{:?}", err);
        let context = format!("{} {}", err.method(), err.endpoint());
        let mut source = std::error::Error::source(err);
        let mut chain = String::new();
        while let Some(err) = source {
            chain.push_str(&err.to_string());
            chain.push('\n');
            source = std::error::Error::source(err);
        }
        let source = chain;
        for secret in secrets {
            assert!(
                !display.contains(secret),
                "display leaked {secret}: {display}"
            );
            assert!(!debug.contains(secret), "debug leaked {secret}: {debug}");
            assert!(
                !context.contains(secret),
                "context leaked {secret}: {context}"
            );
            assert!(
                !source.contains(secret),
                "source chain leaked {secret}: {source}"
            );
        }
    }

    let query_err = {
        let events = Arc::new(Mutex::new(Vec::new()));
        let rate_limiter = Arc::new(RecordingRateLimiter::new(events.clone()));
        let transport = MockTransport::new(
            events.clone(),
            vec![MockResponse::text(StatusCode::OK, "should-not-send")],
        );
        let mut client = client(
            TestAuthVars {
                token: Some(RAW_QUERY_AUTH_SENTINEL.to_string()),
                identity: "query",
            },
            transport,
        );
        configure_runtime(&mut client, Some(rate_limiter));
        let policy = auth_policy(AuthPlacement::Query("api_key"));
        let endpoint = PaginationEndpoint {
            name: "Items",
            path: "/items",
            policy,
            pagination: Some(PaginationPlan::custom::<
                AuthQueryCollisionPagination,
                Vec<String>,
            >()),
        };
        client
            .request(endpoint)
            .paginate(PaginationTermination::hard_page_cap(2))
            .collect()
            .await
            .expect_err("query auth collision should fail")
    };
    assert_secret_free(&query_err, &[RAW_QUERY_AUTH_SENTINEL]);

    let header_err = {
        let events = Arc::new(Mutex::new(Vec::new()));
        let rate_limiter = Arc::new(RecordingRateLimiter::new(events.clone()));
        let transport = MockTransport::new(
            events.clone(),
            vec![MockResponse::text(StatusCode::OK, "should-not-send")],
        );
        let mut client = client(
            TestAuthVars {
                token: Some(RAW_HEADER_AUTH_SENTINEL.to_string()),
                identity: "header",
            },
            transport,
        );
        configure_runtime(&mut client, Some(rate_limiter));
        let policy = auth_policy(AuthPlacement::Header("X-Api-Key"));
        let endpoint = PaginationEndpoint {
            name: "Items",
            path: "/items",
            policy,
            pagination: Some(PaginationPlan::custom::<
                AuthHeaderCollisionPagination,
                Vec<String>,
            >()),
        };
        client
            .request(endpoint)
            .paginate(PaginationTermination::hard_page_cap(2))
            .collect()
            .await
            .expect_err("header auth collision should fail")
    };
    assert_secret_free(&header_err, &[RAW_HEADER_AUTH_SENTINEL]);

    let bearer_err = {
        let events = Arc::new(Mutex::new(Vec::new()));
        let rate_limiter = Arc::new(RecordingRateLimiter::new(events.clone()));
        let transport = MockTransport::new(
            events.clone(),
            vec![MockResponse::text(StatusCode::OK, "should-not-send")],
        );
        let mut client = client(
            TestAuthVars {
                token: Some(RAW_BEARER_SENTINEL.to_string()),
                identity: "bearer",
            },
            transport,
        );
        configure_runtime(&mut client, Some(rate_limiter));
        let policy = auth_policy(AuthPlacement::Bearer);
        let endpoint = PaginationEndpoint {
            name: "Items",
            path: "/items",
            policy,
            pagination: Some(PaginationPlan::custom::<
                AuthorizationCollisionPagination,
                Vec<String>,
            >()),
        };
        client
            .request(endpoint)
            .paginate(PaginationTermination::hard_page_cap(2))
            .collect()
            .await
            .expect_err("bearer auth collision should fail")
    };
    assert_secret_free(&bearer_err, &[RAW_BEARER_SENTINEL]);

    let basic_err = {
        let events = Arc::new(Mutex::new(Vec::new()));
        let rate_limiter = Arc::new(RecordingRateLimiter::new(events.clone()));
        let transport = MockTransport::new(
            events.clone(),
            vec![MockResponse::text(StatusCode::OK, "should-not-send")],
        );
        let mut client = concord_core::prelude::ApiClient::<ObservationAuthCx, _>::with_transport(
            (),
            ObservationAuthVars::basic(
                RAW_BASIC_USERNAME_SENTINEL,
                RAW_BASIC_PASSWORD_SENTINEL,
                "basic",
                events.clone(),
            ),
            transport,
        );
        configure_runtime(&mut client, Some(rate_limiter));
        let mut policy = auth_policy(AuthPlacement::Basic);
        policy.headers.insert(
            http::header::AUTHORIZATION,
            HeaderValue::from_static("public-value"),
        );
        let endpoint = PaginationEndpoint {
            name: "Items",
            path: "/items",
            policy,
            pagination: Some(PaginationPlan::custom::<
                AuthorizationCollisionPagination,
                Vec<String>,
            >()),
        };
        client
            .request(endpoint)
            .paginate(PaginationTermination::hard_page_cap(2))
            .collect()
            .await
            .expect_err("basic auth collision should fail")
    };
    assert_secret_free(
        &basic_err,
        &[RAW_BASIC_USERNAME_SENTINEL, RAW_BASIC_PASSWORD_SENTINEL],
    );
}

#[tokio::test]
async fn mutation_runs_before_auth_collision_validation() {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let events_for_scope = events.clone();
    let runtime_events = Arc::new(Mutex::new(Vec::new()));
    let rate_limiter = Arc::new(RecordingRateLimiter::new(runtime_events.clone()));
    let transport = MockTransport::new(
        runtime_events.clone(),
        vec![MockResponse::text(StatusCode::OK, "should-not-send")],
    );
    let sent = transport.clone();
    let mut client = client(
        TestAuthVars {
            token: Some("AUTH_COLLISION_SENTINEL".to_string()),
            identity: "query",
        },
        transport,
    );
    configure_runtime(&mut client, Some(rate_limiter));
    let policy = auth_policy(AuthPlacement::Query("api_key"));
    let endpoint = PaginationEndpoint {
        name: "Items",
        path: "/items",
        policy,
        pagination: Some(PaginationPlan::custom::<
            TracedAuthQueryCollisionPagination,
            Vec<String>,
        >()),
    };

    let err = PR69_MUTATION_EVENTS
        .scope(events_for_scope, async move {
            client
                .request(endpoint)
                .paginate(PaginationTermination::hard_page_cap(2))
                .collect()
                .await
        })
        .await
        .expect_err("auth collision should fail after mutation");

    assert!(matches!(err, ApiClientError::Auth { .. }));
    events
        .lock()
        .expect("mutation events lock")
        .push("auth_collision_validation_error");
    assert_eq!(
        &*events.lock().expect("mutation events lock"),
        &["mutation", "auth_collision_validation_error"]
    );
    assert_eq!(sent.sent_count().await, 0);
    let runtime_events = runtime_events.lock().await.clone();
    assert!(!runtime_events.iter().any(|event| event == "rate_acquire"));
    assert!(!runtime_events.iter().any(|event| event == "transport"));
}

#[tokio::test]
async fn final_sanitized_url_after_mutation_used_by_rate_limit_and_transport()
-> Result<(), ApiClientError> {
    const QUERY_AUTH_SENTINEL: &str = "RAW_QUERY_AUTH_SENTINEL";

    let rate_urls = Arc::new(Mutex::new(Vec::new()));
    let rate_limiter = Arc::new(RecordingSanitizedUrlRateLimiter::new(rate_urls.clone()));
    let transport = MockTransport::new(
        Arc::new(Mutex::new(Vec::new())),
        vec![MockResponse::text(StatusCode::OK, "ok")],
    );
    let sent = transport.clone();
    let mut client = client(
        TestAuthVars {
            token: Some(QUERY_AUTH_SENTINEL.to_string()),
            identity: "query",
        },
        transport,
    );
    configure_runtime(&mut client, Some(rate_limiter));
    let policy = auth_policy(AuthPlacement::Query("api_key"));
    let endpoint = PaginationEndpoint {
        name: "Items",
        path: "/items",
        policy,
        pagination: Some(PaginationPlan::custom::<
            PublicMutationPagination,
            Vec<String>,
        >()),
    };

    let items = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(2))
        .collect()
        .await?;

    assert_eq!(items, vec!["ok".to_string()]);
    assert_eq!(sent.sent_count().await, 1);
    let rate_url = rate_urls.lock().await.clone();
    assert_eq!(rate_url.len(), 1);
    assert!(rate_url[0].contains("page=7"));
    let requests = sent.requests().await;
    let transport_debug = format!("{:?}", requests[0]);
    assert!(transport_debug.contains("page=7"));
    assert!(rate_url[0].contains("api_key=<redacted>"));
    assert!(transport_debug.contains("api_key=<redacted>"));
    assert!(!rate_url[0].contains(QUERY_AUTH_SENTINEL));
    assert!(!transport_debug.contains(QUERY_AUTH_SENTINEL));
    assert_eq!(
        query_value(&requests[0].url, "api_key"),
        Some(QUERY_AUTH_SENTINEL.to_string())
    );
    Ok(())
}

#[tokio::test]
async fn public_query_and_header_mutation_still_reaches_transport() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let rate_limiter = Arc::new(RecordingRateLimiter::new(events.clone()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "ok")],
    );
    let sent = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    configure_runtime(&mut client, Some(rate_limiter));

    let endpoint = PaginationEndpoint {
        name: "Items",
        path: "/items",
        policy: Default::default(),
        pagination: Some(PaginationPlan::custom::<
            PublicMutationPagination,
            Vec<String>,
        >()),
    };

    let items = client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(2))
        .collect()
        .await?;

    assert_eq!(items, vec!["ok".to_string()]);
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(query_value(&requests[0].url, "page"), Some("7".to_string()));
    assert_eq!(
        requests[0]
            .headers
            .get("x-page-token")
            .and_then(|v| v.to_str().ok()),
        Some("7")
    );
    let events = events.lock().await.clone();
    assert!(events.iter().any(|event| event == "rate_acquire"));
    assert!(events.iter().any(|event| event == "transport"));
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

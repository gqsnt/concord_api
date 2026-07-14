use super::{
    RegressionEndpoint, RegressionIntoPlan, RegressionPaginatedEndpoint, RegressionPlanContext,
    RegressionResponseTerminal,
};
use crate::client::{ApiClient, ClientContext};
use crate::debug::DebugLevel;
use crate::error::{ApiClientError, ErrorContext};
use crate::pagination::{
    PageAdvance, PageApply, PageDecision, PageItems, PaginationRuntime, PaginationTermination,
    ProgressKey,
};
use crate::stream_response::StreamResponse;
use crate::timeout::TimeoutOverride;
use crate::transport::DecodedResponse;
use std::future::{Future, IntoFuture};
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::time::Duration;

#[derive(Clone, Copy, Debug)]
pub(crate) struct RequestOptions {
    debug_level: Option<DebugLevel>,
    timeout_override: TimeoutOverride,
}

impl Default for RequestOptions {
    fn default() -> Self {
        Self {
            debug_level: None,
            timeout_override: TimeoutOverride::Inherit,
        }
    }
}

impl RequestOptions {
    fn apply_to(&self, plan: &mut crate::endpoint::RequestPlan, page_index: u32) {
        plan.overrides.timeout = match self.timeout_override {
            TimeoutOverride::Inherit => plan.overrides.timeout,
            TimeoutOverride::Clear => None,
            TimeoutOverride::Set(duration) => Some(duration),
        };
        plan.overrides.debug_level = self.debug_level;
        plan.overrides.page_index = page_index;
    }
}

pub struct PendingRequest<'a, Cx: ClientContext, E: RegressionIntoPlan<Cx>> {
    client: &'a ApiClient<Cx>,
    endpoint: E,
    options: RequestOptions,
}

impl<'a, Cx: ClientContext, E: RegressionIntoPlan<Cx>> PendingRequest<'a, Cx, E> {
    pub(crate) fn new(client: &'a ApiClient<Cx>, endpoint: E) -> Self {
        Self {
            client,
            endpoint,
            options: RequestOptions::default(),
        }
    }

    pub fn debug_level(mut self, level: DebugLevel) -> Self {
        self.options.debug_level = Some(level);
        self
    }

    pub fn timeout(mut self, duration: Duration) -> Self {
        self.options.timeout_override = TimeoutOverride::Set(duration);
        self
    }

    pub async fn execute(self) -> Result<E::Response, ApiClientError> {
        let client = self.client;
        let plan = self.request_plan()?;
        E::execute(client, plan).await
    }

    pub async fn response(self) -> Result<DecodedResponse<E::Response>, ApiClientError>
    where
        E: RegressionResponseTerminal<Cx>,
    {
        let client = self.client;
        let plan = self.request_plan()?;
        E::execute_response(client, plan).await
    }

    #[cfg(feature = "dangerous-raw-response")]
    pub async fn execute_raw_response(
        self,
    ) -> Result<crate::dangerous::BuiltResponse, ApiClientError> {
        let client = self.client;
        let plan = self.request_plan()?;
        client.execute_plan_raw(plan).await
    }

    fn request_plan(self) -> Result<crate::endpoint::RequestPlan, ApiClientError> {
        let Self {
            client,
            endpoint,
            options,
        } = self;
        let context = RegressionPlanContext {
            vars: client.vars(),
            auth_vars: client.auth_vars(),
        };
        let mut plan = endpoint.into_plan(&context)?;
        options.apply_to(&mut plan, 0);
        Ok(plan)
    }

    pub fn paginate(
        self,
        termination: PaginationTermination,
    ) -> crate::request::PaginatedRequest<'a, Cx, PaginationRegressionAdapter<E>>
    where
        E: RegressionPaginatedEndpoint<Cx>,
        E::Response: PageItems,
    {
        let pending = crate::request::PendingRequest::new(
            self.client,
            PaginationRegressionAdapter(self.endpoint),
        );
        pending.paginate(termination)
    }
}

impl<'a, Cx, E, M> PendingRequest<'a, Cx, E>
where
    Cx: ClientContext + 'a,
    E: RegressionIntoPlan<Cx> + RegressionEndpoint<Cx, Response = StreamResponse<M>> + 'a,
    M: 'a,
{
    pub async fn execute_stream(self) -> Result<StreamResponse<M>, ApiClientError> {
        let client = self.client;
        let plan = self.request_plan()?;
        E::execute(client, plan).await
    }
}

impl<'a, Cx, E> IntoFuture for PendingRequest<'a, Cx, E>
where
    Cx: ClientContext + 'a,
    E: RegressionIntoPlan<Cx> + 'a,
    E::Response: 'a,
{
    type Output = Result<E::Response, ApiClientError>;
    type IntoFuture = Pin<Box<dyn Future<Output = Self::Output> + Send + 'a>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(self.execute())
    }
}

pub(crate) struct PaginationRegressionAdapter<E>(E);

impl<Cx, E> crate::endpoint::GeneratedEndpoint<Cx> for PaginationRegressionAdapter<E>
where
    Cx: ClientContext,
    E: RegressionPaginatedEndpoint<Cx>,
    E::Response: PageItems,
{
    type Response = E::Response;
}

impl<Cx, E> crate::endpoint::GeneratedReusableEndpoint<Cx> for PaginationRegressionAdapter<E>
where
    Cx: ClientContext,
    E: RegressionPaginatedEndpoint<Cx>,
    E::Response: PageItems,
{
    fn plan(
        &self,
        context: &crate::__private::GeneratedPlanContext<'_, Cx>,
    ) -> Result<crate::__private::GeneratedPreparedCall<Cx, Self::Response>, ApiClientError> {
        let context = RegressionPlanContext {
            vars: context.vars(),
            auth_vars: context.auth_vars(),
        };
        let plan = self.0.plan(&context)?;
        Ok(crate::__private::prepared_call_for_core_regression(
            plan,
            E::execute,
        ))
    }
}

impl<E, P> crate::pagination::PaginateBinding<P> for PaginationRegressionAdapter<E>
where
    E: crate::pagination::PaginateBinding<P>,
{
    fn load_pagination(&self) -> P {
        self.0.load_pagination()
    }

    fn store_pagination(&mut self, pagination: &P) {
        self.0.store_pagination(pagination);
    }
}

impl<Cx, E> crate::endpoint::GeneratedPaginatedEndpoint<Cx> for PaginationRegressionAdapter<E>
where
    Cx: ClientContext,
    E: RegressionPaginatedEndpoint<Cx>,
    E::Response: PageItems,
{
    type Pagination = E::Pagination;

    fn pagination_runtime(&self) -> Option<Box<dyn PaginationRuntime<Self, Self::Response>>> {
        self.0
            .pagination_runtime()
            .map(|runtime| Box::new(PaginationRuntimeBridge(runtime)) as Box<_>)
    }
}

struct PaginationRuntimeBridge<E, Page>(Box<dyn PaginationRuntime<E, Page>>)
where
    Page: PageItems;

impl<E, Page> PaginationRuntime<PaginationRegressionAdapter<E>, Page>
    for PaginationRuntimeBridge<E, Page>
where
    Page: PageItems,
{
    fn init(
        &mut self,
        endpoint: &PaginationRegressionAdapter<E>,
        context: PageApply<'_>,
    ) -> Result<(), ApiClientError> {
        self.0.init(&endpoint.0, context)
    }

    fn apply(
        &mut self,
        endpoint: &mut PaginationRegressionAdapter<E>,
        context: PageApply<'_>,
    ) -> Result<(), ApiClientError> {
        self.0.apply(&mut endpoint.0, context)
    }

    fn advance(
        &mut self,
        endpoint: &mut PaginationRegressionAdapter<E>,
        error_context: &ErrorContext,
        page: &Page,
        page_context: PageAdvance<'_>,
    ) -> Result<PageDecision, ApiClientError> {
        self.0
            .advance(&mut endpoint.0, error_context, page, page_context)
    }

    fn expected_items_per_page(&self) -> Option<NonZeroUsize> {
        self.0.expected_items_per_page()
    }

    fn progress_key(&self) -> Option<ProgressKey> {
        self.0.progress_key()
    }
}

use crate::client::{ApiClient, ClientContext};
use crate::error::ApiClientError;
use crate::transport::DecodedResponse;
use std::future::Future;
use std::pin::Pin;

#[allow(unused_imports)]
pub(crate) use crate::endpoint::{
    EndpointMeta, EndpointPlan, PaginationMarker, RequestOverrides, RequestPlan, RequestPlanView,
    ResolvedRoute, ResponsePlan,
};

pub struct RegressionPlanContext<'a, Cx: ClientContext> {
    pub vars: &'a Cx::Vars,
    pub auth_vars: &'a Cx::AuthVars,
}

#[doc(hidden)]
pub type EndpointFuture<'a, Output> =
    Pin<Box<dyn Future<Output = Result<Output, ApiClientError>> + Send + 'a>>;

/// Crate-local endpoint model used only by the Core regression suites.
pub trait RegressionEndpoint<Cx: ClientContext>: Send + Sized + 'static {
    type Response: Send + 'static;

    /// Executes a planned endpoint through its typed response path.
    ///
    /// Generated endpoints implement this with their resolved response entity.
    /// Manual endpoints must provide the corresponding typed execution path.
    fn execute<'a>(
        client: &'a ApiClient<Cx>,
        plan: RequestPlan,
    ) -> EndpointFuture<'a, Self::Response>;
}

/// Marker for endpoints that expose a metadata-bearing decoded response terminal.
///
/// Generated buffered endpoints implement this with their resolved response
/// adapter so callers cannot choose a response codec at the call site.
#[doc(hidden)]
pub trait RegressionResponseTerminal<Cx: ClientContext>: RegressionEndpoint<Cx> {
    fn execute_response<'a>(
        client: &'a ApiClient<Cx>,
        plan: RequestPlan,
    ) -> EndpointFuture<'a, DecodedResponse<Self::Response>>;
}

/// Regression planning for reusable bodyless endpoints.
///
/// Implement this for endpoints that may be planned multiple times by shared
/// reference, such as paginated endpoints.
pub trait RegressionReusableEndpoint<Cx: ClientContext>: RegressionEndpoint<Cx> + Sync {
    fn plan(&self, ctx: &RegressionPlanContext<'_, Cx>) -> Result<RequestPlan, ApiClientError>;
}

/// Regression planning that consumes the endpoint value.
pub trait RegressionIntoPlan<Cx: ClientContext>: RegressionEndpoint<Cx> {
    fn into_plan(self, ctx: &RegressionPlanContext<'_, Cx>) -> Result<RequestPlan, ApiClientError>;
}

impl<Cx, E> RegressionIntoPlan<Cx> for E
where
    Cx: ClientContext,
    E: RegressionReusableEndpoint<Cx>,
{
    fn into_plan(self, ctx: &RegressionPlanContext<'_, Cx>) -> Result<RequestPlan, ApiClientError> {
        self.plan(ctx)
    }
}

/// Marker implemented only for endpoints that declare pagination.
///
/// A response type implementing [`crate::pagination::PageItems`] is not enough
/// to make an endpoint paginated; the endpoint plan must also carry an
/// explicit pagination controller.
pub trait RegressionPaginatedEndpoint<Cx: ClientContext>: RegressionReusableEndpoint<Cx>
where
    Self: crate::pagination::PaginateBinding<Self::Pagination>,
    Self::Pagination: crate::pagination::EndpointPagination<Self::Response>,
    Self::Response: crate::pagination::PageItems,
{
    type Pagination;

    #[doc(hidden)]
    fn pagination_runtime(
        &self,
    ) -> Option<Box<dyn crate::pagination::PaginationRuntime<Self, Self::Response>>>
    where
        Self: Sized,
    {
        Some(Box::new(crate::pagination::PaginationRuntimeAdapter::<
            Self::Pagination,
        >::new()))
    }
}

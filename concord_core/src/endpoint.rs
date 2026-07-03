use crate::client::{ApiClient, ClientContext};
use crate::error::ApiClientError;
use crate::transport::Transport;
use std::future::Future;
use std::pin::Pin;

pub mod plan;
#[allow(unused_imports)]
pub use plan::{
    AttemptState, BodyPlan, EndpointMeta, EndpointPlan, PaginationMarker, RequestArgs,
    RequestOverrides, RequestPlan, RequestPlanView, ResolvedRoute, ResponsePlan,
};

pub struct ClientPlanContext<'a, Cx: ClientContext> {
    pub vars: &'a Cx::Vars,
    pub auth_vars: &'a Cx::AuthVars,
}

/// Endpoint model used by generated Concord clients.
pub trait Endpoint<Cx: ClientContext>: Send + Sync + Sized + 'static {
    type Response: Send + 'static;

    fn plan(&self, ctx: &ClientPlanContext<'_, Cx>) -> Result<RequestPlan, ApiClientError>;

    /// Executes a planned endpoint through its typed response path.
    ///
    /// Generated endpoints implement this with their resolved response entity.
    /// Manual endpoints must provide the corresponding typed execution path.
    fn execute<'a, T>(
        client: &'a ApiClient<Cx, T>,
        plan: RequestPlan,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Response, ApiClientError>> + Send + 'a>>
    where
        T: Transport + 'a;
}

/// Marker implemented only for endpoints that declare pagination.
///
/// A response type implementing [`crate::pagination::PageItems`] is not enough
/// to make an endpoint paginated; the endpoint plan must also carry an
/// explicit pagination controller.
pub trait PaginatedEndpoint<Cx: ClientContext>: Endpoint<Cx>
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

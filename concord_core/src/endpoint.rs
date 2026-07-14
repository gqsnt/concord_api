use crate::client::ClientContext;
use crate::error::ApiClientError;
use std::future::Future;
use std::pin::Pin;

pub mod plan;
#[allow(unused_imports)]
pub use plan::{
    EndpointMeta, EndpointPlan, PaginationMarker, RequestOverrides, RequestPlan, RequestPlanView,
    ResolvedRoute, ResponsePlan,
};

#[doc(hidden)]
pub type EndpointFuture<'a, Output> =
    Pin<Box<dyn Future<Output = Result<Output, ApiClientError>> + Send + 'a>>;

/// Endpoint model used by generated Concord clients.
#[doc(hidden)]
pub trait GeneratedEndpoint<Cx: ClientContext>: Send + Sized + 'static {
    type Response: Send + 'static;
}

/// Marker for endpoints that expose a metadata-bearing decoded response terminal.
///
/// Generated buffered endpoints implement this with their resolved response
/// adapter so callers cannot choose a response codec at the call site.
#[doc(hidden)]
pub trait GeneratedResponseTerminalEndpoint<Cx: ClientContext>: GeneratedEndpoint<Cx> {}

/// Endpoint planning for reusable bodyless endpoints.
///
/// Implement this for endpoints that may be planned multiple times by shared
/// reference, such as paginated endpoints.
#[doc(hidden)]
pub trait GeneratedReusableEndpoint<Cx: ClientContext>: GeneratedEndpoint<Cx> + Sync {
    fn plan(
        &self,
        ctx: &crate::__private::GeneratedPlanContext<'_, Cx>,
    ) -> Result<crate::__private::GeneratedPreparedCall<Cx, Self::Response>, ApiClientError>;
}

/// Endpoint planning that consumes the endpoint value.
#[doc(hidden)]
pub trait GeneratedIntoPreparedCall<Cx: ClientContext>: GeneratedEndpoint<Cx> {
    fn into_plan(
        self,
        ctx: &crate::__private::GeneratedPlanContext<'_, Cx>,
    ) -> Result<crate::__private::GeneratedPreparedCall<Cx, Self::Response>, ApiClientError>;
}

impl<Cx, E> GeneratedIntoPreparedCall<Cx> for E
where
    Cx: ClientContext,
    E: GeneratedReusableEndpoint<Cx>,
{
    fn into_plan(
        self,
        ctx: &crate::__private::GeneratedPlanContext<'_, Cx>,
    ) -> Result<crate::__private::GeneratedPreparedCall<Cx, Self::Response>, ApiClientError> {
        self.plan(ctx)
    }
}

/// Marker implemented only for endpoints that declare pagination.
///
/// A response type implementing [`crate::pagination::PageItems`] is not enough
/// to make an endpoint paginated; the endpoint plan must also carry an
/// explicit pagination controller.
pub trait GeneratedPaginatedEndpoint<Cx: ClientContext>: GeneratedReusableEndpoint<Cx>
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

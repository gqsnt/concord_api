use crate::client::{ApiClient, ClientContext};
use crate::codec::ResponseCodec;
use crate::error::ApiClientError;
use crate::transport::DecodedResponse;
use crate::transport::Transport;
use std::future::Future;
use std::marker::PhantomData;
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

pub trait ResponseSpec: Send + Sync + 'static {
    type Decoded: Send + 'static; // interne
    type Output: Send + 'static; // public
    type Dec: ResponseCodec<Value = Self::Decoded>;

    fn accept_content_type() -> Option<::http::HeaderValue> {
        <Self::Dec as ResponseCodec>::accept()
    }
    fn is_no_content() -> bool {
        <Self::Dec as ResponseCodec>::is_no_content()
    }

    fn map_response(
        resp: DecodedResponse<Self::Decoded>,
    ) -> Result<DecodedResponse<Self::Output>, crate::error::FxError>;
}

/// Helper générique : (decoder, type)
pub struct Decoded<Dec, T>(PhantomData<(Dec, T)>);

impl<Dec, T> ResponseSpec for Decoded<Dec, T>
where
    Dec: ResponseCodec<Value = T> + Send + Sync + 'static,
    T: Send + Sync + 'static,
{
    type Decoded = T;
    type Output = T;
    type Dec = Dec;

    fn map_response(resp: DecodedResponse<T>) -> Result<DecodedResponse<T>, crate::error::FxError> {
        Ok(resp)
    }
}

pub trait Transform<T>: Send + Sync + 'static {
    type Out: Send + 'static;
    fn map(v: T) -> Result<Self::Out, crate::error::FxError>;
}

pub struct Mapped<R, M>(PhantomData<(R, M)>);

impl<R, M> ResponseSpec for Mapped<R, M>
where
    R: ResponseSpec,
    M: Transform<R::Decoded>,
{
    type Decoded = R::Decoded;
    type Output = M::Out;
    type Dec = R::Dec;

    fn map_response(
        resp: DecodedResponse<Self::Decoded>,
    ) -> Result<DecodedResponse<Self::Output>, crate::error::FxError> {
        let DecodedResponse {
            meta,
            url,
            status,
            headers,
            value,
        } = resp;
        let out = M::map(value)?;
        Ok(DecodedResponse {
            meta,
            url,
            status,
            headers,
            value: out,
        })
    }
}

pub trait TransformResp<T>: Send + Sync + 'static {
    type Out: Send + 'static;
    fn map(resp: DecodedResponse<T>) -> Result<DecodedResponse<Self::Out>, crate::error::FxError>;
}

pub struct MappedResp<R, M>(PhantomData<(R, M)>);
impl<R, M> ResponseSpec for MappedResp<R, M>
where
    R: ResponseSpec,
    M: TransformResp<R::Decoded>,
{
    type Decoded = R::Decoded;
    type Output = M::Out;
    type Dec = R::Dec;
    fn map_response(
        resp: DecodedResponse<Self::Decoded>,
    ) -> Result<DecodedResponse<Self::Output>, crate::error::FxError> {
        M::map(resp)
    }
}

/// Endpoint model used by generated Concord clients.
pub trait Endpoint<Cx: ClientContext>: Send + Sync + Sized + 'static {
    type Response: Send + 'static;

    fn plan(&self, ctx: &ClientPlanContext<'_, Cx>) -> Result<RequestPlan, ApiClientError>;

    fn execute<'a, T>(
        client: &'a ApiClient<Cx, T>,
        plan: RequestPlan,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Response, ApiClientError>> + Send + 'a>>
    where
        T: Transport + 'a,
    {
        Box::pin(async move { Ok(client.execute_plan::<Self::Response>(plan).await?.value) })
    }
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

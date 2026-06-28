use crate::client::{ApiClient, ClientContext};
use crate::error::ApiClientError;
use crate::media::MediaType;
use crate::multipart::MultipartFormat;
use crate::multipart_response::{MultipartDecodePart, MultipartStream};
use crate::record::{RecordFormat, RecordStream};
use crate::sse::{SseCodec, SseStream};
use crate::stream_response::StreamResponse;
use crate::transport::DecodedResponse;
use crate::transport::Transport;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;

pub mod plan;
#[allow(unused_imports)]
pub use plan::{
    AttemptState, BodyPlan, CursorNextFn, CustomPaginationPlan, EndpointMeta, EndpointPlan,
    PaginationPlan, RequestArgs, RequestOverrides, RequestPlan, RequestPlanView, ResolvedRoute,
    ResponsePlan,
};

pub struct ClientPlanContext<'a, Cx: ClientContext> {
    pub vars: &'a Cx::Vars,
    pub auth_vars: &'a Cx::AuthVars,
}

pub trait ResponseSpec: Send + Sync + 'static {
    type Decoded: Send + 'static; // interne
    type Output: Send + 'static; // public
    type Dec: crate::codec::Decodes<Self::Decoded>;

    fn accept_content_type() -> &'static str {
        <Self::Dec as crate::codec::ContentType>::CONTENT_TYPE
    }
    fn is_no_content() -> bool {
        <Self::Dec as crate::codec::ContentType>::IS_NO_CONTENT
    }

    fn map_response(
        resp: DecodedResponse<Self::Decoded>,
    ) -> Result<DecodedResponse<Self::Output>, crate::error::FxError>;
}

/// Helper générique : (decoder, type)
pub struct Decoded<Dec, T>(PhantomData<(Dec, T)>);

impl<Dec, T> ResponseSpec for Decoded<Dec, T>
where
    Dec: crate::codec::Decodes<T> + Send + Sync + 'static,
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
pub trait PaginatedEndpoint<Cx: ClientContext>: Endpoint<Cx> {}

/// Marker implemented only for endpoints whose primary response is a stream.
pub trait StreamResponseEndpoint<Cx: ClientContext>: Endpoint<Cx> {
    type Media: MediaType;

    fn execute_stream<'a, T>(
        client: &'a ApiClient<Cx, T>,
        plan: RequestPlan,
    ) -> Pin<
        Box<dyn Future<Output = Result<StreamResponse<Self::Media>, ApiClientError>> + Send + 'a>,
    >
    where
        T: Transport + 'a,
    {
        Box::pin(async move { client.execute_plan_stream::<Self::Media>(plan).await })
    }
}

/// Marker implemented only for endpoints whose primary response is a record stream.
pub trait RecordResponseEndpoint<Cx: ClientContext>: Endpoint<Cx> {
    type Item: Send + 'static;
    type Format: RecordFormat<Self::Item>;

    fn execute_records<'a, T>(
        client: &'a ApiClient<Cx, T>,
        plan: RequestPlan,
    ) -> Pin<Box<dyn Future<Output = Result<RecordStream<Self::Item>, ApiClientError>> + Send + 'a>>
    where
        T: Transport + 'a,
    {
        Box::pin(async move {
            client
                .execute_plan_records::<Self::Item, Self::Format>(plan)
                .await
        })
    }
}

/// Marker implemented only for endpoints whose primary response is multipart.
pub trait MultipartResponseEndpoint<Cx: ClientContext>: Endpoint<Cx> {
    type Part: MultipartDecodePart<Self::Format>;
    type Format: MultipartFormat;

    fn execute_multipart<'a, T>(
        client: &'a ApiClient<Cx, T>,
        plan: RequestPlan,
    ) -> Pin<
        Box<dyn Future<Output = Result<MultipartStream<Self::Part>, ApiClientError>> + Send + 'a>,
    >
    where
        T: Transport + 'a,
    {
        Box::pin(async move {
            client
                .execute_plan_multipart::<Self::Part, Self::Format>(plan)
                .await
        })
    }
}

/// Marker implemented only for endpoints whose primary response is SSE.
pub trait SseResponseEndpoint<Cx: ClientContext>: Endpoint<Cx> {
    type Event: Send + 'static;
    type Codec: SseCodec<Self::Event>;

    fn execute_sse<'a, T>(
        client: &'a ApiClient<Cx, T>,
        plan: RequestPlan,
    ) -> Pin<Box<dyn Future<Output = Result<SseStream<Self::Event>, ApiClientError>> + Send + 'a>>
    where
        T: Transport + 'a,
        Self::Event: Send + 'static,
    {
        Box::pin(async move {
            client
                .execute_plan_sse::<Self::Event, Self::Codec>(plan)
                .await
        })
    }
}

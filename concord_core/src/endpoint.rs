use crate::client::ClientContext;
use crate::error::ApiClientError;
use crate::transport::DecodedResponse;
use std::marker::PhantomData;

pub mod plan;
#[allow(unused_imports)]
pub use plan::{
    AttemptState, BodyPlan, EndpointMeta, EndpointPlan, PaginationPlan, RequestArgs,
    RequestOverrides, RequestPlan, ResolvedRoute, ResponsePlan,
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

/// Primary v4 endpoint model: generated endpoints produce a request plan.
pub trait Endpoint<Cx: ClientContext>: Send + Sync + Sized + 'static {
    type Response: Send + 'static;

    fn plan(&self, ctx: &ClientPlanContext<'_, Cx>) -> Result<RequestPlan, ApiClientError>;
}

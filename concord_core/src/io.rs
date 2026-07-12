use crate::client::{ApiClient, ClientContext};
use crate::codec::{BodyCodec, ContentType, DecodeContext, EncodeContext, ResponseCodec};
use crate::endpoint::{RequestPlan, ResponsePlan};
use crate::error::{ApiClientError, ErrorContext};
#[cfg(feature = "multipart")]
use crate::multipart::MultipartBody;
use crate::stream_body::StreamBody;
use crate::stream_response::StreamResponse;
use crate::transport::{BuiltResponse, DecodedResponse};
use bytes::Bytes;
use http::HeaderValue;
use http_body::{Body as _, SizeHint};
use std::fmt;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;

type BodyFactory = dyn Fn() -> Result<crate::body::DynBody, crate::body::BodyError> + Send + Sync;
type MediaBodyFactory =
    dyn Fn() -> Result<(crate::body::DynBody, HeaderValue), crate::body::BodyError> + Send + Sync;

enum PreparedBodyMediaType {
    Fixed(HeaderValue),
    Dynamic,
}

impl PreparedBodyMediaType {
    fn as_fixed(&self) -> Option<&HeaderValue> {
        match self {
            Self::Fixed(value) => Some(value),
            Self::Dynamic => None,
        }
    }
}

enum ReplayFactory {
    Fixed(std::sync::Arc<BodyFactory>),
    Dynamic(std::sync::Arc<MediaBodyFactory>),
}

enum PreparedBodySource {
    Empty,
    ReusableBytes(Bytes),
    OneShot(Option<crate::body::DynBody>),
    ReplayFactory(ReplayFactory),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BodyProductionErrorKind {
    AlreadyConsumed,
    FactoryFailure,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BodyProductionError {
    kind: BodyProductionErrorKind,
    body_error_kind: Option<crate::body::BodyErrorKind>,
}

impl BodyProductionError {
    pub(crate) fn kind(&self) -> BodyProductionErrorKind {
        self.kind
    }

    #[cfg(test)]
    pub(crate) fn body_error_kind(&self) -> Option<crate::body::BodyErrorKind> {
        self.body_error_kind
    }

    fn already_consumed() -> Self {
        Self {
            kind: BodyProductionErrorKind::AlreadyConsumed,
            body_error_kind: None,
        }
    }

    fn factory_failure(error: crate::body::BodyError) -> Self {
        Self {
            kind: BodyProductionErrorKind::FactoryFailure,
            body_error_kind: Some(error.kind()),
        }
    }
}

impl fmt::Display for BodyProductionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            BodyProductionErrorKind::AlreadyConsumed => {
                f.write_str("one-shot request body has already been consumed")
            }
            BodyProductionErrorKind::FactoryFailure => f.write_str("request body factory failed"),
        }
    }
}

impl std::error::Error for BodyProductionError {}

/// Request-local owner of request body production, metadata, and replayability.
pub struct PreparedBody {
    source: PreparedBodySource,
    media_type: Option<PreparedBodyMediaType>,
    size_hint: SizeHint,
}

pub(crate) struct ProducedBody {
    body: crate::body::DynBody,
    stream_like: bool,
    media_type: Option<HeaderValue>,
}

impl ProducedBody {
    pub(crate) fn is_stream(&self) -> bool {
        self.stream_like
    }

    pub(crate) fn into_dyn_body(self) -> crate::body::DynBody {
        self.body
    }

    pub(crate) fn media_type(&self) -> Option<&HeaderValue> {
        self.media_type.as_ref()
    }
}

impl fmt::Debug for ProducedBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProducedBody")
            .field("body", &self.body)
            .field("stream_like", &self.stream_like)
            .field("has_media_type", &self.media_type.is_some())
            .finish()
    }
}

impl PreparedBody {
    pub fn empty() -> Self {
        Self {
            source: PreparedBodySource::Empty,
            media_type: None,
            size_hint: exact_size_hint(0),
        }
    }

    pub fn reusable_bytes(bytes: Bytes, media_type: Option<HeaderValue>) -> Self {
        let size_hint = exact_size_hint(bytes.len() as u64);
        Self {
            source: PreparedBodySource::ReusableBytes(bytes),
            media_type: media_type.map(PreparedBodyMediaType::Fixed),
            size_hint,
        }
    }

    pub fn one_shot(body: crate::body::DynBody, media_type: Option<HeaderValue>) -> Self {
        let size_hint = body.size_hint();
        Self {
            source: PreparedBodySource::OneShot(Some(body)),
            media_type: media_type.map(PreparedBodyMediaType::Fixed),
            size_hint,
        }
    }

    pub fn replay_factory<F>(
        size_hint: SizeHint,
        media_type: Option<HeaderValue>,
        factory: F,
    ) -> Self
    where
        F: Fn() -> Result<crate::body::DynBody, crate::body::BodyError> + Send + Sync + 'static,
    {
        Self {
            source: PreparedBodySource::ReplayFactory(ReplayFactory::Fixed(std::sync::Arc::new(
                factory,
            ))),
            media_type: media_type.map(PreparedBodyMediaType::Fixed),
            size_hint,
        }
    }

    pub fn replay_factory_with_media<F>(size_hint: SizeHint, factory: F) -> Self
    where
        F: Fn() -> Result<(crate::body::DynBody, HeaderValue), crate::body::BodyError>
            + Send
            + Sync
            + 'static,
    {
        Self {
            source: PreparedBodySource::ReplayFactory(ReplayFactory::Dynamic(std::sync::Arc::new(
                factory,
            ))),
            media_type: Some(PreparedBodyMediaType::Dynamic),
            size_hint,
        }
    }

    pub fn from_stream_body(body: StreamBody, media_type: Option<HeaderValue>) -> Self {
        Self::one_shot(crate::body::DynBody::from_stream_body(body), media_type)
    }

    pub fn media_type(&self) -> Option<&HeaderValue> {
        self.media_type
            .as_ref()
            .and_then(PreparedBodyMediaType::as_fixed)
    }

    pub(crate) fn reserves_content_type(&self) -> bool {
        matches!(self.media_type, Some(PreparedBodyMediaType::Dynamic))
    }

    pub fn size_hint(&self) -> SizeHint {
        self.size_hint.clone()
    }

    pub fn is_replayable(&self) -> bool {
        matches!(
            self.source,
            PreparedBodySource::Empty
                | PreparedBodySource::ReusableBytes(_)
                | PreparedBodySource::ReplayFactory(_)
        )
    }

    pub(crate) fn supports_auth_internal_retries(&self) -> bool {
        matches!(
            self.source,
            PreparedBodySource::Empty | PreparedBodySource::ReusableBytes(_)
        )
    }

    pub(crate) fn produce_for_attempt(&mut self) -> Result<ProducedBody, BodyProductionError> {
        let size_hint = self.size_hint.clone();
        match &mut self.source {
            PreparedBodySource::Empty => Ok(ProducedBody {
                body: crate::body::DynBody::empty(),
                stream_like: false,
                media_type: None,
            }),
            PreparedBodySource::ReusableBytes(bytes) => Ok(ProducedBody {
                body: crate::body::DynBody::from_bytes(bytes.clone()),
                stream_like: false,
                media_type: self.media_type().cloned(),
            }),
            PreparedBodySource::OneShot(body) => body
                .take()
                .map(|body| ProducedBody {
                    body,
                    stream_like: true,
                    media_type: self.media_type().cloned(),
                })
                .ok_or_else(BodyProductionError::already_consumed),
            PreparedBodySource::ReplayFactory(factory) => match factory {
                ReplayFactory::Fixed(factory) => factory()
                    .map(|body| ProducedBody {
                        body: body.with_size_hint(size_hint),
                        stream_like: true,
                        media_type: self.media_type().cloned(),
                    })
                    .map_err(BodyProductionError::factory_failure),
                ReplayFactory::Dynamic(factory) => factory()
                    .map(|(body, media_type)| ProducedBody {
                        // The form owns its per-attempt hint.  In particular,
                        // never replace it with a planning hint (which is
                        // intentionally unknown for multipart replay).
                        body,
                        stream_like: true,
                        media_type: Some(media_type),
                    })
                    .map_err(BodyProductionError::factory_failure),
            },
        }
    }
}

pub(crate) fn apply_prepared_body_media_type(
    headers: &mut http::HeaderMap,
    body: &PreparedBody,
) -> Result<(), ()> {
    match body.media_type.as_ref() {
        None | Some(PreparedBodyMediaType::Fixed(_)) => {
            let media_type = body.media_type();
            if let Some(media_type) = media_type {
                if headers.contains_key(http::header::CONTENT_TYPE) {
                    return (headers.get(http::header::CONTENT_TYPE) == Some(media_type))
                        .then_some(())
                        .ok_or(());
                }
                headers.insert(http::header::CONTENT_TYPE, media_type.clone());
            }
            Ok(())
        }
        Some(PreparedBodyMediaType::Dynamic) => {
            if headers.contains_key(http::header::CONTENT_TYPE) {
                Err(())
            } else {
                Ok(())
            }
        }
    }
}

pub(crate) fn apply_attempt_body_media_type(
    headers: &mut http::HeaderMap,
    body: &ProducedBody,
) -> Result<(), ()> {
    let Some(media_type) = body.media_type() else {
        return Ok(());
    };
    match headers.get(http::header::CONTENT_TYPE) {
        Some(existing) if existing == media_type => Ok(()),
        Some(_) => Err(()),
        None => {
            headers.insert(http::header::CONTENT_TYPE, media_type.clone());
            Ok(())
        }
    }
}

impl fmt::Debug for PreparedBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let category = match self.source {
            PreparedBodySource::Empty => "Empty",
            PreparedBodySource::ReusableBytes(_) => "ReusableBytes",
            PreparedBodySource::OneShot(Some(_)) => "OneShot",
            PreparedBodySource::OneShot(None) => "OneShotConsumed",
            PreparedBodySource::ReplayFactory(_) => "ReplayFactory",
        };
        f.debug_struct("PreparedBody")
            .field("category", &category)
            .field("has_media_type", &self.media_type.is_some())
            .field("size_hint", &self.size_hint)
            .field("replayable", &self.is_replayable())
            .finish()
    }
}

fn exact_size_hint(len: u64) -> SizeHint {
    let mut hint = SizeHint::new();
    hint.set_exact(len);
    hint
}

#[derive(Debug)]
pub struct PreparedRequestEntity {
    pub body: PreparedBody,
}

pub trait RequestEntity {
    type Input;

    fn prepare(
        input: Self::Input,
        ctx: ErrorContext,
    ) -> Result<PreparedRequestEntity, ApiClientError>;
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NoRequestBody;

impl RequestEntity for NoRequestBody {
    type Input = ();

    fn prepare(
        _: Self::Input,
        _ctx: ErrorContext,
    ) -> Result<PreparedRequestEntity, ApiClientError> {
        Ok(PreparedRequestEntity {
            body: PreparedBody::empty(),
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EncodedRequest<C>(PhantomData<fn() -> C>);

impl<C> RequestEntity for EncodedRequest<C>
where
    C: BodyCodec,
{
    type Input = C::Value;

    fn prepare(
        input: Self::Input,
        ctx: ErrorContext,
    ) -> Result<PreparedRequestEntity, ApiClientError> {
        let encoded = C::encode(input, EncodeContext::new(ctx.endpoint, &ctx.method))
            .map_err(|_| ApiClientError::request_body_codec_error(ctx.clone()))?;
        let (bytes, _format) = encoded.into_parts();
        let content_type = C::try_content_type()
            .map_err(|_| ApiClientError::invalid_param(ctx.clone(), "content_type"))?;
        Ok(PreparedRequestEntity {
            body: PreparedBody::reusable_bytes(bytes, content_type),
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RawStreamRequest<M>(PhantomData<fn() -> M>);

impl<M> RequestEntity for RawStreamRequest<M>
where
    M: ContentType,
{
    type Input = StreamBody;

    fn prepare(
        input: Self::Input,
        ctx: ErrorContext,
    ) -> Result<PreparedRequestEntity, ApiClientError> {
        let content_type = M::try_header_value()
            .map_err(|_| ApiClientError::invalid_param(ctx.clone(), "content_type"))?;
        Ok(PreparedRequestEntity {
            body: PreparedBody::from_stream_body(input, Some(content_type)),
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg(feature = "multipart")]
pub struct MultipartRequest;

#[cfg(feature = "multipart")]
impl RequestEntity for MultipartRequest {
    type Input = MultipartBody;

    fn prepare(
        input: Self::Input,
        ctx: ErrorContext,
    ) -> Result<PreparedRequestEntity, ApiClientError> {
        let (content_type, body) = input
            .into_prepared()
            .map_err(|source| ApiClientError::codec_error(ctx.clone(), source))?;
        Ok(PreparedRequestEntity {
            body: PreparedBody::one_shot(body, Some(content_type)),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ResponseEntityCapabilities {
    pub supports_pagination: bool,
    pub is_streaming: bool,
    pub is_no_content: bool,
}

#[derive(Debug, Clone)]
pub struct ResponseEntityPlan {
    pub response_plan: ResponsePlan,
    pub capabilities: ResponseEntityCapabilities,
}

#[doc(hidden)]
pub type ResponseEntityFuture<'a, Output> =
    Pin<Box<dyn Future<Output = Result<Output, ApiClientError>> + Send + 'a>>;

pub trait ResponseEntity {
    type Output;

    fn plan(ctx: ErrorContext) -> Result<ResponseEntityPlan, ApiClientError>;

    fn execute<'a, Cx, T>(
        client: &'a ApiClient<Cx, T>,
        plan: RequestPlan,
    ) -> ResponseEntityFuture<'a, Self::Output>
    where
        Cx: ClientContext,
        T: crate::transport::Transport + 'a;
}

#[doc(hidden)]
pub trait ResponseEntityWithMeta: ResponseEntity {
    fn execute_with_meta<'a, Cx, T>(
        client: &'a ApiClient<Cx, T>,
        plan: RequestPlan,
    ) -> ResponseEntityFuture<'a, DecodedResponse<Self::Output>>
    where
        Cx: ClientContext,
        T: crate::transport::Transport + 'a;
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BufferedResponse<C>(PhantomData<fn() -> C>);

impl<C> ResponseEntity for BufferedResponse<C>
where
    C: ResponseCodec,
{
    type Output = C::Value;

    fn plan(ctx: ErrorContext) -> Result<ResponseEntityPlan, ApiClientError> {
        Ok(ResponseEntityPlan {
            response_plan: response_plan_from_codec::<C>(ctx)?,
            capabilities: ResponseEntityCapabilities {
                supports_pagination: !C::is_no_content(),
                is_streaming: false,
                is_no_content: C::is_no_content(),
            },
        })
    }

    fn execute<'a, Cx, T>(
        client: &'a ApiClient<Cx, T>,
        plan: RequestPlan,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Output, ApiClientError>> + Send + 'a>>
    where
        Cx: ClientContext,
        T: crate::transport::Transport + 'a,
    {
        Box::pin(execute_buffered_codec_response::<Cx, T, C>(client, plan))
    }
}

impl<C> ResponseEntityWithMeta for BufferedResponse<C>
where
    C: ResponseCodec,
{
    fn execute_with_meta<'a, Cx, T>(
        client: &'a ApiClient<Cx, T>,
        plan: RequestPlan,
    ) -> Pin<
        Box<dyn Future<Output = Result<DecodedResponse<Self::Output>, ApiClientError>> + Send + 'a>,
    >
    where
        Cx: ClientContext,
        T: crate::transport::Transport + 'a,
    {
        Box::pin(execute_buffered_codec_response_with_meta::<Cx, T, C>(
            client, plan,
        ))
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BytesResponse;

impl ResponseEntity for BytesResponse {
    type Output = Bytes;

    fn plan(_ctx: ErrorContext) -> Result<ResponseEntityPlan, ApiClientError> {
        Ok(ResponseEntityPlan {
            response_plan: ResponsePlan {
                accept: None,
                no_content: false,
                format: crate::codec::Format::Binary,
            },
            capabilities: ResponseEntityCapabilities {
                supports_pagination: false,
                is_streaming: false,
                is_no_content: false,
            },
        })
    }

    fn execute<'a, Cx, T>(
        client: &'a ApiClient<Cx, T>,
        plan: RequestPlan,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Output, ApiClientError>> + Send + 'a>>
    where
        Cx: ClientContext,
        T: crate::transport::Transport + 'a,
    {
        Box::pin(execute_bytes_response(client, plan))
    }
}

impl ResponseEntityWithMeta for BytesResponse {
    fn execute_with_meta<'a, Cx, T>(
        client: &'a ApiClient<Cx, T>,
        plan: RequestPlan,
    ) -> Pin<
        Box<dyn Future<Output = Result<DecodedResponse<Self::Output>, ApiClientError>> + Send + 'a>,
    >
    where
        Cx: ClientContext,
        T: crate::transport::Transport + 'a,
    {
        Box::pin(execute_bytes_response_with_meta(client, plan))
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NoContentResponse;

impl ResponseEntity for NoContentResponse {
    type Output = ();

    fn plan(_ctx: ErrorContext) -> Result<ResponseEntityPlan, ApiClientError> {
        Ok(ResponseEntityPlan {
            response_plan: ResponsePlan {
                accept: None,
                no_content: true,
                format: crate::codec::Format::Text,
            },
            capabilities: ResponseEntityCapabilities {
                supports_pagination: false,
                is_streaming: false,
                is_no_content: true,
            },
        })
    }

    fn execute<'a, Cx, T>(
        client: &'a ApiClient<Cx, T>,
        plan: RequestPlan,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Output, ApiClientError>> + Send + 'a>>
    where
        Cx: ClientContext,
        T: crate::transport::Transport + 'a,
    {
        Box::pin(execute_no_content_response(client, plan))
    }
}

impl ResponseEntityWithMeta for NoContentResponse {
    fn execute_with_meta<'a, Cx, T>(
        client: &'a ApiClient<Cx, T>,
        plan: RequestPlan,
    ) -> Pin<
        Box<dyn Future<Output = Result<DecodedResponse<Self::Output>, ApiClientError>> + Send + 'a>,
    >
    where
        Cx: ClientContext,
        T: crate::transport::Transport + 'a,
    {
        Box::pin(execute_no_content_response_with_meta(client, plan))
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RawStreamResponse<M>(PhantomData<fn() -> M>);

impl<M> ResponseEntity for RawStreamResponse<M>
where
    M: ContentType,
{
    type Output = StreamResponse<M>;

    fn plan(ctx: ErrorContext) -> Result<ResponseEntityPlan, ApiClientError> {
        Ok(ResponseEntityPlan {
            response_plan: ResponsePlan {
                accept: Some(
                    M::try_header_value()
                        .map_err(|_| ApiClientError::invalid_param(ctx.clone(), "content_type"))?,
                ),
                no_content: false,
                format: crate::codec::Format::Binary,
            },
            capabilities: ResponseEntityCapabilities {
                supports_pagination: false,
                is_streaming: true,
                is_no_content: false,
            },
        })
    }

    fn execute<'a, Cx, T>(
        client: &'a ApiClient<Cx, T>,
        plan: RequestPlan,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Output, ApiClientError>> + Send + 'a>>
    where
        Cx: ClientContext,
        T: crate::transport::Transport + 'a,
    {
        Box::pin(async move { client.execute_stream_response::<M>(plan).await })
    }
}

async fn execute_buffered_codec_response<Cx, T, C>(
    client: &ApiClient<Cx, T>,
    plan: RequestPlan,
) -> Result<C::Value, ApiClientError>
where
    Cx: ClientContext,
    T: crate::transport::Transport,
    C: ResponseCodec,
{
    execute_buffered_codec_response_with_meta::<Cx, T, C>(client, plan)
        .await
        .map(|decoded| decoded.value)
}

async fn execute_buffered_codec_response_with_meta<Cx, T, C>(
    client: &ApiClient<Cx, T>,
    plan: RequestPlan,
) -> Result<DecodedResponse<C::Value>, ApiClientError>
where
    Cx: ClientContext,
    T: crate::transport::Transport,
    C: ResponseCodec,
{
    let resp = if C::is_no_content() {
        client.execute_plan_raw_skip_body(plan).await?
    } else {
        client.execute_plan_raw(plan).await?
    };
    decode_buffered_response_with_meta::<C>(resp)
}

async fn execute_bytes_response<Cx, T>(
    client: &ApiClient<Cx, T>,
    plan: RequestPlan,
) -> Result<Bytes, ApiClientError>
where
    Cx: ClientContext,
    T: crate::transport::Transport,
{
    execute_bytes_response_with_meta(client, plan)
        .await
        .map(|decoded| decoded.value)
}

async fn execute_no_content_response<Cx, T>(
    client: &ApiClient<Cx, T>,
    plan: RequestPlan,
) -> Result<(), ApiClientError>
where
    Cx: ClientContext,
    T: crate::transport::Transport,
{
    execute_no_content_response_with_meta(client, plan)
        .await
        .map(|decoded| decoded.value)
}

async fn execute_bytes_response_with_meta<Cx, T>(
    client: &ApiClient<Cx, T>,
    plan: RequestPlan,
) -> Result<DecodedResponse<Bytes>, ApiClientError>
where
    Cx: ClientContext,
    T: crate::transport::Transport,
{
    let resp = client.execute_plan_raw(plan).await?;
    decode_bytes_response_with_meta(resp)
}

async fn execute_no_content_response_with_meta<Cx, T>(
    client: &ApiClient<Cx, T>,
    plan: RequestPlan,
) -> Result<DecodedResponse<()>, ApiClientError>
where
    Cx: ClientContext,
    T: crate::transport::Transport,
{
    let resp = client.execute_plan_raw_skip_body(plan).await?;
    decode_no_content_response_with_meta(resp)
}

fn response_plan_from_codec<C>(ctx: ErrorContext) -> Result<ResponsePlan, ApiClientError>
where
    C: ResponseCodec,
{
    Ok(ResponsePlan {
        accept: C::try_accept()
            .map_err(|_| ApiClientError::invalid_param(ctx.clone(), "content_type"))?,
        no_content: C::is_no_content(),
        format: C::format(),
    })
}

fn decode_buffered_response_with_meta<C>(
    resp: BuiltResponse,
) -> Result<DecodedResponse<C::Value>, ApiClientError>
where
    C: ResponseCodec,
{
    let ctx = validate_buffered_response(&resp, C::is_no_content())?;
    let content_type = resp
        .headers()
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let status = resp.status();
    let (message, context) = resp.into_parts();
    let (parts, body) = message.into_parts();
    let value = if C::is_no_content() {
        C::decode(
            Bytes::new(),
            DecodeContext::new(ctx.endpoint, &ctx.method, status, content_type.as_deref()),
        )
        .map_err(|_| {
            ApiClientError::response_body_decode_error(ctx.clone(), status, content_type.as_deref())
        })?
    } else {
        C::decode(
            body,
            DecodeContext::new(ctx.endpoint, &ctx.method, status, content_type.as_deref()),
        )
        .map_err(|_| {
            ApiClientError::response_body_decode_error(ctx.clone(), status, content_type.as_deref())
        })?
    };
    Ok(DecodedResponse {
        meta: context.meta,
        url: context.request_url,
        status,
        headers: parts.headers,
        value,
    })
}

fn decode_bytes_response_with_meta(
    resp: BuiltResponse,
) -> Result<DecodedResponse<Bytes>, ApiClientError> {
    let _ctx = validate_buffered_response(&resp, false)?;
    let status = resp.status();
    let (message, context) = resp.into_parts();
    let (parts, body) = message.into_parts();
    Ok(DecodedResponse {
        meta: context.meta,
        url: context.request_url,
        status,
        headers: parts.headers,
        value: body,
    })
}

fn decode_no_content_response_with_meta(
    resp: BuiltResponse,
) -> Result<DecodedResponse<()>, ApiClientError> {
    let _ctx = validate_buffered_response(&resp, true)?;
    let status = resp.status();
    let (message, context) = resp.into_parts();
    let (parts, _) = message.into_parts();
    Ok(DecodedResponse {
        meta: context.meta,
        url: context.request_url,
        status,
        headers: parts.headers,
        value: (),
    })
}

fn validate_buffered_response(
    resp: &BuiltResponse,
    no_content: bool,
) -> Result<ErrorContext, ApiClientError> {
    let ctx = ErrorContext {
        endpoint: resp.meta().endpoint,
        method: resp.meta().method.clone(),
    };
    if resp.meta().method == http::Method::HEAD && !no_content {
        return Err(ApiClientError::HeadRequiresNoContent { ctx });
    }
    if matches!(
        resp.status(),
        http::StatusCode::NO_CONTENT | http::StatusCode::RESET_CONTENT
    ) && !no_content
    {
        return Err(ApiClientError::NoContentStatusRequiresNoContent {
            ctx: ctx.clone(),
            status: resp.status(),
        });
    }
    Ok(ctx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::text::Text;
    use crate::codec::{BodyCodec, ContentType, EncodeContext, EncodedBody};
    use crate::media::{OctetStream, TextContentType};
    #[cfg(feature = "multipart")]
    use crate::multipart::MultipartBody;
    use crate::transport::Transport;
    use http::{HeaderMap, Method, StatusCode};
    use http_body_util::BodyExt;
    #[cfg(feature = "json")]
    use serde::{Deserialize, Serialize};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::task::{Context, Poll};

    struct TestStream<T> {
        items: std::collections::VecDeque<T>,
        polls: Option<Arc<AtomicUsize>>,
    }

    impl<T> TestStream<T> {
        fn new(items: impl IntoIterator<Item = T>) -> Self {
            Self {
                items: items.into_iter().collect(),
                polls: None,
            }
        }

        fn with_polls(mut self, polls: Arc<AtomicUsize>) -> Self {
            self.polls = Some(polls);
            self
        }
    }

    impl<T: Unpin> futures_core::Stream for TestStream<T> {
        type Item = T;

        fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            if let Some(polls) = &self.polls {
                polls.fetch_add(1, Ordering::SeqCst);
            }
            Poll::Ready(self.items.pop_front())
        }
    }

    #[cfg(feature = "json")]
    use crate::media::JsonContentType;

    fn ctx() -> ErrorContext {
        ErrorContext {
            endpoint: "Example",
            method: Method::POST,
        }
    }

    #[derive(Clone, Copy, Debug, Default)]
    struct CustomEncodedContentType;

    impl ContentType for CustomEncodedContentType {
        const CONTENT_TYPE: &'static str = "application/x-custom-encoded";
    }

    struct TextOverrideCodec;

    impl BodyCodec for TextOverrideCodec {
        type Value = String;
        type Content = CustomEncodedContentType;

        fn encode(
            value: Self::Value,
            _ctx: EncodeContext<'_>,
        ) -> Result<EncodedBody, crate::codec::CodecError> {
            Ok(EncodedBody::from_bytes(value.into_bytes()).text())
        }
    }

    #[derive(Clone)]
    struct TestCx;

    impl ClientContext for TestCx {
        type Vars = ();
        type AuthVars = ();
        type AuthState = crate::auth::NoAuthState;
        const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
        const DOMAIN: &'static str = "example.test";

        fn init_auth_state(_vars: &Self::Vars, _auth: &Self::AuthVars) -> Self::AuthState {
            crate::auth::NoAuthState
        }
    }

    #[derive(Clone)]
    struct StaticTransport {
        response: StaticResponse,
    }

    #[derive(Clone)]
    struct StaticResponse {
        status: StatusCode,
        headers: HeaderMap,
        chunks: Vec<Bytes>,
        content_length: Option<u64>,
    }

    impl StaticTransport {
        fn new(response: StaticResponse) -> Self {
            Self { response }
        }
    }

    impl Transport for StaticTransport {
        fn send(
            &self,
            _req: http::Request<crate::body::DynBody>,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = Result<
                            http::Response<crate::body::DynBody>,
                            crate::transport::TransportError,
                        >,
                    > + Send,
            >,
        > {
            let response = self.response.clone();
            Box::pin(async move {
                let mut builder = http::Response::builder().status(response.status);
                *builder.headers_mut().expect("headers") = response.headers;
                if let Some(length) = response.content_length {
                    builder.headers_mut().expect("headers").insert(
                        http::header::CONTENT_LENGTH,
                        http::HeaderValue::from_str(&length.to_string()).expect("length"),
                    );
                }
                let bytes = Bytes::from(response.chunks.concat());
                builder
                    .body(crate::body::DynBody::from_bytes(bytes))
                    .map_err(crate::transport::TransportError::new)
            })
        }
    }

    fn request_plan(response_plan: ResponsePlan) -> RequestPlan {
        RequestPlan {
            endpoint: crate::endpoint::EndpointPlan {
                meta: crate::endpoint::EndpointMeta {
                    name: "Example",
                    method: Method::POST,
                    idempotent: true,
                    facade_path: &[],
                },
                route: crate::endpoint::ResolvedRoute::new(
                    http::uri::Scheme::HTTPS,
                    "example.test",
                    "/items",
                ),
                policy: crate::policy::ResolvedPolicy::default(),
                response: response_plan,
                pagination: None,
            },
            body: PreparedBody::empty(),
            overrides: crate::endpoint::RequestOverrides::default(),
        }
    }

    fn response_headers(content_type: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Some(content_type) = content_type {
            headers.insert(
                http::header::CONTENT_TYPE,
                http::HeaderValue::from_str(content_type).expect("valid content type"),
            );
        }
        headers
    }

    fn transport(response: StaticResponse) -> StaticTransport {
        StaticTransport::new(response)
    }

    #[test]
    fn no_request_body_is_empty_and_replayable() {
        let prepared = NoRequestBody::prepare((), ctx()).expect("no request body");
        assert!(prepared.body.is_replayable());
        assert_eq!(prepared.body.size_hint().exact(), Some(0));
    }

    #[tokio::test]
    async fn encoded_request_matches_buffered_body_path() {
        let input = String::from("hello");
        let mut prepared =
            EncodedRequest::<Text<String>>::prepare(input.clone(), ctx()).expect("encoded request");
        let expected =
            Text::<String>::encode(input.clone(), EncodeContext::new("Example", &Method::POST))
                .expect("encode")
                .into_parts()
                .0;
        assert_eq!(
            prepared.body.media_type(),
            Some(&TextContentType::try_header_value().expect("text content type"))
        );
        let bytes = prepared
            .body
            .produce_for_attempt()
            .expect("body")
            .into_dyn_body()
            .collect()
            .await
            .expect("collect")
            .to_bytes();
        assert_eq!(bytes, expected);
    }

    #[tokio::test]
    async fn encoded_request_preserves_returned_media_type() {
        let prepared = EncodedRequest::<TextOverrideCodec>::prepare(String::from("hello"), ctx())
            .expect("encoded request");
        assert_eq!(
            prepared.body.media_type(),
            Some(&CustomEncodedContentType::try_header_value().expect("custom content type"))
        );
        assert!(prepared.body.is_replayable());
    }

    #[test]
    fn raw_stream_request_is_non_replayable() {
        let prepared = RawStreamRequest::<OctetStream>::prepare(
            StreamBody::from_bytes(Bytes::from_static(b"abc")),
            ctx(),
        )
        .expect("stream request");
        assert!(!prepared.body.is_replayable());
        assert_eq!(prepared.body.size_hint().exact(), Some(3));
    }

    #[test]
    #[cfg(feature = "multipart")]
    fn multipart_request_prepares_stream_body() {
        let multipart = MultipartRequest::prepare(
            MultipartBody::new().bytes("payload", Bytes::from_static(b"abc")),
            ctx(),
        )
        .expect("multipart request");
        assert!(!multipart.body.is_replayable());
        assert!(multipart.body.media_type().is_some());
    }

    #[tokio::test]
    async fn one_shot_production_is_unpolled_and_second_use_fails_structurally() {
        let polls = Arc::new(AtomicUsize::new(0));
        let stream = TestStream::new(Vec::<Result<Bytes, crate::body::BodyError>>::new())
            .with_polls(polls.clone());
        let mut prepared =
            PreparedBody::one_shot(crate::body::DynBody::from_byte_stream(stream), None);

        assert!(!prepared.is_replayable());
        assert_eq!(polls.load(Ordering::SeqCst), 0);
        let first = prepared
            .produce_for_attempt()
            .expect("first body")
            .into_dyn_body();
        assert_eq!(polls.load(Ordering::SeqCst), 0);
        let error = prepared
            .produce_for_attempt()
            .expect_err("second body production must fail");
        assert_eq!(error.kind(), BodyProductionErrorKind::AlreadyConsumed);
        assert_eq!(polls.load(Ordering::SeqCst), 0);
        drop(first);
    }

    #[tokio::test]
    async fn reusable_bytes_and_replay_factory_produce_fresh_attempt_bodies() {
        let mut bytes_body = PreparedBody::reusable_bytes(Bytes::from_static(b"repeat"), None);
        for _ in 0..2 {
            let bytes = bytes_body
                .produce_for_attempt()
                .expect("reusable body")
                .into_dyn_body()
                .collect()
                .await
                .expect("collect")
                .to_bytes();
            assert_eq!(bytes, Bytes::from_static(b"repeat"));
        }

        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let mut factory = PreparedBody::replay_factory(exact_size_hint(5), None, move || {
            observed.fetch_add(1, Ordering::SeqCst);
            Ok(crate::body::DynBody::from_bytes(Bytes::from_static(
                b"fresh",
            )))
        });
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        for expected_calls in 1..=2 {
            let body = factory
                .produce_for_attempt()
                .expect("factory body")
                .into_dyn_body();
            assert_eq!(calls.load(Ordering::SeqCst), expected_calls);
            assert_eq!(
                body.collect().await.expect("collect").to_bytes(),
                Bytes::from_static(b"fresh")
            );
        }
    }

    #[test]
    fn factory_failure_is_distinct_from_one_shot_exhaustion_and_is_redacted() {
        let sentinel = "FACTORY_PRODUCER_SENTINEL_MUST_NOT_APPEAR";
        let mut factory = PreparedBody::replay_factory(exact_size_hint(0), None, move || {
            let _ = sentinel;
            Err(crate::body::BodyError::invalid_configuration())
        });
        let error = factory
            .produce_for_attempt()
            .expect_err("factory should fail");
        assert_eq!(error.kind(), BodyProductionErrorKind::FactoryFailure);
        assert_eq!(
            error.body_error_kind(),
            Some(crate::body::BodyErrorKind::InvalidConfiguration)
        );
        assert!(!format!("{error}").contains(sentinel));
        assert!(!format!("{error:?}").contains(sentinel));
        assert!(std::error::Error::source(&error).is_none());
    }

    #[test]
    fn auth_internal_support_check_does_not_invoke_unsupported_factory() {
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let factory = PreparedBody::replay_factory(exact_size_hint(0), None, move || {
            observed.fetch_add(1, Ordering::SeqCst);
            Ok(crate::body::DynBody::empty())
        });
        assert!(!factory.supports_auth_internal_retries());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        let one_shot = PreparedBody::one_shot(crate::body::DynBody::empty(), None);
        assert!(!one_shot.supports_auth_internal_retries());
    }

    #[tokio::test]
    #[cfg(feature = "multipart")]
    async fn multipart_media_type_and_encoded_boundary_share_one_owner() {
        let mut prepared =
            MultipartRequest::prepare(MultipartBody::new().text("title", "hello"), ctx())
                .expect("multipart request")
                .body;
        let media_type = prepared
            .media_type()
            .and_then(|value| value.to_str().ok())
            .expect("media type")
            .to_string();
        let boundary = media_type
            .strip_prefix("multipart/form-data; boundary=")
            .expect("boundary");
        let encoded = prepared
            .produce_for_attempt()
            .expect("multipart body")
            .into_dyn_body()
            .collect()
            .await
            .expect("collect")
            .to_bytes();
        assert!(
            encoded
                .windows(boundary.len())
                .any(|part| part == boundary.as_bytes())
        );
    }

    #[tokio::test]
    async fn prepared_one_shot_preserves_frames_and_trailers_before_legacy_bridge() {
        let mut trailers = HeaderMap::new();
        trailers.insert("x-trailer", http::HeaderValue::from_static("present"));
        let frames = TestStream::new(vec![
            Ok::<_, crate::body::BodyError>(http_body::Frame::data(Bytes::from_static(b"data"))),
            Ok(http_body::Frame::trailers(trailers.clone())),
        ]);
        let mut prepared =
            PreparedBody::one_shot(crate::body::DynBody::from_frame_stream(frames), None);
        let mut body = prepared
            .produce_for_attempt()
            .expect("body")
            .into_dyn_body();
        let data = body
            .frame()
            .await
            .expect("data frame")
            .expect("data")
            .into_data()
            .expect("data frame");
        assert_eq!(data, Bytes::from_static(b"data"));
        let actual_trailers = body
            .frame()
            .await
            .expect("trailer frame")
            .expect("trailers")
            .into_trailers()
            .expect("trailer frame");
        assert_eq!(actual_trailers, trailers);
    }

    #[tokio::test]
    async fn buffered_response_execute_returns_decoded_value() {
        let plan = BufferedResponse::<Text<String>>::plan(ctx()).expect("buffered response");
        let transport = transport(StaticResponse {
            status: StatusCode::OK,
            headers: response_headers(Some("text/plain")),
            chunks: vec![Bytes::from_static(b"hello")],
            content_length: None,
        });
        let client = ApiClient::<TestCx, _>::with_transport((), (), transport.clone());
        let response =
            BufferedResponse::<Text<String>>::execute(&client, request_plan(plan.response_plan))
                .await
                .expect("buffered execute");
        assert_eq!(response, "hello");
    }

    #[tokio::test]
    async fn bytes_response_execute_returns_bytes() {
        let plan = BytesResponse::plan(ctx()).expect("bytes response");
        let transport = transport(StaticResponse {
            status: StatusCode::OK,
            headers: response_headers(None),
            chunks: vec![Bytes::from_static(b"hello"), Bytes::from_static(b" world")],
            content_length: None,
        });
        let client = ApiClient::<TestCx, _>::with_transport((), (), transport.clone());
        let response = BytesResponse::execute(&client, request_plan(plan.response_plan))
            .await
            .expect("bytes execute");
        assert_eq!(response, Bytes::from_static(b"hello world"));
    }

    #[tokio::test]
    async fn no_content_response_execute_returns_unit() {
        let plan = NoContentResponse::plan(ctx()).expect("no content response");
        let transport = transport(StaticResponse {
            status: StatusCode::NO_CONTENT,
            headers: response_headers(None),
            chunks: vec![],
            content_length: None,
        });
        let client = ApiClient::<TestCx, _>::with_transport((), (), transport.clone());
        NoContentResponse::execute(&client, request_plan(plan.response_plan))
            .await
            .expect("no content execute");
        assert_eq!((), ());
    }

    #[tokio::test]
    async fn streaming_response_adapter_executes_through_existing_stream_path() {
        let plan = RawStreamResponse::<OctetStream>::plan(ctx()).expect("stream response");
        let transport = transport(StaticResponse {
            status: StatusCode::OK,
            headers: response_headers(Some(OctetStream::CONTENT_TYPE)),
            chunks: vec![Bytes::from_static(b"abc"), Bytes::from_static(b"def")],
            content_length: None,
        });
        let client = ApiClient::<TestCx, _>::with_transport((), (), transport.clone());
        let mut response =
            RawStreamResponse::<OctetStream>::execute(&client, request_plan(plan.response_plan))
                .await
                .expect("stream execute");
        let mut out = Vec::new();
        while let Some(chunk) = response.next_chunk().await.expect("stream chunk") {
            out.extend_from_slice(&chunk);
        }
        assert_eq!(Bytes::from(out), Bytes::from_static(b"abcdef"));
    }

    #[cfg(feature = "json")]
    #[tokio::test]
    async fn buffered_response_json_exposes_buffered_capabilities() {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        struct Item {
            id: u32,
        }

        let plan = BufferedResponse::<crate::codec::json::Json<Item>>::plan(ctx())
            .expect("buffered response");

        assert_eq!(
            plan.capabilities,
            ResponseEntityCapabilities {
                supports_pagination: true,
                is_streaming: false,
                is_no_content: false,
            }
        );

        assert_eq!(
            plan.response_plan.accept,
            Some(JsonContentType::try_header_value().expect("json content type"))
        );

        let client = ApiClient::<TestCx, _>::with_transport(
            (),
            (),
            transport(StaticResponse {
                status: StatusCode::OK,
                headers: response_headers(Some("application/json")),
                chunks: vec![Bytes::from_static(br#"{"id":1}"#)],
                content_length: None,
            }),
        );

        let decoded = execute_buffered_codec_response::<_, _, crate::codec::json::Json<Item>>(
            &client,
            request_plan(plan.response_plan),
        )
        .await
        .expect("decode");

        assert_eq!(decoded, Item { id: 1 });
    }

    #[tokio::test]
    async fn buffered_response_text_is_buffered_and_non_streaming() {
        let plan = BufferedResponse::<Text<String>>::plan(ctx()).expect("buffered response");
        assert_eq!(
            plan.capabilities,
            ResponseEntityCapabilities {
                supports_pagination: true,
                is_streaming: false,
                is_no_content: false,
            }
        );
        let client = ApiClient::<TestCx, _>::with_transport(
            (),
            (),
            transport(StaticResponse {
                status: StatusCode::OK,
                headers: response_headers(Some("text/plain; charset=utf-8")),
                chunks: vec![Bytes::from_static(b"hello")],
                content_length: None,
            }),
        );
        let decoded = execute_buffered_codec_response::<_, _, Text<String>>(
            &client,
            request_plan(plan.response_plan),
        )
        .await
        .expect("decode");
        assert_eq!(decoded, "hello");
    }

    #[tokio::test]
    async fn bytes_response_exposes_buffered_bytes_capabilities() {
        let plan = BytesResponse::plan(ctx()).expect("bytes response");
        assert_eq!(
            plan.capabilities,
            ResponseEntityCapabilities {
                supports_pagination: false,
                is_streaming: false,
                is_no_content: false,
            }
        );
        let client = ApiClient::<TestCx, _>::with_transport(
            (),
            (),
            transport(StaticResponse {
                status: StatusCode::OK,
                headers: response_headers(None),
                chunks: vec![Bytes::from_static(b"bytes")],
                content_length: None,
            }),
        );
        let decoded = BytesResponse::execute(&client, request_plan(plan.response_plan))
            .await
            .expect("decode");
        assert_eq!(decoded, Bytes::from_static(b"bytes"));
    }

    #[tokio::test]
    async fn no_content_response_has_no_content_capabilities() {
        let plan = NoContentResponse::plan(ctx()).expect("no content response");
        assert_eq!(
            plan.capabilities,
            ResponseEntityCapabilities {
                supports_pagination: false,
                is_streaming: false,
                is_no_content: true,
            }
        );
        let client = ApiClient::<TestCx, _>::with_transport(
            (),
            (),
            transport(StaticResponse {
                status: StatusCode::NO_CONTENT,
                headers: response_headers(None),
                chunks: vec![],
                content_length: None,
            }),
        );
        NoContentResponse::execute(&client, request_plan(plan.response_plan))
            .await
            .expect("decode");
        assert_eq!((), ());
    }

    #[test]
    fn streaming_response_adapter_reports_streaming_capabilities() {
        let plan = RawStreamResponse::<OctetStream>::plan(ctx()).expect("stream response");
        assert_eq!(
            plan.capabilities,
            ResponseEntityCapabilities {
                supports_pagination: false,
                is_streaming: true,
                is_no_content: false,
            }
        );
        assert_eq!(
            plan.response_plan.accept,
            Some(OctetStream::try_header_value().expect("octet-stream"))
        );
        assert_eq!(plan.response_plan.format, crate::codec::Format::Binary);
    }
}

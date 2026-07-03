use crate::client::{ApiClient, ClientContext};
use crate::codec::{BodyCodec, ContentType, DecodeContext, EncodeContext, ResponseCodec};
use crate::endpoint::{BodyPlan, RequestArgs, RequestPlan, ResponsePlan};
use crate::error::{ApiClientError, ErrorContext};
use crate::media::EventStream;
use crate::multipart::{MultipartBody, MultipartFormat};
use crate::record::{RecordBody, RecordFormat};
use crate::sse::SseCodec;
use crate::stream_body::StreamBody;
use crate::stream_response::StreamResponse;
use crate::transport::BuiltResponse;
use bytes::Bytes;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Replayability {
    Replayable,
    NonReplayable,
}

#[derive(Debug)]
pub struct PreparedRequestEntity {
    pub body_plan: BodyPlan,
    pub args: RequestArgs,
    pub replayability: Replayability,
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
            body_plan: BodyPlan::None,
            args: RequestArgs::empty(),
            replayability: Replayability::Replayable,
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
            .map_err(|source| ApiClientError::codec_error(ctx.clone(), source))?;
        let (bytes, format) = encoded.into_parts();
        let content_type = C::try_content_type()
            .map_err(|_| ApiClientError::invalid_param(ctx.clone(), "content_type"))?;
        Ok(PreparedRequestEntity {
            body_plan: BodyPlan::Encoded {
                content_type,
                format,
            },
            args: RequestArgs::with_body_bytes(bytes),
            replayability: Replayability::Replayable,
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
            body_plan: BodyPlan::RawStream { content_type },
            args: RequestArgs::with_stream_body(input),
            replayability: Replayability::NonReplayable,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RecordRequest<Item, F>(PhantomData<fn() -> (Item, F)>);

impl<Item, F> RequestEntity for RecordRequest<Item, F>
where
    Item: Send + 'static,
    F: RecordFormat<Item>,
{
    type Input = RecordBody<Item>;

    fn prepare(
        input: Self::Input,
        ctx: ErrorContext,
    ) -> Result<PreparedRequestEntity, ApiClientError> {
        let content_type = F::try_header_value()
            .map_err(|_| ApiClientError::invalid_param(ctx.clone(), "content_type"))?;
        Ok(PreparedRequestEntity {
            body_plan: BodyPlan::Records {
                content_type,
                format: crate::codec::Format::Text,
            },
            args: RequestArgs::with_record_body::<Item, F>(input),
            replayability: Replayability::NonReplayable,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MultipartRequest<F>(PhantomData<fn() -> F>);

impl<F> RequestEntity for MultipartRequest<F>
where
    F: MultipartFormat,
{
    type Input = MultipartBody;

    fn prepare(
        input: Self::Input,
        ctx: ErrorContext,
    ) -> Result<PreparedRequestEntity, ApiClientError> {
        let content_type = input
            .try_content_type::<F>()
            .map_err(|_| ApiClientError::invalid_param(ctx.clone(), "content_type"))?;
        let args = RequestArgs::with_multipart_body::<F>(input)
            .map_err(|source| ApiClientError::codec_error(ctx.clone(), source))?;
        Ok(PreparedRequestEntity {
            body_plan: BodyPlan::Multipart {
                content_type,
                format: crate::codec::Format::Text,
            },
            args,
            replayability: Replayability::NonReplayable,
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

pub trait ResponseEntity {
    type Output;

    fn plan(ctx: ErrorContext) -> Result<ResponseEntityPlan, ApiClientError>;

    fn execute<'a, Cx, T>(
        client: &'a ApiClient<Cx, T>,
        plan: RequestPlan,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Output, ApiClientError>> + Send + 'a>>
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
        Box::pin(async move { execute_buffered_codec_response::<Cx, T, C>(client, plan).await })
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
        Box::pin(async move { execute_bytes_response(client, plan).await })
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
        Box::pin(async move { execute_no_content_response(client, plan).await })
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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RecordResponse<Item, F>(PhantomData<fn() -> (Item, F)>);

impl<Item, F> ResponseEntity for RecordResponse<Item, F>
where
    Item: Send + 'static,
    F: RecordFormat<Item>,
{
    type Output = crate::record::RecordStream<Item>;

    fn plan(ctx: ErrorContext) -> Result<ResponseEntityPlan, ApiClientError> {
        Ok(ResponseEntityPlan {
            response_plan: ResponsePlan {
                accept: Some(
                    F::try_header_value()
                        .map_err(|_| ApiClientError::invalid_param(ctx.clone(), "content_type"))?,
                ),
                no_content: false,
                format: crate::codec::Format::Text,
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
        Box::pin(async move { client.execute_record_response::<Item, F>(plan).await })
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MultipartResponse<Part, F>(PhantomData<fn() -> (Part, F)>);

impl<Part, F> ResponseEntity for MultipartResponse<Part, F>
where
    Part: crate::multipart_response::MultipartDecodePart<F>,
    F: MultipartFormat,
{
    type Output = crate::multipart_response::MultipartStream<Part>;

    fn plan(ctx: ErrorContext) -> Result<ResponseEntityPlan, ApiClientError> {
        Ok(ResponseEntityPlan {
            response_plan: ResponsePlan {
                accept: Some(
                    F::try_header_value()
                        .map_err(|_| ApiClientError::invalid_param(ctx.clone(), "content_type"))?,
                ),
                no_content: false,
                format: crate::codec::Format::Text,
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
        Box::pin(async move { client.execute_multipart_response::<Part, F>(plan).await })
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SseResponse<Event, C>(PhantomData<fn() -> (Event, C)>);

impl<Event, C> ResponseEntity for SseResponse<Event, C>
where
    Event: Send + 'static,
    C: SseCodec<Event>,
{
    type Output = crate::sse::SseStream<Event>;

    fn plan(ctx: ErrorContext) -> Result<ResponseEntityPlan, ApiClientError> {
        Ok(ResponseEntityPlan {
            response_plan: ResponsePlan {
                accept: Some(
                    EventStream::try_header_value()
                        .map_err(|_| ApiClientError::invalid_param(ctx.clone(), "content_type"))?,
                ),
                no_content: false,
                format: crate::codec::Format::Text,
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
        Box::pin(async move { client.execute_sse_response::<Event, C>(plan).await })
    }
}

fn execute_buffered_codec_response<'a, Cx, T, C>(
    client: &'a ApiClient<Cx, T>,
    plan: RequestPlan,
) -> Pin<Box<dyn Future<Output = Result<C::Value, ApiClientError>> + Send + 'a>>
where
    Cx: ClientContext,
    T: crate::transport::Transport + 'a,
    C: ResponseCodec,
{
    Box::pin(async move {
        if C::is_no_content() {
            let resp = client.execute_plan_raw_skip_body(plan).await?;
            return decode_buffered_response::<C>(resp);
        }
        let resp = client.execute_plan_raw(plan).await?;
        decode_buffered_response::<C>(resp)
    })
}

async fn execute_bytes_response<Cx, T>(
    client: &ApiClient<Cx, T>,
    plan: RequestPlan,
) -> Result<Bytes, ApiClientError>
where
    Cx: ClientContext,
    T: crate::transport::Transport,
{
    let resp = client.execute_plan_raw(plan).await?;
    validate_buffered_response(&resp, false)?;
    Ok(resp.body)
}

async fn execute_no_content_response<Cx, T>(
    client: &ApiClient<Cx, T>,
    plan: RequestPlan,
) -> Result<(), ApiClientError>
where
    Cx: ClientContext,
    T: crate::transport::Transport,
{
    let resp = client.execute_plan_raw_skip_body(plan).await?;
    validate_buffered_response(&resp, true)?;
    Ok(())
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

fn decode_buffered_response<C>(resp: BuiltResponse) -> Result<C::Value, ApiClientError>
where
    C: ResponseCodec,
{
    let ctx = validate_buffered_response(&resp, C::is_no_content())?;
    let content_type = resp
        .headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok());
    if C::is_no_content() {
        return C::decode(
            Bytes::new(),
            DecodeContext::new(ctx.endpoint, &ctx.method, resp.status, content_type),
        )
        .map_err(|source| {
            ApiClientError::decode_error(ctx.clone(), resp.status, content_type, source)
        });
    }
    let decoded = C::decode(
        resp.body.clone(),
        DecodeContext::new(ctx.endpoint, &ctx.method, resp.status, content_type),
    )
    .map_err(|source| {
        ApiClientError::decode_error(ctx.clone(), resp.status, content_type, source)
    })?;
    Ok(decoded)
}

fn validate_buffered_response(
    resp: &BuiltResponse,
    no_content: bool,
) -> Result<ErrorContext, ApiClientError> {
    let ctx = ErrorContext {
        endpoint: resp.meta.endpoint,
        method: resp.meta.method.clone(),
    };
    if resp.meta.method == http::Method::HEAD && !no_content {
        return Err(ApiClientError::HeadRequiresNoContent { ctx });
    }
    if matches!(
        resp.status,
        http::StatusCode::NO_CONTENT | http::StatusCode::RESET_CONTENT
    ) && !no_content
    {
        return Err(ApiClientError::NoContentStatusRequiresNoContent {
            ctx: ctx.clone(),
            status: resp.status,
        });
    }
    Ok(ctx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::text::Text;
    use crate::codec::{BodyCodec, ContentType, EncodeContext, EncodedBody, Format};
    use crate::media::{EventStream, OctetStream, TextContentType};
    use crate::multipart::{FormData, Mixed, MultipartBody};
    use crate::multipart_response::RawResponsePart;
    use crate::record::NdJson;
    use crate::transport::Transport;
    use http::{HeaderMap, Method, StatusCode};
    use serde::{Deserialize, Serialize};

    #[cfg(feature = "json")]
    use crate::media::JsonContentType;

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct Payload {
        id: u32,
    }

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

    struct ChunkBody {
        chunks: std::collections::VecDeque<Bytes>,
    }

    impl ChunkBody {
        fn new(chunks: Vec<Bytes>) -> Self {
            Self {
                chunks: chunks.into(),
            }
        }
    }

    impl crate::transport::TransportBody for ChunkBody {
        fn next_chunk<'a>(
            &'a mut self,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = Result<Option<Bytes>, crate::transport::TransportError>,
                    > + Send
                    + 'a,
            >,
        > {
            let chunk = self.chunks.pop_front();
            Box::pin(async move { Ok(chunk) })
        }
    }

    impl StaticTransport {
        fn new(response: StaticResponse) -> Self {
            Self { response }
        }
    }

    impl Transport for StaticTransport {
        fn send(
            &self,
            req: crate::transport::TransportRequest,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = Result<
                            crate::transport::TransportResponse,
                            crate::transport::TransportError,
                        >,
                    > + Send,
            >,
        > {
            let response = self.response.clone();
            Box::pin(async move {
                Ok(crate::transport::TransportResponse {
                    meta: req.meta,
                    url: req.url,
                    status: response.status,
                    headers: response.headers,
                    content_length: response.content_length,
                    rate_limit: req.rate_limit,
                    body: Box::new(ChunkBody::new(response.chunks)),
                })
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
                body: BodyPlan::None,
                response: response_plan,
                pagination: None,
            },
            args: RequestArgs::empty(),
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
        assert_eq!(prepared.body_plan, BodyPlan::None);
        assert!(prepared.args.body.is_empty());
        assert!(prepared.args.stream_size_hint.is_none());
        assert_eq!(prepared.replayability, Replayability::Replayable);
    }

    #[test]
    fn encoded_request_matches_buffered_body_path() {
        let input = String::from("hello");
        let prepared =
            EncodedRequest::<Text<String>>::prepare(input.clone(), ctx()).expect("encoded request");
        let expected =
            Text::<String>::encode(input.clone(), EncodeContext::new("Example", &Method::POST))
                .expect("encode")
                .into_parts()
                .0;
        assert_eq!(
            prepared.body_plan,
            BodyPlan::Encoded {
                content_type: Some(TextContentType::try_header_value().expect("text content type")),
                format: crate::codec::Format::Text,
            }
        );
        assert_eq!(prepared.args.body.as_bytes(), Some(&expected));
        assert_eq!(prepared.replayability, Replayability::Replayable);
    }

    #[test]
    fn encoded_request_preserves_returned_encoded_body_format() {
        let prepared = EncodedRequest::<TextOverrideCodec>::prepare(String::from("hello"), ctx())
            .expect("encoded request");
        assert_eq!(
            prepared.body_plan,
            BodyPlan::Encoded {
                content_type: Some(
                    CustomEncodedContentType::try_header_value().expect("custom content type")
                ),
                format: Format::Text,
            }
        );
        assert_eq!(
            prepared.args.body.as_bytes(),
            Some(&Bytes::from_static(b"hello"))
        );
        assert_eq!(prepared.replayability, Replayability::Replayable);
    }

    #[test]
    fn raw_stream_request_is_non_replayable() {
        let prepared = RawStreamRequest::<OctetStream>::prepare(
            StreamBody::from_bytes(Bytes::from_static(b"abc")),
            ctx(),
        )
        .expect("stream request");
        assert!(matches!(prepared.body_plan, BodyPlan::RawStream { .. }));
        assert!(prepared.args.body.is_stream());
        assert_eq!(prepared.replayability, Replayability::NonReplayable);
    }

    #[test]
    fn record_and_multipart_requests_prepare_stream_bodies() {
        let record = RecordRequest::<Payload, NdJson>::prepare(
            RecordBody::from_iter(vec![Payload { id: 1 }]),
            ctx(),
        )
        .expect("record request");
        assert!(matches!(record.body_plan, BodyPlan::Records { .. }));
        assert!(record.args.body.is_stream());
        assert_eq!(record.replayability, Replayability::NonReplayable);

        let multipart = MultipartRequest::<FormData>::prepare(
            MultipartBody::new().bytes("payload", Bytes::from_static(b"abc")),
            ctx(),
        )
        .expect("multipart request");
        assert!(matches!(multipart.body_plan, BodyPlan::Multipart { .. }));
        assert!(multipart.args.body.is_stream());
        assert!(multipart.args.multipart_content_type.is_some());
        assert_eq!(multipart.replayability, Replayability::NonReplayable);
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
        let response = NoContentResponse::execute(&client, request_plan(plan.response_plan))
            .await
            .expect("no content execute");
        assert_eq!(response, ());
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

    #[tokio::test]
    async fn record_response_adapter_executes_through_existing_record_path() {
        let plan = RecordResponse::<Payload, NdJson>::plan(ctx()).expect("record response");
        let transport = transport(StaticResponse {
            status: StatusCode::OK,
            headers: response_headers(Some(NdJson::CONTENT_TYPE)),
            chunks: vec![Bytes::from_static(b"{\"id\":1}\n{\"id\":2}\n")],
            content_length: None,
        });
        let client = ApiClient::<TestCx, _>::with_transport((), (), transport);
        let mut response =
            RecordResponse::<Payload, NdJson>::execute(&client, request_plan(plan.response_plan))
                .await
                .expect("record execute");
        let mut out = Vec::new();
        while let Some(item) = response.next_record().await.expect("record item") {
            out.push(item.id);
        }
        assert_eq!(out, vec![1, 2]);
    }

    #[tokio::test]
    async fn multipart_response_adapter_executes_through_existing_multipart_path() {
        let plan =
            MultipartResponse::<RawResponsePart, Mixed>::plan(ctx()).expect("multipart response");
        let boundary = "concord-test-boundary";
        let body = Bytes::from_static(
            b"--concord-test-boundary\r\ncontent-type: text/plain\r\n\r\nhello\r\n--concord-test-boundary--\r\n",
        );
        let transport = transport(StaticResponse {
            status: StatusCode::OK,
            headers: response_headers(Some(&format!("multipart/mixed; boundary={boundary}"))),
            chunks: vec![body],
            content_length: None,
        });
        let client = ApiClient::<TestCx, _>::with_transport((), (), transport);
        let mut response = MultipartResponse::<RawResponsePart, Mixed>::execute(
            &client,
            request_plan(plan.response_plan),
        )
        .await
        .expect("multipart execute");
        let part = response
            .next_part()
            .await
            .expect("multipart part result")
            .expect("multipart part");
        assert_eq!(
            part.content_type().and_then(|value| value.to_str().ok()),
            Some("text/plain")
        );
    }

    #[tokio::test]
    async fn sse_response_adapter_executes_through_existing_sse_path() {
        let plan = SseResponse::<Payload, crate::sse::JsonSse>::plan(ctx()).expect("sse");
        let transport = transport(StaticResponse {
            status: StatusCode::OK,
            headers: response_headers(Some(EventStream::CONTENT_TYPE)),
            chunks: vec![Bytes::from_static(b"data: {\"id\":1}\n\n")],
            content_length: None,
        });
        let client = ApiClient::<TestCx, _>::with_transport((), (), transport);
        let mut response = SseResponse::<Payload, crate::sse::JsonSse>::execute(
            &client,
            request_plan(plan.response_plan),
        )
        .await
        .expect("sse execute");
        let event = response
            .next_event()
            .await
            .expect("sse event result")
            .expect("sse event");
        assert_eq!(event.data.id, 1);
    }

    #[cfg(feature = "json")]
    #[test]
    fn buffered_response_json_exposes_buffered_capabilities() {
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
        let decoded = NoContentResponse::execute(&client, request_plan(plan.response_plan))
            .await
            .expect("decode");
        assert_eq!(decoded, ());
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

    #[test]
    fn sse_response_reports_streaming_capabilities() {
        let plan = SseResponse::<Payload, crate::sse::JsonSse>::plan(ctx()).expect("sse");
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
            Some(EventStream::try_header_value().expect("event stream"))
        );
        assert_eq!(plan.response_plan.format, crate::codec::Format::Text);
    }
}

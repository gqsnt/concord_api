use crate::codec::{BodyCodec, ContentType, DecodeContext, EncodeContext, ResponseCodec};
use crate::endpoint::{BodyPlan, RequestArgs, ResponsePlan};
use crate::error::{ApiClientError, ErrorContext};
use crate::media::EventStream;
use crate::multipart::{MultipartBody, MultipartFormat};
use crate::record::{RecordBody, RecordFormat};
use crate::sse::SseCodec;
use crate::stream_body::StreamBody;
use crate::stream_response::StreamResponse;
use crate::transport::{BuiltResponse, DecodedResponse};
use bytes::Bytes;
use std::any::Any;
use std::marker::PhantomData;

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
    pub supports_map: bool,
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
                supports_map: !C::is_no_content(),
                supports_pagination: !C::is_no_content(),
                is_streaming: false,
                is_no_content: C::is_no_content(),
            },
        })
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
                decode: decode_bytes_response,
            },
            capabilities: ResponseEntityCapabilities {
                supports_map: true,
                supports_pagination: false,
                is_streaming: false,
                is_no_content: false,
            },
        })
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
                decode: decode_no_content_response,
            },
            capabilities: ResponseEntityCapabilities {
                supports_map: false,
                supports_pagination: false,
                is_streaming: false,
                is_no_content: true,
            },
        })
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
                decode: decode_streaming_response,
            },
            capabilities: ResponseEntityCapabilities {
                supports_map: false,
                supports_pagination: false,
                is_streaming: true,
                is_no_content: false,
            },
        })
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
                decode: decode_streaming_response,
            },
            capabilities: ResponseEntityCapabilities {
                supports_map: false,
                supports_pagination: false,
                is_streaming: true,
                is_no_content: false,
            },
        })
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
                decode: decode_streaming_response,
            },
            capabilities: ResponseEntityCapabilities {
                supports_map: false,
                supports_pagination: false,
                is_streaming: true,
                is_no_content: false,
            },
        })
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
                decode: decode_streaming_response,
            },
            capabilities: ResponseEntityCapabilities {
                supports_map: false,
                supports_pagination: false,
                is_streaming: true,
                is_no_content: false,
            },
        })
    }
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
        decode: decode_buffered_response::<C>,
    })
}

fn decode_buffered_response<C>(
    resp: BuiltResponse,
    ctx: ErrorContext,
) -> Result<Box<dyn Any + Send>, ApiClientError>
where
    C: ResponseCodec,
{
    let content_type = resp
        .headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok());
    let decoded = C::decode(
        resp.body.clone(),
        DecodeContext::new(ctx.endpoint, &ctx.method, resp.status, content_type),
    )
    .map_err(|source| {
        ApiClientError::decode_error(ctx.clone(), resp.status, content_type, source)
    })?;
    Ok(Box::new(DecodedResponse {
        meta: resp.meta,
        url: resp.url,
        status: resp.status,
        headers: resp.headers,
        value: decoded,
    }))
}

fn decode_bytes_response(
    resp: BuiltResponse,
    _ctx: ErrorContext,
) -> Result<Box<dyn Any + Send>, ApiClientError> {
    Ok(Box::new(DecodedResponse {
        meta: resp.meta,
        url: resp.url,
        status: resp.status,
        headers: resp.headers,
        value: resp.body,
    }))
}

fn decode_no_content_response(
    resp: BuiltResponse,
    _ctx: ErrorContext,
) -> Result<Box<dyn Any + Send>, ApiClientError> {
    Ok(Box::new(DecodedResponse {
        meta: resp.meta,
        url: resp.url,
        status: resp.status,
        headers: resp.headers,
        value: (),
    }))
}

fn decode_streaming_response(
    _resp: BuiltResponse,
    ctx: ErrorContext,
) -> Result<Box<dyn Any + Send>, ApiClientError> {
    Err(ApiClientError::PolicyViolation {
        ctx,
        msg: "streaming response adapters do not use buffered decode execution",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::text::Text;
    use crate::codec::{BodyCodec, ContentType, EncodeContext, EncodedBody, Format};
    use crate::media::{EventStream, OctetStream, TextContentType};
    use crate::multipart::{FormData, MultipartBody};
    use crate::record::NdJson;
    use crate::transport::{BuiltResponse, RequestMeta};
    use http::{HeaderMap, Method, StatusCode};
    use serde::{Deserialize, Serialize};
    use url::Url;

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

    fn built_response(body: Bytes, content_type: Option<&str>) -> BuiltResponse {
        let mut headers = HeaderMap::new();
        if let Some(content_type) = content_type {
            headers.insert(
                http::header::CONTENT_TYPE,
                http::HeaderValue::from_str(content_type).expect("valid test content type"),
            );
        }
        BuiltResponse {
            meta: RequestMeta {
                endpoint: "Example",
                method: Method::POST,
                idempotent: true,
                attempt: 0,
                page_index: 0,
            },
            url: Url::parse("https://example.test/items").expect("url"),
            status: StatusCode::OK,
            headers,
            body,
            rate_limit: Default::default(),
        }
    }

    fn downcast_response<T: Send + 'static>(value: Box<dyn Any + Send>) -> DecodedResponse<T> {
        *value
            .downcast::<DecodedResponse<T>>()
            .expect("decoded response type")
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
                supports_map: true,
                supports_pagination: true,
                is_streaming: false,
                is_no_content: false,
            }
        );
        assert_eq!(
            plan.response_plan.accept,
            Some(JsonContentType::try_header_value().expect("json content type"))
        );
        let decoded = downcast_response::<Item>(
            (plan.response_plan.decode)(
                built_response(Bytes::from_static(br#"{"id":1}"#), Some("application/json")),
                ctx(),
            )
            .expect("decode"),
        );
        assert_eq!(decoded.value, Item { id: 1 });
    }

    #[test]
    fn buffered_response_text_is_buffered_and_non_streaming() {
        let plan = BufferedResponse::<Text<String>>::plan(ctx()).expect("buffered response");
        assert_eq!(
            plan.capabilities,
            ResponseEntityCapabilities {
                supports_map: true,
                supports_pagination: true,
                is_streaming: false,
                is_no_content: false,
            }
        );
        let decoded = downcast_response::<String>(
            (plan.response_plan.decode)(
                built_response(
                    Bytes::from_static(b"hello"),
                    Some("text/plain; charset=utf-8"),
                ),
                ctx(),
            )
            .expect("decode"),
        );
        assert_eq!(decoded.value, "hello");
    }

    #[test]
    fn bytes_response_exposes_buffered_bytes_capabilities() {
        let plan = BytesResponse::plan(ctx()).expect("bytes response");
        assert_eq!(
            plan.capabilities,
            ResponseEntityCapabilities {
                supports_map: true,
                supports_pagination: false,
                is_streaming: false,
                is_no_content: false,
            }
        );
        let decoded = downcast_response::<Bytes>(
            (plan.response_plan.decode)(built_response(Bytes::from_static(b"bytes"), None), ctx())
                .expect("decode"),
        );
        assert_eq!(decoded.value, Bytes::from_static(b"bytes"));
    }

    #[test]
    fn no_content_response_has_no_content_capabilities() {
        let plan = NoContentResponse::plan(ctx()).expect("no content response");
        assert_eq!(
            plan.capabilities,
            ResponseEntityCapabilities {
                supports_map: false,
                supports_pagination: false,
                is_streaming: false,
                is_no_content: true,
            }
        );
        let decoded = downcast_response::<()>(
            (plan.response_plan.decode)(built_response(Bytes::new(), None), ctx()).expect("decode"),
        );
        assert_eq!(decoded.value, ());
    }

    #[test]
    fn streaming_response_adapter_reports_streaming_capabilities() {
        let plan = RawStreamResponse::<OctetStream>::plan(ctx()).expect("stream response");
        assert_eq!(
            plan.capabilities,
            ResponseEntityCapabilities {
                supports_map: false,
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
                supports_map: false,
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

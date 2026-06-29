use bytes::Bytes;
use concord_core::advanced::{
    BodyCodec, CodecError, ContentType, DecodeContext, EncodeContext, EncodedBody, ResponseCodec,
};
use concord_core::prelude::ApiClientError;
use http::HeaderValue;
use std::marker::PhantomData;

#[derive(Debug)]
struct RequestOnlyContent;

impl ContentType for RequestOnlyContent {
    const CONTENT_TYPE: &'static str = "application/x-request-only";
}

#[derive(Debug)]
struct RequestOnly;

impl BodyCodec for RequestOnly {
    type Value = String;
    type Content = RequestOnlyContent;

    fn encode(value: Self::Value, _ctx: EncodeContext<'_>) -> Result<EncodedBody, CodecError> {
        Ok(EncodedBody::from_bytes(value))
    }
}

#[derive(Debug)]
struct RequestOmitContent;

impl ContentType for RequestOmitContent {
    const CONTENT_TYPE: &'static str = "application/x-request-omit";
}

#[derive(Debug)]
struct RequestOmit;

impl BodyCodec for RequestOmit {
    type Value = String;
    type Content = RequestOmitContent;

    fn try_content_type() -> Result<Option<HeaderValue>, http::header::InvalidHeaderValue> {
        Ok(None)
    }

    fn encode(value: Self::Value, _ctx: EncodeContext<'_>) -> Result<EncodedBody, CodecError> {
        Ok(EncodedBody::from_bytes(value))
    }
}

#[derive(Debug)]
struct ResponseOnlyContent;

impl ContentType for ResponseOnlyContent {
    const CONTENT_TYPE: &'static str = "application/x-response-only";
}

#[derive(Debug)]
struct ResponseOnly;

impl ResponseCodec for ResponseOnly {
    type Value = String;
    type Content = ResponseOnlyContent;

    fn decode(bytes: Bytes, _ctx: DecodeContext<'_>) -> Result<Self::Value, CodecError> {
        String::from_utf8(bytes.to_vec())
            .map_err(|err| CodecError::with_source("response-only decode failed", err))
    }
}

#[derive(Debug)]
struct ResponseOmitContent;

impl ContentType for ResponseOmitContent {
    const CONTENT_TYPE: &'static str = "application/x-response-omit";
}

#[derive(Debug)]
struct ResponseOmit;

impl ResponseCodec for ResponseOmit {
    type Value = String;
    type Content = ResponseOmitContent;

    fn try_accept() -> Result<Option<HeaderValue>, http::header::InvalidHeaderValue> {
        Ok(None)
    }

    fn decode(bytes: Bytes, _ctx: DecodeContext<'_>) -> Result<Self::Value, CodecError> {
        String::from_utf8(bytes.to_vec())
            .map_err(|err| CodecError::with_source("response-omit decode failed", err))
    }
}

#[derive(Debug)]
struct BothContent;

impl ContentType for BothContent {
    const CONTENT_TYPE: &'static str = "application/x-both";
}

#[derive(Debug)]
struct Both<T>(PhantomData<T>);

impl<T> BodyCodec for Both<T>
where
    T: AsRef<[u8]> + Send + Sync + 'static,
{
    type Value = T;
    type Content = BothContent;

    fn encode(value: Self::Value, _ctx: EncodeContext<'_>) -> Result<EncodedBody, CodecError> {
        Ok(EncodedBody::from_bytes(Bytes::copy_from_slice(
            value.as_ref(),
        )))
    }
}

impl ResponseCodec for Both<String> {
    type Value = String;
    type Content = BothContent;

    fn decode(bytes: Bytes, _ctx: DecodeContext<'_>) -> Result<Self::Value, CodecError> {
        String::from_utf8(bytes.to_vec())
            .map_err(|err| CodecError::with_source("both decode failed", err))
    }
}

#[derive(Debug)]
struct InvalidContentType;

impl ContentType for InvalidContentType {
    const CONTENT_TYPE: &'static str = "bad\nvalue";
}

#[derive(Debug)]
struct InvalidRequestContent;

impl BodyCodec for InvalidRequestContent {
    type Value = String;
    type Content = InvalidContentType;

    fn encode(value: Self::Value, _ctx: EncodeContext<'_>) -> Result<EncodedBody, CodecError> {
        Ok(EncodedBody::from_bytes(value))
    }
}

#[derive(Debug)]
struct InvalidResponseContent;

impl ResponseCodec for InvalidResponseContent {
    type Value = String;
    type Content = InvalidContentType;

    fn decode(bytes: Bytes, _ctx: DecodeContext<'_>) -> Result<Self::Value, CodecError> {
        String::from_utf8(bytes.to_vec())
            .map_err(|err| CodecError::with_source("invalid-content decode failed", err))
    }
}

fn encode_ctx() -> EncodeContext<'static> {
    static METHOD: http::Method = http::Method::POST;
    EncodeContext::new("Test", &METHOD)
}

fn decode_ctx() -> DecodeContext<'static> {
    static METHOD: http::Method = http::Method::GET;
    DecodeContext::new("Test", &METHOD, http::StatusCode::OK, Some("text/plain"))
}

#[test]
fn custom_request_only_codec_implements_public_body_trait() {
    let body = RequestOnly::encode("hello".to_string(), encode_ctx()).expect("encode");

    assert_eq!(body.content_len(), Some(5));
    assert_eq!(
        RequestOnly::content_type(),
        Some(HeaderValue::from_static("application/x-request-only"))
    );
}

#[test]
fn custom_response_only_codec_implements_public_response_trait() {
    let value = ResponseOnly::decode(Bytes::from_static(b"hello"), decode_ctx()).expect("decode");

    assert_eq!(value, "hello");
    assert_eq!(
        ResponseOnly::accept(),
        Some(HeaderValue::from_static("application/x-response-only"))
    );
}

#[test]
fn custom_bidirectional_codec_can_use_generic_marker_type() {
    let body = Both::<Vec<u8>>::encode(b"abc".to_vec(), encode_ctx()).expect("encode");
    let value = Both::<String>::decode(Bytes::from_static(b"abc"), decode_ctx()).expect("decode");

    assert_eq!(body.content_len(), Some(3));
    assert_eq!(value, "abc");
}

#[test]
fn custom_request_codec_can_omit_content_type() {
    assert_eq!(
        RequestOmit::try_content_type().expect("try content type"),
        None
    );
    assert_eq!(RequestOmit::content_type(), None);
}

#[test]
fn custom_response_codec_can_omit_accept() {
    assert_eq!(ResponseOmit::try_accept().expect("try accept"), None);
    assert_eq!(ResponseOmit::accept(), None);
}

#[test]
fn invalid_custom_content_type_returns_typed_error() {
    assert!(InvalidRequestContent::try_content_type().is_err());
    assert!(InvalidResponseContent::try_accept().is_err());
}

#[test]
fn codec_error_converts_into_api_client_error() {
    let err = ApiClientError::codec_error(
        concord_core::advanced::ErrorContext {
            endpoint: "Test",
            method: http::Method::POST,
        },
        CodecError::new("bad codec"),
    );

    assert_eq!(err.category(), concord_core::prelude::ErrorCategory::Decode);
    assert!(err.to_string().contains("bad codec"));
}

#[cfg(feature = "json")]
use concord_core::advanced::{JsonContentType, TextContentType};
#[cfg(feature = "json")]
use concord_core::prelude::Json;
#[cfg(feature = "json")]
use concord_core::prelude::{NoContent, Text};
#[cfg(feature = "json")]
use serde::{Deserialize, Serialize};

#[cfg(feature = "json")]
#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct JsonValue {
    id: u64,
}

#[cfg(feature = "json")]
#[test]
fn built_in_json_text_and_no_content_use_codec_traits() {
    let json = Json::<JsonValue>::encode(JsonValue { id: 7 }, encode_ctx()).expect("json encode");
    let decoded =
        Json::<JsonValue>::decode(Bytes::from_static(br#"{"id":7}"#), decode_ctx()).expect("json");
    let text = Text::<String>::encode("hello".to_string(), encode_ctx()).expect("text encode");
    let decoded_text =
        Text::<String>::decode(Bytes::from_static(b"hello"), decode_ctx()).expect("text decode");
    NoContent::decode(Bytes::new(), decode_ctx()).expect("no content");

    assert_eq!(json.content_len(), Some(8));
    assert_eq!(
        Json::<JsonValue>::try_content_type().expect("json content type"),
        Some(JsonContentType::try_header_value().expect("json marker"))
    );
    assert_eq!(decoded, JsonValue { id: 7 });
    assert_eq!(text.content_len(), Some(5));
    assert_eq!(
        Text::<String>::try_accept().expect("text accept"),
        Some(TextContentType::try_header_value().expect("text marker"))
    );
    assert_eq!(decoded_text, "hello");
}

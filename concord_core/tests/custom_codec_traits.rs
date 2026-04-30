use bytes::Bytes;
use concord_core::advanced::{
    BodyCodec, CodecError, DecodeContext, EncodeContext, EncodedBody, ResponseCodec,
};
use concord_core::prelude::{ApiClientError, Json, NoContent, Text};
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;

#[derive(Debug)]
struct RequestOnly;

impl BodyCodec for RequestOnly {
    type Value = String;

    fn content_type() -> &'static str {
        "application/x-request-only"
    }

    fn encode(value: &Self::Value, _ctx: EncodeContext) -> Result<EncodedBody, CodecError> {
        Ok(EncodedBody::from_bytes(value.clone()).with_content_type(Self::content_type()))
    }
}

#[derive(Debug)]
struct ResponseOnly;

impl ResponseCodec for ResponseOnly {
    type Value = String;

    fn accept() -> &'static str {
        "application/x-response-only"
    }

    fn decode(bytes: &Bytes, _ctx: DecodeContext) -> Result<Self::Value, CodecError> {
        String::from_utf8(bytes.to_vec())
            .map_err(|err| CodecError::with_source("response-only decode failed", err))
    }
}

#[derive(Debug)]
struct Both<T>(PhantomData<T>);

impl<T> BodyCodec for Both<T>
where
    T: AsRef<[u8]> + Send + Sync + 'static,
{
    type Value = T;

    fn content_type() -> &'static str {
        "application/x-both"
    }

    fn encode(value: &Self::Value, _ctx: EncodeContext) -> Result<EncodedBody, CodecError> {
        Ok(
            EncodedBody::from_bytes(Bytes::copy_from_slice(value.as_ref()))
                .with_content_type(Self::content_type()),
        )
    }
}

impl ResponseCodec for Both<String> {
    type Value = String;

    fn accept() -> &'static str {
        "application/x-both"
    }

    fn decode(bytes: &Bytes, _ctx: DecodeContext) -> Result<Self::Value, CodecError> {
        String::from_utf8(bytes.to_vec())
            .map_err(|err| CodecError::with_source("both decode failed", err))
    }
}

fn encode_ctx() -> EncodeContext {
    EncodeContext {
        endpoint: "Test",
        method: http::Method::POST,
    }
}

fn decode_ctx() -> DecodeContext {
    DecodeContext {
        endpoint: "Test",
        method: http::Method::GET,
        status: http::StatusCode::OK,
        content_type: Some("text/plain".to_string()),
    }
}

#[test]
fn custom_request_only_codec_implements_public_body_trait() {
    let body = RequestOnly::encode(&"hello".to_string(), encode_ctx()).expect("encode");

    assert_eq!(body.content_len(), 5);
    assert_eq!(RequestOnly::content_type(), "application/x-request-only");
}

#[test]
fn custom_response_only_codec_implements_public_response_trait() {
    let value = ResponseOnly::decode(&Bytes::from_static(b"hello"), decode_ctx()).expect("decode");

    assert_eq!(value, "hello");
    assert_eq!(ResponseOnly::accept(), "application/x-response-only");
}

#[test]
fn custom_bidirectional_codec_can_use_generic_marker_type() {
    let body = Both::<Vec<u8>>::encode(&b"abc".to_vec(), encode_ctx()).expect("encode");
    let value = Both::<String>::decode(&Bytes::from_static(b"abc"), decode_ctx()).expect("decode");

    assert_eq!(body.content_len(), 3);
    assert_eq!(value, "abc");
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

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct JsonValue {
    id: u64,
}

#[test]
fn built_in_json_text_and_no_content_use_codec_traits() {
    let json = Json::<JsonValue>::encode(&JsonValue { id: 7 }, encode_ctx()).expect("json encode");
    let decoded =
        Json::<JsonValue>::decode(&Bytes::from_static(br#"{"id":7}"#), decode_ctx()).expect("json");
    let text = Text::<String>::encode(&"hello".to_string(), encode_ctx()).expect("text encode");
    let decoded_text =
        Text::<String>::decode(&Bytes::from_static(b"hello"), decode_ctx()).expect("text decode");
    NoContent::decode(&Bytes::new(), decode_ctx()).expect("no content");

    assert_eq!(json.content_len(), 8);
    assert_eq!(decoded, JsonValue { id: 7 });
    assert_eq!(text.content_len(), 5);
    assert_eq!(decoded_text, "hello");
}

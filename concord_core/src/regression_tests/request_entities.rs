use crate::regression_tests::test_api::Format;
#[cfg(feature = "multipart")]
use crate::regression_tests::test_api::MultipartRequest;
use crate::regression_tests::test_api::{NoRequestBody, RawStreamRequest, RequestEntity};
use bytes::Bytes;
#[cfg(feature = "multipart")]
use concord_core::advanced::MultipartBody;
use concord_core::advanced::{
    BodyCodec, CodecError, ContentType, EncodeContext, EncodedBody, ErrorContext, OctetStream,
    StreamBody,
};
use http::Method;
use std::error::Error;
use std::fmt;

#[derive(Clone, Copy, Debug, Default)]
struct RequestCodecContent;

impl ContentType for RequestCodecContent {
    const CONTENT_TYPE: &'static str = "text/plain";
}

#[allow(dead_code)]
struct SentinelError(&'static str);

impl fmt::Debug for SentinelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = self.0;
        f.write_str("<redacted>")
    }
}

impl fmt::Display for SentinelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = self.0;
        f.write_str("<redacted>")
    }
}

impl Error for SentinelError {}

#[derive(Clone, Copy, Debug, Default)]
struct FailingBodyCodec;

impl BodyCodec for FailingBodyCodec {
    type Value = String;
    type Content = RequestCodecContent;

    fn format() -> Format {
        Format::Text
    }

    fn encode(_value: Self::Value, _ctx: EncodeContext<'_>) -> Result<EncodedBody, CodecError> {
        Err(CodecError::with_source(
            "request body encoding failed",
            SentinelError("REQUEST_ENTITY_SENTINEL"),
        ))
    }
}

fn ctx() -> ErrorContext {
    ErrorContext {
        endpoint: "Example",
        method: Method::POST,
    }
}

#[test]
fn no_request_body_prepares_empty_body() {
    let prepared = NoRequestBody::prepare((), ctx()).expect("no request body");

    assert!(prepared.body.is_replayable());
    assert_eq!(prepared.body.size_hint().exact(), Some(0));
}

#[test]
fn encoded_request_prepares_buffered_bytes() {
    let prepared = crate::regression_tests::test_api::EncodedRequest::<
        concord_core::prelude::Text<String>,
    >::prepare("hello".to_string(), ctx())
    .expect("encoded request");

    let rendered = prepared
        .body
        .media_type()
        .and_then(|value| value.to_str().ok())
        .expect("text content type");
    assert!(rendered.starts_with("text/plain"));
    assert_eq!(prepared.body.size_hint().exact(), Some(5));
    assert!(prepared.body.is_replayable());
}

#[test]
fn raw_stream_request_prepares_stream_body() {
    let prepared = RawStreamRequest::<OctetStream>::prepare(
        StreamBody::from_bytes(Bytes::from_static(b"raw-body")),
        ctx(),
    )
    .expect("raw stream request");

    assert_eq!(
        prepared.body.media_type(),
        Some(&http::HeaderValue::from_static("application/octet-stream"))
    );
    assert!(!prepared.body.is_replayable());
    assert_eq!(prepared.body.size_hint().exact(), Some(8));
}

#[test]
#[cfg(feature = "multipart")]
fn multipart_request_prepares_stream_body_and_content_type() {
    let prepared = MultipartRequest::prepare(
        MultipartBody::new()
            .text("title", "hello")
            .bytes("file", Bytes::from_static(b"abc")),
        ctx(),
    )
    .expect("multipart request");

    // The recipe reserves this Concord-owned header before auth preflight;
    // Reqwest chooses the boundary only when the native form is materialized.
    assert!(prepared.body.reserves_content_type());
    assert!(prepared.body.media_type().is_none());
    assert!(prepared.body.is_replayable());
}

#[test]
fn request_entity_codec_errors_hide_sentinels() {
    let err = crate::regression_tests::test_api::EncodedRequest::<FailingBodyCodec>::prepare(
        "ignored".to_string(),
        ctx(),
    )
    .expect_err("encode failure should surface as codec error");

    assert!(matches!(
        err,
        concord_core::prelude::ApiClientError::Codec { .. }
    ));
    assert_eq!(err.category(), concord_core::prelude::ErrorCategory::Decode);
    assert_eq!(err.context().endpoint, "Example");
    assert_eq!(err.context().method, Method::POST);
    let rendered = format!("{err}");
    assert!(rendered.contains("request body encoding failed"));
    assert!(!rendered.contains("REQUEST_ENTITY_SENTINEL"));
    crate::support::assert_error_chain_does_not_contain_any(&err, &["REQUEST_ENTITY_SENTINEL"]);
}

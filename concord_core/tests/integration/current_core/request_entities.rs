use bytes::Bytes;
use concord_core::advanced::{
    BodyCodec, CodecError, ContentType, EncodeContext, EncodedBody, ErrorContext, FormData,
    MultipartBody, MultipartRequest, NdJson, NoRequestBody, OctetStream, RawStreamRequest,
    RecordBody, RecordRequest, RequestEntity, StreamBody,
};
use concord_core::internal::{BodyPlan, Format, Replayability};
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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct RecordRow {
    value: String,
}

#[test]
fn no_request_body_prepares_empty_body() {
    let prepared = NoRequestBody::prepare((), ctx()).expect("no request body");

    assert!(matches!(prepared.body_plan, BodyPlan::None));
    assert!(prepared.args.body.is_empty());
    assert!(matches!(prepared.replayability, Replayability::Replayable));
}

#[test]
fn encoded_request_prepares_buffered_bytes() {
    let prepared =
        concord_core::advanced::EncodedRequest::<concord_core::prelude::Text<String>>::prepare(
            "hello".to_string(),
            ctx(),
        )
        .expect("encoded request");

    match prepared.body_plan {
        BodyPlan::Encoded {
            content_type,
            format,
        } => {
            let rendered = content_type
                .as_ref()
                .and_then(|value| value.to_str().ok())
                .expect("text content type");
            assert!(rendered.starts_with("text/plain"));
            assert_eq!(format, Format::Text);
        }
        other => panic!("expected encoded body plan, got {other:?}"),
    }
    assert!(prepared.args.body.is_bytes());
    assert_eq!(
        prepared.args.body.as_bytes().map(|body| body.as_ref()),
        Some(b"hello".as_slice())
    );
    assert!(matches!(prepared.replayability, Replayability::Replayable));
}

#[test]
fn raw_stream_request_prepares_stream_body() {
    let prepared = RawStreamRequest::<OctetStream>::prepare(
        StreamBody::from_bytes(Bytes::from_static(b"raw-body")),
        ctx(),
    )
    .expect("raw stream request");

    match prepared.body_plan {
        BodyPlan::RawStream { content_type } => {
            assert_eq!(
                content_type,
                http::HeaderValue::from_static("application/octet-stream")
            );
        }
        other => panic!("expected raw stream body plan, got {other:?}"),
    }
    assert!(prepared.args.body.is_stream());
    assert!(matches!(
        prepared.replayability,
        Replayability::NonReplayable
    ));
}

#[test]
fn record_request_prepares_ndjson_stream_body() {
    let prepared = RecordRequest::<RecordRow, NdJson>::prepare(
        RecordBody::from_iter([
            RecordRow {
                value: "first".to_string(),
            },
            RecordRow {
                value: "second".to_string(),
            },
        ]),
        ctx(),
    )
    .expect("record request");

    match prepared.body_plan {
        BodyPlan::Records {
            content_type,
            format,
        } => {
            assert_eq!(
                content_type,
                http::HeaderValue::from_static("application/x-ndjson")
            );
            assert_eq!(format, Format::Text);
        }
        other => panic!("expected record body plan, got {other:?}"),
    }
    assert!(prepared.args.body.is_stream());
    assert!(matches!(
        prepared.replayability,
        Replayability::NonReplayable
    ));
}

#[test]
fn multipart_request_prepares_stream_body_and_content_type() {
    let prepared = MultipartRequest::<FormData>::prepare(
        MultipartBody::new()
            .text("title", "hello")
            .bytes("file", Bytes::from_static(b"abc")),
        ctx(),
    )
    .expect("multipart request");

    match prepared.body_plan {
        BodyPlan::Multipart {
            content_type,
            format,
        } => {
            let rendered = content_type
                .to_str()
                .expect("multipart content type should be valid");
            assert!(rendered.starts_with("multipart/form-data; boundary="));
            assert_eq!(format, Format::Text);
        }
        other => panic!("expected multipart body plan, got {other:?}"),
    }
    assert!(prepared.args.body.is_stream());
    assert!(matches!(
        prepared.replayability,
        Replayability::NonReplayable
    ));
}

#[test]
fn request_entity_codec_errors_hide_sentinels() {
    let err = concord_core::advanced::EncodedRequest::<FailingBodyCodec>::prepare(
        "ignored".to_string(),
        ctx(),
    )
    .expect_err("encode failure should surface as codec error");

    assert!(matches!(
        err,
        concord_core::prelude::ApiClientError::Codec { .. }
    ));
    assert_eq!(err.category(), concord_core::prelude::ErrorCategory::Decode);
    let rendered = format!("{err}");
    assert!(!rendered.contains("REQUEST_ENTITY_SENTINEL"));
    crate::support::assert_error_chain_does_not_contain_any(&err, &["REQUEST_ENTITY_SENTINEL"]);
}

use super::common::{MockResponse, MockTransport, TestAuthVars, client};
use bytes::Bytes;
use concord_core::advanced::{
    BodyCodec, CodecError, ContentType, DecodeContext, EncodeContext, EncodedBody, ErrorContext,
    RequestEntity, ResponseCodec, ResponseEntity,
};
use concord_core::prelude::{ApiClientError, ErrorCategory};
use http::Method;
use std::error::Error;
use std::fmt;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone, Copy, Debug, Default)]
struct InvalidRequestContentType;

impl ContentType for InvalidRequestContentType {
    const CONTENT_TYPE: &'static str = "bad\nvalue";
}

#[derive(Clone, Copy, Debug, Default)]
struct InvalidRequestCodec;

impl BodyCodec for InvalidRequestCodec {
    type Value = String;
    type Content = InvalidRequestContentType;

    fn encode(_value: Self::Value, _ctx: EncodeContext<'_>) -> Result<EncodedBody, CodecError> {
        Ok(EncodedBody::from_bytes(Bytes::from_static(b"body")).text())
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct ErrorRequestContentType;

impl ContentType for ErrorRequestContentType {
    const CONTENT_TYPE: &'static str = "text/plain";
}

#[derive(Clone, Copy, Debug, Default)]
struct ErrorResponseContentType;

impl ContentType for ErrorResponseContentType {
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
struct FailingRequestCodec;

impl BodyCodec for FailingRequestCodec {
    type Value = String;
    type Content = ErrorRequestContentType;

    fn encode(_value: Self::Value, _ctx: EncodeContext<'_>) -> Result<EncodedBody, CodecError> {
        Err(CodecError::with_source(
            "request body encoding failed",
            SentinelError("REQUEST_ERROR_SENTINEL"),
        ))
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct FailingResponseCodec;

impl ResponseCodec for FailingResponseCodec {
    type Value = String;
    type Content = ErrorResponseContentType;

    fn decode(_bytes: Bytes, _ctx: DecodeContext<'_>) -> Result<Self::Value, CodecError> {
        Err(CodecError::with_source(
            "response body decoding failed",
            SentinelError("RESPONSE_ERROR_SENTINEL"),
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
fn invalid_request_content_type_is_config_error() {
    let err = concord_core::advanced::EncodedRequest::<InvalidRequestCodec>::prepare(
        "body".to_string(),
        ctx(),
    )
    .expect_err("invalid request content type should fail");

    assert!(matches!(err, ApiClientError::InvalidParam { .. }));
    assert_eq!(err.category(), ErrorCategory::Config);
}

#[test]
fn request_entity_encoding_error_hides_sentinels() {
    let err = concord_core::advanced::EncodedRequest::<FailingRequestCodec>::prepare(
        "body".to_string(),
        ctx(),
    )
    .expect_err("request codec failure should surface");

    assert!(matches!(err, ApiClientError::Codec { .. }));
    assert_eq!(err.category(), ErrorCategory::Decode);
    crate::support::assert_error_chain_does_not_contain_any(&err, &["REQUEST_ERROR_SENTINEL"]);
}

#[tokio::test]
async fn response_entity_decode_error_hides_sentinels() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![MockResponse::text(http::StatusCode::OK, "ignored")],
    );
    let client = client(TestAuthVars::default(), transport);
    let plan = super::common::request_plan(
        "ResponseEntity",
        Method::GET,
        "/response",
        concord_core::internal::ResolvedPolicy::default(),
        None,
    );

    let err =
        concord_core::advanced::BufferedResponse::<FailingResponseCodec>::execute(&client, plan)
            .await
            .expect_err("response codec failure should surface");

    assert!(matches!(err, ApiClientError::Decode { .. }));
    assert_eq!(err.category(), ErrorCategory::Decode);
    crate::support::assert_error_chain_does_not_contain_any(&err, &["RESPONSE_ERROR_SENTINEL"]);
}

#[tokio::test]
async fn no_content_status_mismatch_is_response_contract_error() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![MockResponse::text(http::StatusCode::NO_CONTENT, "ignored")],
    );
    let client = client(TestAuthVars::default(), transport);
    let plan = super::common::request_plan(
        "Text",
        Method::GET,
        "/text",
        concord_core::internal::ResolvedPolicy::default(),
        None,
    );

    let err =
        concord_core::advanced::BufferedResponse::<concord_core::prelude::Text<String>>::execute(
            &client, plan,
        )
        .await
        .expect_err("no-content status mismatch should fail");

    assert!(matches!(
        err,
        ApiClientError::NoContentStatusRequiresNoContent { .. }
    ));
    assert_eq!(err.category(), ErrorCategory::ResponseContract);
}

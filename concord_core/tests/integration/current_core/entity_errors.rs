use super::common::{MockResponse, MockTransport, TestAuthVars, client};
use bytes::Bytes;
use concord_core::advanced::{
    BodyCodec, CodecError, ContentType, DecodeContext, EncodeContext, EncodedBody, ErrorContext,
    RequestEntity, ResponseCodec, ResponseEntity,
};
use concord_core::internal::{RequestPlan, ResolvedPolicy};
use concord_core::prelude::{ApiClientError, Endpoint, ErrorCategory, ReusableEndpoint};
use http::Method;
use std::error::Error;
use std::fmt;
use std::marker::PhantomData;
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

#[derive(Clone, Copy, Debug, Default)]
struct RequestBodyMessageSentinelContentType;

impl ContentType for RequestBodyMessageSentinelContentType {
    const CONTENT_TYPE: &'static str = "text/plain";
}

#[derive(Clone, Copy, Debug, Default)]
struct RequestBodySourceSentinelContentType;

impl ContentType for RequestBodySourceSentinelContentType {
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
struct RequestBodyMessageSentinelCodec;

impl BodyCodec for RequestBodyMessageSentinelCodec {
    type Value = String;
    type Content = RequestBodyMessageSentinelContentType;

    fn encode(_value: Self::Value, _ctx: EncodeContext<'_>) -> Result<EncodedBody, CodecError> {
        Err(CodecError::new("LEAK_SENTINEL_REQUEST_BODY"))
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct RequestBodySourceSentinelCodec;

impl BodyCodec for RequestBodySourceSentinelCodec {
    type Value = String;
    type Content = RequestBodySourceSentinelContentType;

    fn encode(_value: Self::Value, _ctx: EncodeContext<'_>) -> Result<EncodedBody, CodecError> {
        Err(CodecError::with_source(
            "request body encoding failed",
            SentinelError("LEAK_SENTINEL_CODEC_SOURCE"),
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

#[derive(Clone)]
struct BufferedEncodeFailureEndpoint<C> {
    body: String,
    name: &'static str,
    path: &'static str,
    _marker: PhantomData<C>,
}

impl<C> BufferedEncodeFailureEndpoint<C> {
    fn new(name: &'static str, path: &'static str, body: impl Into<String>) -> Self {
        Self {
            body: body.into(),
            name,
            path,
            _marker: PhantomData,
        }
    }
}

impl<C> Endpoint<super::common::TestCx> for BufferedEncodeFailureEndpoint<C>
where
    C: BodyCodec<Value = String>,
{
    type Response = String;

    fn execute<'a, T>(
        client: &'a concord_core::prelude::ApiClient<super::common::TestCx, T>,
        plan: RequestPlan,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, ApiClientError>> + Send + 'a>,
    >
    where
        T: concord_core::advanced::Transport + 'a,
    {
        super::common::execute_buffered::<_, _, concord_core::prelude::Text<String>>(client, plan)
    }
}

impl<C> ReusableEndpoint<super::common::TestCx> for BufferedEncodeFailureEndpoint<C>
where
    C: BodyCodec<Value = String>,
{
    fn plan(
        &self,
        _ctx: &concord_core::internal::ClientPlanContext<'_, super::common::TestCx>,
    ) -> Result<RequestPlan, ApiClientError> {
        let mut plan = super::common::request_plan(
            self.name,
            Method::POST,
            self.path,
            ResolvedPolicy::default(),
            None,
        );
        let prepared = concord_core::advanced::EncodedRequest::<C>::prepare(
            self.body.clone(),
            ErrorContext {
                endpoint: self.name,
                method: Method::POST,
            },
        )?;
        plan.endpoint.body = prepared.body_plan;
        plan.args = prepared.args;
        plan.replayability = prepared.replayability;
        Ok(plan)
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
    let rendered = format!("{err}\n{err:?}\n{err:#?}");
    assert!(rendered.contains("request body encoding failed"));
    crate::support::assert_error_chain_does_not_contain_any(&err, &["REQUEST_ERROR_SENTINEL"]);
}

#[tokio::test]
async fn buffered_request_encoding_failure_is_public_safe_through_client_request_path() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport =
        MockTransport::new(events, vec![MockResponse::text(http::StatusCode::OK, "ok")]);
    let sent = transport.clone();
    let client = client(
        TestAuthVars {
            token: Some("LEAK_SENTINEL_AUTH".to_string()),
            identity: "sentinel",
        },
        transport,
    );
    let err = client
        .request(BufferedEncodeFailureEndpoint::<
            RequestBodySourceSentinelCodec,
        >::new("RequestEncode", "/request-encode", "body"))
        .execute_decoded_with::<concord_core::prelude::Text<String>>()
        .await
        .expect_err("buffered request encoding should fail");

    assert!(matches!(err, ApiClientError::Codec { .. }));
    assert_eq!(err.category(), ErrorCategory::Decode);
    assert_eq!(err.context().endpoint, "RequestEncode");
    assert_eq!(err.context().method, Method::POST);
    let rendered = format!("{err}\n{err:?}\n{err:#?}");
    assert!(rendered.contains("request body encoding failed"));
    assert!(!rendered.contains("LEAK_SENTINEL_AUTH"));
    assert!(!rendered.contains("LEAK_SENTINEL_REQUEST_BODY"));
    assert!(!rendered.contains("LEAK_SENTINEL_CODEC_SOURCE"));
    crate::support::assert_error_chain_does_not_contain_any(
        &err,
        &[
            "LEAK_SENTINEL_AUTH",
            "LEAK_SENTINEL_REQUEST_BODY",
            "LEAK_SENTINEL_CODEC_SOURCE",
        ],
    );
    assert_eq!(sent.sent_count().await, 0);
}

#[tokio::test]
async fn buffered_request_encoding_message_sentinel_is_redacted_through_client_request_path() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport =
        MockTransport::new(events, vec![MockResponse::text(http::StatusCode::OK, "ok")]);
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);
    let err = client
        .request(BufferedEncodeFailureEndpoint::<
            RequestBodyMessageSentinelCodec,
        >::new("RequestEncode", "/request-encode", "body"))
        .execute_decoded_with::<concord_core::prelude::Text<String>>()
        .await
        .expect_err("buffered request encoding should fail");

    assert!(matches!(err, ApiClientError::Codec { .. }));
    assert_eq!(err.category(), ErrorCategory::Decode);
    assert_eq!(err.context().endpoint, "RequestEncode");
    assert_eq!(err.context().method, Method::POST);
    let rendered = format!("{err}\n{err:?}\n{err:#?}");
    assert!(rendered.contains("request body encoding failed"));
    assert!(!rendered.contains("LEAK_SENTINEL_REQUEST_BODY"));
    assert!(!rendered.contains("LEAK_SENTINEL_CODEC_SOURCE"));
    crate::support::assert_error_chain_does_not_contain_any(
        &err,
        &["LEAK_SENTINEL_REQUEST_BODY", "LEAK_SENTINEL_CODEC_SOURCE"],
    );
    assert_eq!(sent.sent_count().await, 0);
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

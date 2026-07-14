use bytes::Bytes;
use concord_core::advanced::{
    BodyCodec, CodecError, ContentType, DecodeContext, EncodeContext, EncodedBody, ResponseCodec,
};
use concord_core::prelude::{ApiClientError, Json};
use concord_macros::api;
use concord_test_support::{
    DeterministicMock, MockExecutionHandle, ScriptedReply, deterministic_mock,
};
use http::{HeaderValue, StatusCode};
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;
use std::sync::{Arc, Mutex as StdMutex};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct UploadResult {
    ok: bool,
}

#[derive(Debug)]
pub struct RequestOmitContent;

impl ContentType for RequestOmitContent {
    const CONTENT_TYPE: &'static str = "application/x-request-omit";
}

#[derive(Debug)]
pub struct RequestOmitCodec<T>(PhantomData<T>);

impl BodyCodec for RequestOmitCodec<String> {
    type Value = String;
    type Content = RequestOmitContent;

    fn try_content_type() -> Result<Option<HeaderValue>, http::header::InvalidHeaderValue> {
        Ok(None)
    }

    fn encode(value: Self::Value, _ctx: EncodeContext<'_>) -> Result<EncodedBody, CodecError> {
        Ok(EncodedBody::from_bytes(Bytes::from(value)))
    }
}

#[derive(Debug)]
pub struct ResponseOmitContent;

impl ContentType for ResponseOmitContent {
    const CONTENT_TYPE: &'static str = "application/x-response-omit";
}

#[derive(Debug)]
pub struct ResponseOmitCodec<T>(PhantomData<T>);

impl ResponseCodec for ResponseOmitCodec<String> {
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
pub struct InvalidContentType;

impl ContentType for InvalidContentType {
    const CONTENT_TYPE: &'static str = "bad\nvalue";
}

#[derive(Debug)]
pub struct InvalidRequestCodec<T>(PhantomData<T>);

impl BodyCodec for InvalidRequestCodec<String> {
    type Value = String;
    type Content = InvalidContentType;

    fn encode(value: Self::Value, _ctx: EncodeContext<'_>) -> Result<EncodedBody, CodecError> {
        Ok(EncodedBody::from_bytes(Bytes::from(value)))
    }
}

#[derive(Debug)]
pub struct InvalidResponseCodec<T>(PhantomData<T>);

impl ResponseCodec for InvalidResponseCodec<String> {
    type Value = String;
    type Content = InvalidContentType;

    fn decode(bytes: Bytes, _ctx: DecodeContext<'_>) -> Result<Self::Value, CodecError> {
        String::from_utf8(bytes.to_vec())
            .map_err(|err| CodecError::with_source("invalid-response decode failed", err))
    }
}

mod custom_codec_helper_contract {
    #![allow(unused_imports)]
    use super::*;

    api! {
        client CustomCodecHelperApi {
            base "https://example.com"
        }

        POST RequestOmit(body: RequestOmitCodec<String>)
            path ["request-omit"]
            -> Json<UploadResult>

        GET ResponseOmit
            path ["response-omit"]
            -> ResponseOmitCodec<String>

        POST InvalidRequest(body: InvalidRequestCodec<String>)
            path ["invalid-request"]
            -> Json<UploadResult>

        GET InvalidResponse
            path ["invalid-response"]
            -> InvalidResponseCodec<String>
    }

    pub(super) use custom_codec_helper_api::CustomCodecHelperApi;
}

use custom_codec_helper_contract::CustomCodecHelperApi;

#[derive(Clone)]
struct CapturedRequest {
    debug: String,
    content_type: Option<String>,
    accept: Option<String>,
    body_category: concord_core::__development::CapturedBodyCategory,
    known_body_length: Option<u64>,
}

impl std::fmt::Debug for CapturedRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CapturedRequest")
            .field("debug", &self.debug)
            .field("content_type", &self.content_type)
            .field("accept", &self.accept)
            .field("body_category", &self.body_category)
            .field("known_body_length", &self.known_body_length)
            .finish()
    }
}

#[derive(Clone)]
enum ResponseFixture {
    Json {
        status: StatusCode,
        body: Bytes,
    },
    Text {
        status: StatusCode,
        body: Bytes,
        content_type: &'static str,
    },
}

#[derive(Clone)]
struct RecordingTransport {
    server: DeterministicMock,
    handle: Arc<StdMutex<MockExecutionHandle>>,
}

impl RecordingTransport {
    fn new(response: ResponseFixture) -> Self {
        Self::from_replies([response.into_reply()])
    }

    fn new_expecting_body(response: ResponseFixture, body: Bytes) -> Self {
        Self::from_replies([response.into_reply().expect_body(body)])
    }

    fn empty() -> Self {
        Self::from_replies([])
    }

    fn from_replies(replies: impl IntoIterator<Item = ScriptedReply>) -> Self {
        let (server, handle) = deterministic_mock().replies(replies).build();
        Self {
            server,
            handle: Arc::new(StdMutex::new(handle)),
        }
    }

    fn requests(&self) -> Vec<CapturedRequest> {
        self.handle
            .lock()
            .expect("handle lock")
            .recorded()
            .into_iter()
            .map(|request| CapturedRequest {
                debug: "Request { body: <body>, .. }".to_string(),
                content_type: request
                    .headers
                    .get(http::header::CONTENT_TYPE)
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_string),
                accept: request
                    .headers
                    .get(http::header::ACCEPT)
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_string),
                body_category: request.body_category,
                known_body_length: request.known_body_length,
            })
            .collect()
    }

    fn send_count(&self) -> usize {
        self.handle.lock().expect("handle lock").recorded_len()
    }

    fn configure(
        &self,
        builder: concord_core::advanced::SafeReqwestBuilder,
    ) -> concord_core::advanced::SafeReqwestBuilder {
        self.server.configure_application(builder)
    }
}

impl ResponseFixture {
    fn into_reply(self) -> ScriptedReply {
        match self {
            Self::Json { status, body } => ScriptedReply::status(status)
                .with_header(
                    http::header::CONTENT_TYPE,
                    HeaderValue::from_static("application/json"),
                )
                .with_body(body),
            Self::Text {
                status,
                body,
                content_type,
            } => ScriptedReply::status(status)
                .with_header(
                    http::header::CONTENT_TYPE,
                    HeaderValue::from_static(content_type),
                )
                .with_body(body),
        }
    }
}

#[tokio::test]
async fn generated_request_codec_can_omit_content_type() {
    let transport = RecordingTransport::new_expecting_body(
        ResponseFixture::Json {
            status: StatusCode::OK,
            body: Bytes::from_static(br#"{"ok":true}"#),
        },
        Bytes::from_static(b"hello"),
    );
    let api =
        CustomCodecHelperApi::new_with_safe_reqwest_builder(|builder| transport.configure(builder))
            .expect("deterministic generated client");

    let response = api
        .request_omit("hello".to_string())
        .execute()
        .await
        .expect("request omit succeeds");

    assert!(response.ok);
    let request = &transport.requests()[0];
    assert_eq!(request.content_type, None);
    assert_eq!(request.accept, Some("application/json".to_string()));
    assert_eq!(
        request.body_category,
        concord_core::__development::CapturedBodyCategory::Buffered
    );
    assert_eq!(request.known_body_length, Some(5));
    assert!(!format!("{request:?}").contains("hello"));
}

#[tokio::test]
async fn generated_response_codec_can_omit_accept() {
    let transport = RecordingTransport::new(ResponseFixture::Text {
        status: StatusCode::OK,
        body: Bytes::from_static(b"hello"),
        content_type: "application/x-response-omit",
    });
    let api =
        CustomCodecHelperApi::new_with_safe_reqwest_builder(|builder| transport.configure(builder))
            .expect("deterministic generated client");

    let response = api
        .response_omit()
        .execute()
        .await
        .expect("response omit succeeds");

    assert_eq!(response, "hello");
    let request = &transport.requests()[0];
    assert_eq!(request.content_type, None);
    assert_eq!(request.accept.as_deref(), Some("*/*"));
    assert_eq!(
        request.body_category,
        concord_core::__development::CapturedBodyCategory::Empty
    );
    assert!(!format!("{request:?}").contains("hello"));
}

#[tokio::test]
async fn invalid_custom_request_content_type_returns_typed_error() {
    let transport = RecordingTransport::empty();
    let api =
        CustomCodecHelperApi::new_with_safe_reqwest_builder(|builder| transport.configure(builder))
            .expect("deterministic generated client");

    let err = api
        .invalid_request("boom".to_string())
        .execute()
        .await
        .expect_err("invalid request content type fails");

    assert!(matches!(err, ApiClientError::InvalidParam { .. }));
    assert_eq!(transport.send_count(), 0);
    assert!(transport.requests().is_empty());
    assert!(!format!("{err:?}").contains("bad"));
}

#[tokio::test]
async fn invalid_custom_response_content_type_returns_typed_error() {
    let transport = RecordingTransport::empty();
    let api =
        CustomCodecHelperApi::new_with_safe_reqwest_builder(|builder| transport.configure(builder))
            .expect("deterministic generated client");

    let err = api
        .invalid_response()
        .execute()
        .await
        .expect_err("invalid response content type fails");

    assert!(matches!(err, ApiClientError::InvalidParam { .. }));
    assert_eq!(transport.send_count(), 0);
    assert!(transport.requests().is_empty());
    assert!(!format!("{err:?}").contains("bad"));
}

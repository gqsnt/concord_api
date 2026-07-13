use bytes::Bytes;
use concord_core::advanced::{
    BodyCodec, CodecError, ContentType, DecodeContext, DynBody, EncodeContext, EncodedBody,
    ResponseCodec, Transport, TransportError,
};
use concord_core::prelude::{ApiClientError, Json};
use concord_macros::api;
use http::{HeaderMap, HeaderValue, StatusCode};
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
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
    body: Bytes,
}

impl std::fmt::Debug for CapturedRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CapturedRequest")
            .field("debug", &self.debug)
            .field("content_type", &self.content_type)
            .field("accept", &self.accept)
            .field("body_len", &self.body.len())
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
    requests: Arc<StdMutex<Vec<CapturedRequest>>>,
    response: ResponseFixture,
    send_count: Arc<AtomicUsize>,
}

impl RecordingTransport {
    fn new(response: ResponseFixture) -> Self {
        Self {
            requests: Arc::new(StdMutex::new(Vec::new())),
            response,
            send_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn requests(&self) -> Vec<CapturedRequest> {
        self.requests.lock().expect("requests lock").clone()
    }

    fn send_count(&self) -> usize {
        self.send_count.load(Ordering::SeqCst)
    }
}

impl Transport for RecordingTransport {
    fn send(
        &self,
        req: http::Request<DynBody>,
    ) -> Pin<Box<dyn Future<Output = Result<http::Response<DynBody>, TransportError>> + Send>> {
        let transport = self.clone();
        Box::pin(async move {
            transport.send_count.fetch_add(1, Ordering::SeqCst);
            let debug = "Request { body: <body>, .. }".to_string();
            let content_type = req
                .headers()
                .get(http::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string);
            let accept = req
                .headers()
                .get(http::header::ACCEPT)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string);
            let body = http_body_util::BodyExt::collect(req.into_body())
                .await
                .map_err(TransportError::new)?
                .to_bytes();
            transport
                .requests
                .lock()
                .expect("requests lock")
                .push(CapturedRequest {
                    debug,
                    content_type,
                    accept,
                    body,
                });

            match transport.response.clone() {
                ResponseFixture::Json { status, body } => {
                    let mut headers = HeaderMap::new();
                    headers.insert(
                        http::header::CONTENT_TYPE,
                        HeaderValue::from_static("application/json"),
                    );
                    let mut response = http::Response::new(DynBody::from_bytes(body));
                    *response.status_mut() = status;
                    *response.headers_mut() = headers;
                    Ok(response)
                }
                ResponseFixture::Text {
                    status,
                    body,
                    content_type,
                } => {
                    let mut headers = HeaderMap::new();
                    headers.insert(
                        http::header::CONTENT_TYPE,
                        HeaderValue::from_static(content_type),
                    );
                    let mut response = http::Response::new(DynBody::from_bytes(body));
                    *response.status_mut() = status;
                    *response.headers_mut() = headers;
                    Ok(response)
                }
            }
        })
    }
}

#[tokio::test]
async fn generated_request_codec_can_omit_content_type() {
    let transport = RecordingTransport::new(ResponseFixture::Json {
        status: StatusCode::OK,
        body: Bytes::from_static(br#"{"ok":true}"#),
    });
    let api = CustomCodecHelperApi::new_with_transport(transport.clone());

    let response = api
        .request_omit("hello".to_string())
        .execute()
        .await
        .expect("request omit succeeds");

    assert!(response.ok);
    let request = &transport.requests()[0];
    assert_eq!(request.content_type, None);
    assert_eq!(request.accept, Some("application/json".to_string()));
    assert_eq!(request.body, Bytes::from_static(b"hello"));
    assert!(!format!("{request:?}").contains("hello"));
}

#[tokio::test]
async fn generated_response_codec_can_omit_accept() {
    let transport = RecordingTransport::new(ResponseFixture::Text {
        status: StatusCode::OK,
        body: Bytes::from_static(b"hello"),
        content_type: "application/x-response-omit",
    });
    let api = CustomCodecHelperApi::new_with_transport(transport.clone());

    let response = api
        .response_omit()
        .execute()
        .await
        .expect("response omit succeeds");

    assert_eq!(response, "hello");
    let request = &transport.requests()[0];
    assert_eq!(request.content_type, None);
    assert_eq!(request.accept.as_deref(), Some("*/*"));
    assert_eq!(request.body, Bytes::new());
    assert!(!format!("{request:?}").contains("hello"));
}

#[tokio::test]
async fn invalid_custom_request_content_type_returns_typed_error() {
    let transport = RecordingTransport::new(ResponseFixture::Json {
        status: StatusCode::OK,
        body: Bytes::from_static(br#"{"ok":true}"#),
    });
    let api = CustomCodecHelperApi::new_with_transport(transport.clone());

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
    let transport = RecordingTransport::new(ResponseFixture::Json {
        status: StatusCode::OK,
        body: Bytes::from_static(br#"{"ok":true}"#),
    });
    let api = CustomCodecHelperApi::new_with_transport(transport.clone());

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

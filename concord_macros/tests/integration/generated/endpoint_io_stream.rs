use bytes::Bytes;
use concord_core::advanced::{OctetStream, StreamBody, StreamResponse};
use concord_core::prelude::*;
use concord_macros::api;
use concord_test_support::{MockHandle, MockReply, MockServer, ReplyGate, ResponseStep, mock};
use http::{HeaderMap, HeaderValue, StatusCode};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

#[derive(Debug, Serialize, Deserialize)]
pub struct UploadResult {
    ok: bool,
}

mod stream_helper_contract {
    #![allow(unused_imports)]
    use super::*;

    api! {
        client StreamHelperApi {
            base "https://example.com"
        }

        POST Upload(body: Stream<OctetStream>)
            path ["upload"]
            -> Json<UploadResult>

        GET Download
            path ["download"]
            -> Stream<OctetStream>
    }

    pub(super) use stream_helper_api::StreamHelperApi;
}

use stream_helper_contract::StreamHelperApi;

#[derive(Clone, Debug, PartialEq, Eq)]
struct CapturedBody(Bytes);

#[derive(Clone, PartialEq, Eq)]
struct CapturedRequest {
    debug: String,
    content_type: Option<String>,
    body: CapturedBody,
}

impl std::fmt::Debug for CapturedRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let body = format!("<stream:{} bytes>", self.body.0.len());
        f.debug_struct("CapturedRequest")
            .field("debug", &self.debug)
            .field("content_type", &self.content_type)
            .field("body", &body)
            .finish()
    }
}

#[derive(Clone)]
struct RecordingTransport {
    server: MockServer,
    handle: Arc<StdMutex<MockHandle>>,
}

#[derive(Clone)]
enum ResponseFixture {
    Buffered {
        status: StatusCode,
        headers: HeaderMap,
        body: Bytes,
        content_length: Option<u64>,
    },
    Stream {
        status: StatusCode,
        headers: HeaderMap,
        chunks: Vec<Bytes>,
        content_length: Option<u64>,
    },
}

impl ResponseFixture {
    fn buffered_json(body: &'static str) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        Self::Buffered {
            status: StatusCode::OK,
            headers,
            body: Bytes::from_static(body.as_bytes()),
            content_length: Some(body.len() as u64),
        }
    }

    fn streamed(chunks: Vec<Bytes>) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/octet-stream"),
        );
        let content_length = chunks.iter().map(|chunk| chunk.len() as u64).sum();
        Self::Stream {
            status: StatusCode::OK,
            headers,
            chunks,
            content_length: Some(content_length),
        }
    }

    fn content_length(mut self, content_length: Option<u64>) -> Self {
        match &mut self {
            ResponseFixture::Buffered {
                content_length: len,
                ..
            } => *len = content_length,
            ResponseFixture::Stream {
                content_length: len,
                ..
            } => *len = content_length,
        }
        self
    }
}

impl RecordingTransport {
    fn buffered_response(body: &'static str) -> Self {
        Self::new(ResponseFixture::buffered_json(body))
    }

    fn streamed_response(chunks: Vec<Bytes>) -> Self {
        Self::new(ResponseFixture::streamed(chunks))
    }

    fn new(response: ResponseFixture) -> Self {
        Self::from_replies([response.into_reply()])
    }

    fn empty() -> Self {
        Self::from_replies([])
    }

    fn from_replies(replies: impl IntoIterator<Item = MockReply>) -> Self {
        let (server, handle) = mock().replies(replies).build();
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
                body: CapturedBody(request.body),
            })
            .collect()
    }

    fn send_count(&self) -> usize {
        self.handle.lock().expect("handle lock").recorded_len()
    }

    fn configure_reqwest(
        &self,
        builder: concord_core::advanced::SafeReqwestBuilder,
    ) -> concord_core::advanced::SafeReqwestBuilder {
        self.server.configure_reqwest(builder)
    }
}

impl ResponseFixture {
    fn into_reply(self) -> MockReply {
        let (status, headers, body, chunks, content_length) = match self {
            Self::Buffered {
                status,
                headers,
                body,
                content_length,
            } => (status, headers, Some(body), None, content_length),
            Self::Stream {
                status,
                headers,
                chunks,
                content_length,
            } => (status, headers, None, Some(chunks), content_length),
        };
        let mut reply = MockReply::status(status);
        for (name, value) in headers {
            if let Some(name) = name {
                reply = reply.with_header(name, value);
            }
        }
        if let Some(length) = content_length {
            reply = reply.with_header(
                http::header::CONTENT_LENGTH,
                HeaderValue::from_str(&length.to_string()).expect("length"),
            );
        }
        match (body, chunks, content_length) {
            (Some(body), _, _) => reply.with_body(body),
            (_, Some(chunks), Some(_)) => reply.with_body(Bytes::from(chunks.concat())),
            (_, Some(chunks), None) => reply.with_chunks(chunks),
            _ => reply,
        }
    }
}

#[tokio::test]
async fn generated_stream_request_reaches_transport() {
    const SENTINEL: &[u8] = b"SECRET_STREAM_REQUEST_SENTINEL_MUST_NOT_APPEAR";
    let transport = RecordingTransport::buffered_response(r#"{"ok":true}"#);
    let api = StreamHelperApi::new_with_safe_reqwest_builder(|builder| {
        transport.configure_reqwest(builder)
    })
    .expect("mock client");

    let response = api
        .upload(StreamBody::from_bytes(Bytes::from_static(SENTINEL)))
        .execute()
        .await
        .expect("stream upload succeeds");
    assert!(response.ok);

    assert_eq!(transport.send_count(), 1);
    let requests = transport.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].content_type.as_deref(),
        Some("application/octet-stream")
    );
    assert_eq!(requests[0].body.0.as_ref(), SENTINEL);
    assert!(
        !requests[0]
            .debug
            .contains("SECRET_STREAM_REQUEST_SENTINEL_MUST_NOT_APPEAR")
    );
    assert!(
        !format!("{:?}", requests[0]).contains("SECRET_STREAM_REQUEST_SENTINEL_MUST_NOT_APPEAR")
    );
}

#[tokio::test]
async fn generated_stream_response_returns_stream_without_buffering() {
    const SENTINEL: &[u8] = b"SECRET_STREAM_RESPONSE_SENTINEL_MUST_NOT_APPEAR";
    let transport = RecordingTransport::streamed_response(vec![
        Bytes::from_static(b"hello"),
        Bytes::from_static(b" "),
        Bytes::from_static(SENTINEL),
    ]);
    let api = StreamHelperApi::new_with_safe_reqwest_builder(|builder| {
        transport.configure_reqwest(builder)
    })
    .expect("mock client");

    let mut response: StreamResponse<OctetStream> = api
        .download()
        .execute()
        .await
        .expect("stream download succeeds");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.media_type(), "application/octet-stream");
    assert_eq!(
        response
            .headers()
            .get(http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/octet-stream")
    );
    assert_eq!(
        response.content_length(),
        Some((5 + 1 + SENTINEL.len()) as u64)
    );

    let response_debug = format!("{:?}", response);
    assert!(!response_debug.contains("SECRET_STREAM_RESPONSE_SENTINEL_MUST_NOT_APPEAR"));

    let mut received = Vec::new();
    while let Some(chunk) = response.next_chunk().await.expect("stream chunk") {
        received.extend_from_slice(&chunk);
    }

    assert_eq!(received, [b"hello".as_slice(), b" ", SENTINEL].concat());
}

#[tokio::test]
async fn generated_stream_response_execute_stream_returns_stream_without_buffering() {
    const SENTINEL: &[u8] = b"SECRET_STREAM_RESPONSE_SENTINEL_MUST_NOT_APPEAR";
    let transport = RecordingTransport::streamed_response(vec![
        Bytes::from_static(b"hello"),
        Bytes::from_static(b" "),
        Bytes::from_static(SENTINEL),
    ]);
    let api = StreamHelperApi::new_with_safe_reqwest_builder(|builder| {
        transport.configure_reqwest(builder)
    })
    .expect("mock client");

    let mut response: StreamResponse<OctetStream> = api
        .download()
        .execute_stream()
        .await
        .expect("stream download succeeds");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.media_type(), "application/octet-stream");
    assert_eq!(
        response
            .headers()
            .get(http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/octet-stream")
    );
    assert_eq!(
        response.content_length(),
        Some((5 + 1 + SENTINEL.len()) as u64)
    );

    let response_debug = format!("{:?}", response);
    assert!(!response_debug.contains("SECRET_STREAM_RESPONSE_SENTINEL_MUST_NOT_APPEAR"));

    let mut received = Vec::new();
    while let Some(chunk) = response.next_chunk().await.expect("stream chunk") {
        received.extend_from_slice(&chunk);
    }

    assert_eq!(received, [b"hello".as_slice(), b" ", SENTINEL].concat());
}

#[tokio::test]
async fn generated_stream_response_delivers_first_chunk_before_gated_later_chunk() {
    let gate = ReplyGate::new();
    let reply = MockReply::status(StatusCode::OK)
        .with_header(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/octet-stream"),
        )
        .with_response_steps([
            ResponseStep::Chunk(Bytes::from_static(b"first")),
            ResponseStep::Gate(gate.clone()),
            ResponseStep::Chunk(Bytes::from_static(b"second")),
        ]);
    let transport = RecordingTransport::from_replies([reply]);
    let api = StreamHelperApi::new_with_safe_reqwest_builder(|builder| {
        transport.configure_reqwest(builder)
    })
    .expect("mock client");

    let mut response = tokio::time::timeout(Duration::from_millis(250), async {
        api.download().execute_stream().await
    })
    .await
    .expect("stream execution must return before the gated tail")
    .expect("stream response");
    assert_eq!(
        response.next_chunk().await.expect("first chunk").as_deref(),
        Some(b"first".as_slice())
    );
    gate.wait_until_entered(Duration::from_secs(1));
    assert!(
        tokio::time::timeout(Duration::from_millis(25), response.next_chunk())
            .await
            .is_err(),
        "later response chunk must remain gated"
    );
    gate.release();
    assert_eq!(
        response
            .next_chunk()
            .await
            .expect("second chunk")
            .as_deref(),
        Some(b"second".as_slice())
    );
    assert!(response.next_chunk().await.expect("stream eof").is_none());
}

#[tokio::test]
async fn generated_stream_response_limit_is_enforced_after_gated_chunk_release() {
    let gate = ReplyGate::new();
    let reply = MockReply::status(StatusCode::OK)
        .with_header(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/octet-stream"),
        )
        .with_response_steps([
            ResponseStep::Chunk(Bytes::from_static(b"abcd")),
            ResponseStep::Gate(gate.clone()),
            ResponseStep::Chunk(Bytes::from_static(b"efgh")),
        ]);
    let transport = RecordingTransport::from_replies([reply]);
    let api = StreamHelperApi::new_with_safe_reqwest_builder(|builder| {
        transport.configure_reqwest(builder)
    })
    .expect("mock client")
    .configure(|config| {
        config.max_stream_response_body_bytes(5);
    });
    let mut response = api
        .download()
        .execute_stream()
        .await
        .expect("stream response");

    assert_eq!(
        response.next_chunk().await.expect("first chunk").as_deref(),
        Some(b"abcd".as_slice())
    );
    gate.wait_until_entered(Duration::from_secs(1));
    assert!(
        tokio::time::timeout(Duration::from_millis(25), response.next_chunk())
            .await
            .is_err()
    );
    gate.release();
    let error = response
        .next_chunk()
        .await
        .expect_err("released second chunk must exceed the client-side limit");
    assert!(matches!(
        error,
        ApiClientError::ResponseBodyLimitExceeded { limit: 5, .. }
    ));
}

#[tokio::test]
async fn generated_stream_response_drop_cancels_gated_tail_without_consuming_it() {
    let gate = ReplyGate::new();
    let reply = MockReply::status(StatusCode::OK)
        .with_header(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/octet-stream"),
        )
        .with_response_steps([
            ResponseStep::Chunk(Bytes::from_static(b"first")),
            ResponseStep::Gate(gate.clone()),
            ResponseStep::Chunk(Bytes::from_static(b"unconsumed-secret")),
        ]);
    let transport = RecordingTransport::from_replies([reply]);
    let api = StreamHelperApi::new_with_safe_reqwest_builder(|builder| {
        transport.configure_reqwest(builder)
    })
    .expect("mock client");
    let mut response = api
        .download()
        .execute_stream()
        .await
        .expect("stream response");
    assert_eq!(
        response.next_chunk().await.expect("first chunk").as_deref(),
        Some(b"first".as_slice())
    );
    gate.wait_until_entered(Duration::from_secs(1));
    drop(response);
    drop(api);
    drop(transport);
}

#[tokio::test]
async fn generated_stream_response_disconnect_is_safely_categorized_and_redacted() {
    const SENTINEL: &str = "GATED_STREAM_BODY_FAILURE_SECRET";
    let reply = MockReply::status(StatusCode::OK)
        .with_header(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/octet-stream"),
        )
        .with_response_steps([
            ResponseStep::Chunk(Bytes::from_static(b"first")),
            ResponseStep::Disconnect,
        ]);
    let transport = RecordingTransport::from_replies([reply]);
    let api = StreamHelperApi::new_with_safe_reqwest_builder(|builder| {
        transport.configure_reqwest(builder)
    })
    .expect("mock client");
    let mut response = api
        .download()
        .execute_stream()
        .await
        .expect("stream response");
    assert_eq!(
        response.next_chunk().await.expect("first chunk").as_deref(),
        Some(b"first".as_slice())
    );
    let error = response
        .next_chunk()
        .await
        .expect_err("disconnect must fail the client-side stream");
    assert_eq!(error.category(), concord_core::error::ErrorCategory::Decode);
    assert!(!error.to_string().contains(SENTINEL));
    assert!(!format!("{error:?}").contains(SENTINEL));
}

#[tokio::test]
async fn generated_stream_request_enforces_configured_request_limit() {
    const SENTINEL: &[u8] = b"SECRET_STREAM_REQUEST_SENTINEL_MUST_NOT_APPEAR";
    let transport = RecordingTransport::empty();
    let api = StreamHelperApi::new_with_safe_reqwest_builder(|builder| {
        transport.configure_reqwest(builder)
    })
    .expect("mock client");
    let api = api.configure(|cfg| {
        cfg.max_stream_request_body_bytes(4);
    });

    let err = api
        .upload(StreamBody::from_bytes(Bytes::from_static(SENTINEL)))
        .execute()
        .await
        .expect_err("stream upload should fail when limit is exceeded");

    assert!(matches!(
        err,
        ApiClientError::RequestBodyLimitExceeded { limit: 4, .. }
    ));
    assert!(
        err.to_string()
            .contains("stream request body exceeded configured size limit")
    );
    assert_eq!(transport.send_count(), 0);
    assert!(transport.requests().is_empty());
}

#[tokio::test]
async fn generated_stream_response_enforces_configured_response_limit() {
    let transport = RecordingTransport::new(
        ResponseFixture::streamed(vec![
            Bytes::from_static(b"abcd"),
            Bytes::from_static(b"efgh"),
        ])
        .content_length(None),
    );
    let api = StreamHelperApi::new_with_safe_reqwest_builder(|builder| {
        transport.configure_reqwest(builder)
    })
    .expect("mock client");
    let api = api.configure(|cfg| {
        cfg.max_stream_response_body_bytes(5);
    });

    let mut response: StreamResponse<OctetStream> = api
        .download()
        .execute_stream()
        .await
        .expect("stream response should be returned before limit is hit");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(transport.send_count(), 1);
    assert_eq!(
        response.next_chunk().await.unwrap().as_deref(),
        Some(b"abcd".as_slice())
    );
    let err = response
        .next_chunk()
        .await
        .expect_err("second chunk should exceed configured limit");

    assert!(matches!(
        err,
        ApiClientError::ResponseBodyLimitExceeded { .. }
    ));
    assert!(!format!("{err:?}").contains("SECRET_STREAM_RESPONSE_SENTINEL_MUST_NOT_APPEAR"));
}

#[tokio::test]
async fn generated_stream_response_exact_limit_uses_only_stream_policy() {
    let transport = RecordingTransport::new(
        ResponseFixture::streamed(vec![Bytes::from_static(b"ab"), Bytes::from_static(b"cde")])
            .content_length(None),
    );
    let api = StreamHelperApi::new_with_safe_reqwest_builder(|builder| {
        transport.configure_reqwest(builder)
    })
    .expect("mock client")
    .configure(|config| {
        config.max_response_body_bytes(1);
        config.max_stream_response_body_bytes(5);
    });

    let mut response = api
        .download()
        .execute_stream()
        .await
        .expect("stream response");
    let mut body = Vec::new();
    while let Some(chunk) = response.next_chunk().await.expect("bounded chunk") {
        body.extend_from_slice(&chunk);
    }
    assert_eq!(body, b"abcde");
}

#[tokio::test]
async fn generated_stream_response_known_oversize_fails_before_consumption() {
    let reply = MockReply::status(StatusCode::OK)
        .with_header(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/octet-stream"),
        )
        .with_body(Bytes::from_static(b"oversize"));
    let transport = RecordingTransport::from_replies([reply]);
    let api = StreamHelperApi::new_with_safe_reqwest_builder(|builder| {
        transport.configure_reqwest(builder)
    })
    .expect("mock client")
    .configure(|config| {
        config.max_stream_response_body_bytes(5);
    });

    let error = api
        .download()
        .execute_stream()
        .await
        .expect_err("known oversized native response must fail at the head");
    assert!(matches!(
        error,
        ApiClientError::ResponseTooLarge {
            limit: 5,
            actual: 8,
            ..
        }
    ));
    assert!(!format!("{error:?}").contains("oversize"));
}

#[tokio::test]
async fn generated_stream_response_disabled_limit_reads_all_chunks() {
    let transport = RecordingTransport::new(
        ResponseFixture::streamed(vec![
            Bytes::from_static(b"first"),
            Bytes::from_static(b"second"),
        ])
        .content_length(None),
    );
    let api = StreamHelperApi::new_with_safe_reqwest_builder(|builder| {
        transport.configure_reqwest(builder)
    })
    .expect("mock client")
    .configure(|config| {
        config.no_stream_response_body_limit();
    });

    let mut response = api
        .download()
        .execute_stream()
        .await
        .expect("stream response");
    let mut body = Vec::new();
    while let Some(chunk) = response.next_chunk().await.expect("unbounded chunk") {
        body.extend_from_slice(&chunk);
    }
    assert_eq!(body, b"firstsecond");
}

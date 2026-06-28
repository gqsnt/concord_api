use bytes::Bytes;
use concord_core::advanced::{
    OctetStream, StreamBody, StreamResponse, Transport, TransportBody, TransportError,
    TransportRequest, TransportRequestBody, TransportResponse,
};
use concord_core::prelude::*;
use concord_macros::api;
use futures_core::Stream;
use http::{HeaderMap, HeaderValue, StatusCode};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

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
enum CapturedBody {
    Empty,
    Bytes(Bytes),
    Stream(Bytes),
}

#[derive(Clone, PartialEq, Eq)]
struct CapturedRequest {
    debug: String,
    content_type: Option<String>,
    body: CapturedBody,
}

impl std::fmt::Debug for CapturedRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let body = match &self.body {
            CapturedBody::Empty => "<empty>".to_string(),
            CapturedBody::Bytes(bytes) => format!("<{} bytes>", bytes.len()),
            CapturedBody::Stream(bytes) => format!("<stream:{} bytes>", bytes.len()),
        };
        f.debug_struct("CapturedRequest")
            .field("debug", &self.debug)
            .field("content_type", &self.content_type)
            .field("body", &body)
            .finish()
    }
}

#[derive(Clone)]
struct RecordingTransport {
    events: Arc<StdMutex<Vec<String>>>,
    requests: Arc<StdMutex<Vec<CapturedRequest>>>,
    response: ResponseFixture,
    send_count: Arc<AtomicUsize>,
}

#[derive(Clone)]
enum ResponseFixture {
    Buffered {
        status: StatusCode,
        headers: HeaderMap,
        body: Bytes,
    },
    Stream {
        status: StatusCode,
        headers: HeaderMap,
        chunks: Vec<Bytes>,
        poll_flag: Arc<AtomicBool>,
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
        }
    }

    fn streamed(chunks: Vec<Bytes>, poll_flag: Arc<AtomicBool>) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/octet-stream"),
        );
        Self::Stream {
            status: StatusCode::OK,
            headers,
            chunks,
            poll_flag,
        }
    }
}

impl RecordingTransport {
    fn buffered_response(body: &'static str) -> Self {
        Self::new(ResponseFixture::buffered_json(body))
    }

    fn streamed_response(chunks: Vec<Bytes>, poll_flag: Arc<AtomicBool>) -> Self {
        Self::new(ResponseFixture::streamed(chunks, poll_flag))
    }

    fn new(response: ResponseFixture) -> Self {
        Self {
            events: Arc::new(StdMutex::new(Vec::new())),
            requests: Arc::new(StdMutex::new(Vec::new())),
            response,
            send_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn events(&self) -> Vec<String> {
        self.events.lock().expect("events lock").clone()
    }

    fn requests(&self) -> Vec<CapturedRequest> {
        self.requests.lock().expect("requests lock").clone()
    }

    fn send_count(&self) -> usize {
        self.send_count.load(Ordering::SeqCst)
    }

    fn push_event(&self, event: impl Into<String>) {
        self.events.lock().expect("events lock").push(event.into());
    }
}

impl Transport for RecordingTransport {
    fn send(
        &self,
        req: TransportRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send>> {
        let transport = self.clone();
        Box::pin(async move {
            transport.send_count.fetch_add(1, Ordering::SeqCst);
            transport.push_event("transport_send");
            let debug = format!("{req:?}");
            let content_type = req
                .headers
                .get(http::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string);
            let body = match req.body {
                TransportRequestBody::Empty => CapturedBody::Empty,
                TransportRequestBody::Bytes(body) => CapturedBody::Bytes(body),
                TransportRequestBody::Stream(stream) => CapturedBody::Stream(
                    collect_stream(stream, &transport.events, "request_stream_poll").await?,
                ),
            };
            transport
                .requests
                .lock()
                .expect("requests lock")
                .push(CapturedRequest {
                    debug,
                    content_type,
                    body,
                });

            match transport.response.clone() {
                ResponseFixture::Buffered {
                    status,
                    headers,
                    body,
                } => Ok(TransportResponse {
                    meta: req.meta,
                    url: req.url,
                    status,
                    headers,
                    content_length: Some(body.len() as u64),
                    rate_limit: req.rate_limit,
                    body: Box::new(StaticBody(Some(body))),
                }),
                ResponseFixture::Stream {
                    status,
                    headers,
                    chunks,
                    poll_flag,
                } => Ok(TransportResponse {
                    meta: req.meta,
                    url: req.url,
                    status,
                    headers,
                    content_length: chunks.iter().fold(Some(0u64), |acc, chunk| {
                        acc.and_then(|len| len.checked_add(chunk.len() as u64))
                    }),
                    rate_limit: req.rate_limit,
                    body: Box::new(ChunkBody::new(transport.events.clone(), chunks, poll_flag)),
                }),
            }
        })
    }
}

struct StaticBody(Option<Bytes>);

impl TransportBody for StaticBody {
    fn next_chunk<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Bytes>, TransportError>> + Send + 'a>> {
        Box::pin(async move { Ok(self.0.take()) })
    }
}

struct ChunkBody {
    events: Arc<StdMutex<Vec<String>>>,
    chunks: VecDeque<Bytes>,
    poll_flag: Arc<AtomicBool>,
}

impl ChunkBody {
    fn new(
        events: Arc<StdMutex<Vec<String>>>,
        chunks: Vec<Bytes>,
        poll_flag: Arc<AtomicBool>,
    ) -> Self {
        Self {
            events,
            chunks: chunks.into(),
            poll_flag,
        }
    }
}

impl TransportBody for ChunkBody {
    fn next_chunk<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Bytes>, TransportError>> + Send + 'a>> {
        let events = self.events.clone();
        let poll_flag = self.poll_flag.clone();
        let chunk = self.chunks.pop_front();
        Box::pin(async move {
            if chunk.is_some() {
                poll_flag.store(true, Ordering::SeqCst);
                events
                    .lock()
                    .expect("response stream events lock")
                    .push("response_stream_poll".to_string());
            }
            Ok(chunk)
        })
    }
}

async fn collect_stream(
    mut stream: concord_core::advanced::TransportByteStream,
    events: &Arc<StdMutex<Vec<String>>>,
    event: &'static str,
) -> Result<Bytes, TransportError> {
    let mut out = Vec::new();
    loop {
        let next = std::future::poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)).await;
        match next {
            Some(Ok(chunk)) => {
                events
                    .lock()
                    .expect("request stream events lock")
                    .push(event.to_string());
                out.extend_from_slice(&chunk);
            }
            Some(Err(error)) => return Err(error),
            None => break,
        }
    }
    Ok(Bytes::from(out))
}

#[tokio::test]
async fn generated_stream_request_reaches_transport() {
    const SENTINEL: &[u8] = b"SECRET_STREAM_REQUEST_SENTINEL_MUST_NOT_APPEAR";
    let transport = RecordingTransport::buffered_response(r#"{"ok":true}"#);
    let api = StreamHelperApi::new_with_transport(transport.clone());

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
    match &requests[0].body {
        CapturedBody::Stream(bytes) => assert_eq!(bytes.as_ref(), SENTINEL),
        other => panic!("expected stream body, got {other:?}"),
    }
    assert!(
        !requests[0]
            .debug
            .contains("SECRET_STREAM_REQUEST_SENTINEL_MUST_NOT_APPEAR")
    );
    assert!(
        !format!("{:?}", requests[0]).contains("SECRET_STREAM_REQUEST_SENTINEL_MUST_NOT_APPEAR")
    );
    let events = transport.events();
    let transport_idx = events
        .iter()
        .position(|event| event == "transport_send")
        .expect("transport send event");
    let stream_idx = events
        .iter()
        .position(|event| event == "request_stream_poll")
        .expect("request stream poll event");
    assert!(transport_idx < stream_idx);
}

#[tokio::test]
async fn generated_stream_response_returns_stream_without_buffering() {
    const SENTINEL: &[u8] = b"SECRET_STREAM_RESPONSE_SENTINEL_MUST_NOT_APPEAR";
    let poll_flag = Arc::new(AtomicBool::new(false));
    let transport = RecordingTransport::streamed_response(
        vec![
            Bytes::from_static(b"hello"),
            Bytes::from_static(b" "),
            Bytes::from_static(SENTINEL),
        ],
        poll_flag.clone(),
    );
    let api = StreamHelperApi::new_with_transport(transport.clone());

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
    assert!(!poll_flag.load(Ordering::SeqCst));
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

    assert!(poll_flag.load(Ordering::SeqCst));
    assert_eq!(received, [b"hello".as_slice(), b" ", SENTINEL].concat());
    let events = transport.events();
    let transport_idx = events
        .iter()
        .position(|event| event == "transport_send")
        .expect("transport send event");
    let stream_idx = events
        .iter()
        .position(|event| event == "response_stream_poll")
        .expect("response stream poll event");
    assert!(transport_idx < stream_idx);
    assert_eq!(
        events
            .iter()
            .filter(|event| event.as_str() == "response_stream_poll")
            .count(),
        3
    );
}

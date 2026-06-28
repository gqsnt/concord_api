use bytes::Bytes;
use concord_core::advanced::{
    JsonSse, SseStream, Transport, TransportBody, TransportError, TransportRequest,
    TransportRequestBody, TransportResponse,
};
use concord_core::prelude::{ApiClientError, Json};
use concord_macros::api;
use futures_core::Stream;
use http::{HeaderMap, HeaderValue, StatusCode};
use serde::Deserialize;
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct LogEvent {
    id: u64,
    msg: String,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct UploadResult {
    ok: bool,
}

const SSE_SENTINEL: &str = "SECRET_SSE_SENTINEL_MUST_NOT_APPEAR";

mod sse_helper_contract {
    #![allow(unused_imports)]
    use super::*;

    api! {
        client SseHelperApi {
            base "https://example.com"
        }

        GET EventsDefault
            path ["events-default"]
            -> Sse<LogEvent>

        GET EventsExplicit
            path ["events-explicit"]
            -> Sse<LogEvent, JsonSse>

        GET EventsLimit
            path ["events-limit"]
            -> Sse<LogEvent>

        GET Buffered
            path ["buffered"]
            -> Json<UploadResult>
    }

    pub(super) use sse_helper_api::SseHelperApi;
}

use sse_helper_contract::SseHelperApi;

#[derive(Clone, Debug, PartialEq, Eq)]
enum CapturedBody {
    Empty,
    Bytes(Bytes),
    Stream(Bytes),
}

#[derive(Clone, PartialEq, Eq)]
struct CapturedRequest {
    debug: String,
    accept: Option<String>,
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
            .field("accept", &self.accept)
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
    Sse {
        status: StatusCode,
        headers: HeaderMap,
        chunks: Vec<Bytes>,
        content_length: Option<u64>,
        poll_flag: Arc<AtomicBool>,
    },
    BufferedJson {
        status: StatusCode,
        headers: HeaderMap,
        body: Bytes,
        content_length: Option<u64>,
    },
}

impl ResponseFixture {
    fn sse(
        content_type: &'static str,
        chunks: Vec<Bytes>,
        content_length: Option<u64>,
        poll_flag: Arc<AtomicBool>,
    ) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static(content_type),
        );
        Self::Sse {
            status: StatusCode::OK,
            headers,
            chunks,
            content_length,
            poll_flag,
        }
    }

    fn buffered_json(body: &'static str) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        Self::BufferedJson {
            status: StatusCode::OK,
            headers,
            body: Bytes::from_static(body.as_bytes()),
            content_length: Some(body.len() as u64),
        }
    }
}

impl RecordingTransport {
    fn new(response: ResponseFixture) -> Self {
        Self {
            events: Arc::new(StdMutex::new(Vec::new())),
            requests: Arc::new(StdMutex::new(Vec::new())),
            response,
            send_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn requests(&self) -> Vec<CapturedRequest> {
        self.requests.lock().expect("requests lock").clone()
    }

    fn events(&self) -> Vec<String> {
        self.events.lock().expect("events lock").clone()
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
            let accept = req
                .headers
                .get(http::header::ACCEPT)
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
                    accept,
                    body,
                });

            match transport.response.clone() {
                ResponseFixture::BufferedJson {
                    status,
                    headers,
                    body,
                    content_length,
                } => Ok(TransportResponse {
                    meta: req.meta,
                    url: req.url,
                    status,
                    headers,
                    content_length,
                    rate_limit: req.rate_limit,
                    body: Box::new(StaticBody(Some(body))),
                }),
                ResponseFixture::Sse {
                    status,
                    headers,
                    chunks,
                    content_length,
                    poll_flag,
                } => Ok(TransportResponse {
                    meta: req.meta,
                    url: req.url,
                    status,
                    headers,
                    content_length,
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

fn sse_chunks() -> Vec<Bytes> {
    vec![
        Bytes::from_static(br#"data: {"id":1,"msg":"hello"}"#),
        Bytes::from_static(b"\n\n"),
        Bytes::from_static(br#"data: {"id":2,"msg":"world"}"#),
        Bytes::from_static(b"\n\n"),
    ]
}

#[tokio::test]
async fn generated_sse_response_execute_sse_returns_stream_without_buffering() {
    const SENTINEL: &str = SSE_SENTINEL;
    let poll_flag = Arc::new(AtomicBool::new(false));
    let total_len = sse_chunks()
        .iter()
        .map(|chunk| chunk.len() as u64)
        .sum::<u64>();
    let transport = RecordingTransport::new(ResponseFixture::sse(
        "text/event-stream",
        sse_chunks(),
        Some(total_len),
        poll_flag.clone(),
    ));
    let api = SseHelperApi::new_with_transport(transport.clone());

    let mut stream: SseStream<LogEvent> = api
        .events_default()
        .execute_sse()
        .await
        .expect("execute_sse succeeds");
    assert_eq!(transport.send_count(), 1);
    assert!(!poll_flag.load(Ordering::SeqCst));
    assert_eq!(
        transport.requests()[0].accept.as_deref(),
        Some("text/event-stream")
    );
    assert!(!format!("{stream:?}").contains(SENTINEL));

    let first = stream.next_event().await.expect("first event decode");
    assert_eq!(
        first,
        Some(concord_core::advanced::SseEvent {
            event: None,
            id: None,
            retry: None,
            data: LogEvent {
                id: 1,
                msg: "hello".into(),
            },
        })
    );
    let second = stream.next_event().await.expect("second event decode");
    assert_eq!(
        second,
        Some(concord_core::advanced::SseEvent {
            event: None,
            id: None,
            retry: None,
            data: LogEvent {
                id: 2,
                msg: "world".into(),
            },
        })
    );
    let end = stream.next_event().await.expect("end of stream");
    assert_eq!(end, None);
    assert!(!format!("{stream:?}").contains(SENTINEL));
}

#[tokio::test]
async fn generated_sse_response_execute_also_returns_stream_without_buffering() {
    let poll_flag = Arc::new(AtomicBool::new(false));
    let transport = RecordingTransport::new(ResponseFixture::sse(
        "text/event-stream",
        sse_chunks(),
        None,
        poll_flag.clone(),
    ));
    let api = SseHelperApi::new_with_transport(transport.clone());

    let mut stream: SseStream<LogEvent> = api
        .events_explicit()
        .execute()
        .await
        .expect("execute succeeds");
    assert_eq!(transport.send_count(), 1);
    assert!(!poll_flag.load(Ordering::SeqCst));
    let first = stream.next_event().await.expect("first event decode");
    assert_eq!(first.unwrap().data.id, 1);
    let second = stream.next_event().await.expect("second event decode");
    assert_eq!(second.unwrap().data.msg, "world");
    assert_eq!(stream.next_event().await.expect("end of stream"), None);
}

#[tokio::test]
async fn generated_sse_response_wrong_content_type_is_rejected_before_body_exposure() {
    let poll_flag = Arc::new(AtomicBool::new(false));
    let transport = RecordingTransport::new(ResponseFixture::sse(
        "application/json",
        sse_chunks(),
        None,
        poll_flag.clone(),
    ));
    let api = SseHelperApi::new_with_transport(transport.clone());

    let err = api
        .events_default()
        .execute_sse()
        .await
        .expect_err("wrong content type must fail");
    assert!(matches!(err, ApiClientError::PolicyViolation { .. }));
    assert_eq!(transport.send_count(), 1);
    assert!(!poll_flag.load(Ordering::SeqCst));
}

#[tokio::test]
async fn generated_sse_response_content_length_limit_applies_before_body_exposure() {
    let poll_flag = Arc::new(AtomicBool::new(false));
    let transport = RecordingTransport::new(ResponseFixture::sse(
        "text/event-stream",
        sse_chunks(),
        Some(64),
        poll_flag.clone(),
    ));
    let api = SseHelperApi::new_with_transport(transport.clone());
    let api = api.configure(|cfg| {
        cfg.max_stream_response_body_bytes(5);
    });

    let err = api
        .events_default()
        .execute_sse()
        .await
        .expect_err("content-length limit must fail");
    assert!(matches!(err, ApiClientError::ResponseTooLarge { .. }));
    assert_eq!(transport.send_count(), 1);
    assert!(!poll_flag.load(Ordering::SeqCst));
    assert!(!format!("{err:?}").contains(SSE_SENTINEL));
}

#[tokio::test]
async fn generated_sse_response_unknown_length_limit_applies_while_reading() {
    let poll_flag = Arc::new(AtomicBool::new(false));
    let transport = RecordingTransport::new(ResponseFixture::sse(
        "text/event-stream",
        sse_chunks(),
        None,
        poll_flag.clone(),
    ));
    let api = SseHelperApi::new_with_transport(transport.clone());
    let api = api.configure(|cfg| {
        cfg.max_stream_response_body_bytes(35);
    });

    let mut stream: SseStream<LogEvent> = api
        .events_limit()
        .execute_sse()
        .await
        .expect("execute_sse succeeds");
    assert_eq!(transport.send_count(), 1);
    assert!(!poll_flag.load(Ordering::SeqCst));
    let first = stream.next_event().await.expect("first event");
    assert!(first.is_some());
    let err = stream
        .next_event()
        .await
        .expect_err("response limit must fail while reading");
    assert!(matches!(
        err,
        ApiClientError::ResponseBodyLimitExceeded { .. }
    ));
    assert!(!format!("{err:?}").contains(SSE_SENTINEL));
}

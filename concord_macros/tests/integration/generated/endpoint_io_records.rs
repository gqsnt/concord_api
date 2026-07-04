use bytes::Bytes;
use concord_core::advanced::{
    CodecError, ContentType, NdJson, RecordBody, RecordDecoder, RecordEncoder, RecordFormat,
    RecordStream, Transport, TransportBody, TransportError, TransportRequest, TransportRequestBody,
    TransportResponse,
};
use concord_core::prelude::{ApiClientError, Json};
use concord_macros::api;
use futures_core::Stream;
use http::{HeaderMap, HeaderValue, StatusCode};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LogEntry {
    message: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct UploadResult {
    ok: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PipeEntry {
    pub id: u64,
    pub message: String,
}

const PIPE_RECORD_SENTINEL: &str = "SECRET_RECORD_SENTINEL_MUST_NOT_APPEAR";

#[derive(Debug, Default, Clone, Copy)]
pub struct PipeText;

impl ContentType for PipeText {
    const CONTENT_TYPE: &'static str = "text/x-pipe-records";
}

struct PipeTextEncoder;

impl RecordEncoder<PipeEntry> for PipeTextEncoder {
    fn encode_record(&mut self, value: PipeEntry) -> Result<Bytes, CodecError> {
        if value.message.contains('|')
            || value.message.contains('\n')
            || value.message.contains('\r')
        {
            return Err(CodecError::new(PIPE_RECORD_SENTINEL));
        }
        Ok(Bytes::from(format!("{}|{}\n", value.id, value.message)))
    }
}

#[derive(Default)]
struct PipeTextDecoder {
    buffer: Vec<u8>,
}

impl PipeTextDecoder {
    fn decode_line(&self, line: &[u8]) -> Result<PipeEntry, CodecError> {
        let text = std::str::from_utf8(line).map_err(|_| CodecError::new(PIPE_RECORD_SENTINEL))?;
        if text.is_empty() || text.contains('\r') {
            return Err(CodecError::new(PIPE_RECORD_SENTINEL));
        }
        let mut parts = text.split('|');
        let id = parts
            .next()
            .ok_or_else(|| CodecError::new(PIPE_RECORD_SENTINEL))?;
        let message = parts
            .next()
            .ok_or_else(|| CodecError::new(PIPE_RECORD_SENTINEL))?;
        if parts.next().is_some() || id.is_empty() {
            return Err(CodecError::new(PIPE_RECORD_SENTINEL));
        }
        let id = id
            .parse::<u64>()
            .map_err(|_| CodecError::new(PIPE_RECORD_SENTINEL))?;
        Ok(PipeEntry {
            id,
            message: message.to_string(),
        })
    }

    fn parse_available(&mut self, finalizing: bool) -> Result<Vec<PipeEntry>, CodecError> {
        let mut out = Vec::new();
        while let Some(pos) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let mut line: Vec<u8> = self.buffer.drain(..=pos).collect();
            line.pop();
            out.push(self.decode_line(&line)?);
        }
        if finalizing && !self.buffer.is_empty() {
            let line = std::mem::take(&mut self.buffer);
            out.push(self.decode_line(&line)?);
        }
        Ok(out)
    }
}

impl RecordDecoder<PipeEntry> for PipeTextDecoder {
    fn push_chunk(&mut self, chunk: Bytes) -> Result<Vec<PipeEntry>, CodecError> {
        self.buffer.extend_from_slice(&chunk);
        self.parse_available(false)
    }

    fn finish(&mut self) -> Result<Vec<PipeEntry>, CodecError> {
        self.parse_available(true)
    }
}

impl RecordFormat<PipeEntry> for PipeText {
    fn encoder() -> Box<dyn RecordEncoder<PipeEntry>> {
        Box::new(PipeTextEncoder)
    }

    fn decoder() -> Box<dyn RecordDecoder<PipeEntry>> {
        Box::new(PipeTextDecoder::default())
    }
}

mod record_helper_contract {
    #![allow(unused_imports)]
    use super::*;

    api! {
        client RecordHelperApi {
            base "https://example.com"
        }

        POST Upload(body: Records<LogEntry, NdJson>)
            path ["upload"]
            -> Json<UploadResult>

        POST PipeUpload(body: Records<PipeEntry, PipeText>)
            path ["pipe-upload"]
            -> Json<UploadResult>

        GET Tail
            path ["tail"]
            -> Records<LogEntry, NdJson>

        GET PipeTail
            path ["pipe-tail"]
            -> Records<PipeEntry, PipeText>

        GET TailNoDebug
            path ["tail-no-debug"]
            -> Records<NoDebug, NdJson>

        POST Mirror(body: Records<LogEntry, NdJson>)
            path ["mirror"]
            -> Records<LogEntry, NdJson>

        POST PipeMirror(body: Records<PipeEntry, PipeText>)
            path ["pipe-mirror"]
            -> Records<PipeEntry, PipeText>
    }

    pub(super) use record_helper_api::RecordHelperApi;
}

use record_helper_contract::RecordHelperApi;

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
        content_length: Option<u64>,
    },
    Stream {
        status: StatusCode,
        headers: HeaderMap,
        chunks: Vec<Bytes>,
        content_length: Option<u64>,
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
            content_length: Some(body.len() as u64),
        }
    }

    fn streamed(chunks: Vec<Bytes>, poll_flag: Arc<AtomicBool>) -> Self {
        Self::streamed_with_content_type(chunks, poll_flag, NdJson::CONTENT_TYPE)
    }

    fn streamed_with_content_type(
        chunks: Vec<Bytes>,
        poll_flag: Arc<AtomicBool>,
        content_type: &'static str,
    ) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static(content_type),
        );
        let content_length = chunks.iter().map(|chunk| chunk.len() as u64).sum();
        Self::Stream {
            status: StatusCode::OK,
            headers,
            chunks,
            content_length: Some(content_length),
            poll_flag,
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
}

impl Transport for RecordingTransport {
    fn send(
        &self,
        req: TransportRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send>> {
        let transport = self.clone();
        Box::pin(async move {
            transport.send_count.fetch_add(1, Ordering::SeqCst);
            transport
                .events
                .lock()
                .expect("events lock")
                .push("transport".to_string());
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
                ResponseFixture::Stream {
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
                    .expect("response events lock")
                    .push("response_record_poll".to_string());
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
                    .expect("request stream lock")
                    .push(event.to_string());
                out.extend_from_slice(&chunk);
            }
            Some(Err(error)) => return Err(error),
            None => break,
        }
    }
    Ok(Bytes::from(out))
}

fn ndjson_bytes(entries: &[LogEntry]) -> Bytes {
    let mut out = String::new();
    for entry in entries {
        out.push_str(&serde_json::to_string(entry).expect("json encode"));
        out.push('\n');
    }
    Bytes::from(out)
}

fn pipe_bytes(entries: &[PipeEntry]) -> Bytes {
    let mut out = String::new();
    for entry in entries {
        out.push_str(&format!("{}|{}\n", entry.id, entry.message));
    }
    Bytes::from(out)
}

#[tokio::test]
async fn generated_record_request_reaches_transport() {
    const SENTINEL: &str = "SECRET_RECORD_SENTINEL_MUST_NOT_APPEAR";
    let transport = RecordingTransport::new(ResponseFixture::buffered_json(r#"{"ok":true}"#));
    let api = RecordHelperApi::new_with_transport(transport.clone());

    let response = api
        .upload(RecordBody::<LogEntry>::from_iter(vec![LogEntry {
            message: SENTINEL.to_string(),
        }]))
        .execute()
        .await
        .expect("record upload succeeds");
    assert!(response.ok);

    assert_eq!(transport.send_count(), 1);
    let requests = transport.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].content_type.as_deref(),
        Some(NdJson::CONTENT_TYPE)
    );
    match &requests[0].body {
        CapturedBody::Stream(bytes) => {
            assert_eq!(
                bytes.as_ref(),
                ndjson_bytes(&[LogEntry {
                    message: SENTINEL.to_string()
                }])
                .as_ref()
            );
        }
        other => panic!("expected stream body, got {other:?}"),
    }
    assert!(!requests[0].debug.contains(SENTINEL));
    assert!(!format!("{:?}", requests[0]).contains(SENTINEL));
    let events = transport.events();
    let transport_idx = events
        .iter()
        .position(|event| event == "transport")
        .expect("transport event");
    let poll_idx = events
        .iter()
        .position(|event| event == "request_stream_poll")
        .expect("request stream poll event");
    assert!(transport_idx < poll_idx);
}

#[tokio::test]
async fn generated_record_response_yields_records_incrementally() {
    let poll_flag = Arc::new(AtomicBool::new(false));
    let transport = RecordingTransport::new(ResponseFixture::streamed(
        vec![
            Bytes::from_static(
                br#"{"message":"one"}
"#,
            ),
            Bytes::from_static(br#"{"message":"two"}"#),
        ],
        poll_flag.clone(),
    ));
    let api = RecordHelperApi::new_with_transport(transport.clone());

    let mut stream: RecordStream<LogEntry> = api.tail().execute_records().await.unwrap();
    assert!(!poll_flag.load(Ordering::SeqCst));
    let first = stream.next_record().await.unwrap().expect("first record");
    assert_eq!(first.message, "one");
    assert!(poll_flag.load(Ordering::SeqCst));
    let second = stream.next_record().await.unwrap().expect("second record");
    assert_eq!(second.message, "two");
    assert_eq!(stream.next_record().await.unwrap(), None);
    let debug = format!("{:?}", stream);
    assert!(!debug.contains("SECRET_RECORD_SENTINEL_MUST_NOT_APPEAR"));
}

#[derive(Serialize, Deserialize)]
pub struct NoDebug {
    message: String,
}

#[tokio::test]
async fn record_stream_debug_is_body_free_without_debug_bound() {
    let poll_flag = Arc::new(AtomicBool::new(false));
    let transport = RecordingTransport::new(ResponseFixture::streamed(
        vec![Bytes::from_static(
            br#"{"message":"SECRET_RECORD_SENTINEL_MUST_NOT_APPEAR"}"#,
        )],
        poll_flag,
    ));
    let api = RecordHelperApi::new_with_transport(transport);

    let stream: RecordStream<NoDebug> = api.tail_no_debug().execute_records().await.unwrap();
    let debug = format!("{:?}", stream);
    assert!(!debug.contains("SECRET_RECORD_SENTINEL_MUST_NOT_APPEAR"));
}

#[tokio::test]
async fn generated_record_request_stream_limit_applies() {
    let transport = RecordingTransport::new(ResponseFixture::buffered_json(r#"{"ok":true}"#));
    let api = RecordHelperApi::new_with_transport(transport.clone()).configure(|cfg| {
        cfg.max_stream_request_body_bytes(10);
    });

    let err = api
        .upload(RecordBody::<LogEntry>::from_iter(vec![
            LogEntry {
                message: "one".to_string(),
            },
            LogEntry {
                message: "two".to_string(),
            },
        ]))
        .execute()
        .await
        .expect_err("request limit should fail");
    assert!(matches!(
        err,
        ApiClientError::RequestBodyLimitExceeded { limit: 10, .. }
    ));
    assert_eq!(transport.send_count(), 1);
}

#[tokio::test]
async fn generated_record_response_stream_limit_applies() {
    let poll_flag = Arc::new(AtomicBool::new(false));
    let transport = RecordingTransport::new(
        ResponseFixture::streamed(
            vec![
                Bytes::from_static(
                    br#"{"message":"one"}
"#,
                ),
                Bytes::from_static(
                    br#"{"message":"two"}
"#,
                ),
            ],
            poll_flag.clone(),
        )
        .content_length(None),
    );
    let api = RecordHelperApi::new_with_transport(transport.clone()).configure(|cfg| {
        cfg.max_stream_response_body_bytes(20);
    });

    let mut stream: RecordStream<LogEntry> = api.tail().execute_records().await.unwrap();
    assert!(!poll_flag.load(Ordering::SeqCst));
    let first = stream.next_record().await.unwrap().expect("first record");
    assert_eq!(first.message, "one");
    let err = stream
        .next_record()
        .await
        .expect_err("response limit should fail");
    assert!(matches!(
        err,
        ApiClientError::ResponseBodyLimitExceeded { .. }
    ));
}

#[tokio::test]
async fn generated_custom_record_request_reaches_transport() {
    const SENTINEL: &str = "SECRET_RECORD_SENTINEL_MUST_NOT_APPEAR";
    let transport = RecordingTransport::new(ResponseFixture::buffered_json(r#"{"ok":true}"#));
    let api = RecordHelperApi::new_with_transport(transport.clone());

    let response = api
        .pipe_upload(RecordBody::<PipeEntry>::from_iter(vec![
            PipeEntry {
                id: 1,
                message: SENTINEL.to_string(),
            },
            PipeEntry {
                id: 2,
                message: "world".to_string(),
            },
        ]))
        .execute()
        .await
        .expect("pipe upload succeeds");
    assert!(response.ok);

    assert_eq!(transport.send_count(), 1);
    let requests = transport.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].content_type.as_deref(),
        Some(PipeText::CONTENT_TYPE)
    );
    match &requests[0].body {
        CapturedBody::Stream(bytes) => {
            assert_eq!(
                bytes.as_ref(),
                pipe_bytes(&[
                    PipeEntry {
                        id: 1,
                        message: SENTINEL.to_string(),
                    },
                    PipeEntry {
                        id: 2,
                        message: "world".to_string(),
                    },
                ])
                .as_ref()
            );
        }
        other => panic!("expected pipe stream body, got {other:?}"),
    }
    assert!(!requests[0].debug.contains(SENTINEL));
    assert!(!format!("{:?}", requests[0]).contains(SENTINEL));
}

#[tokio::test]
async fn generated_custom_record_response_yields_records_incrementally() {
    let poll_flag = Arc::new(AtomicBool::new(false));
    let transport = RecordingTransport::new(ResponseFixture::streamed_with_content_type(
        vec![
            Bytes::from_static(b"1|hello\n2|wor"),
            Bytes::from_static(b"ld"),
        ],
        poll_flag.clone(),
        PipeText::CONTENT_TYPE,
    ));
    let api = RecordHelperApi::new_with_transport(transport.clone());

    let mut stream: RecordStream<PipeEntry> = api.pipe_tail().execute_records().await.unwrap();
    assert!(!poll_flag.load(Ordering::SeqCst));
    let first = stream.next_record().await.unwrap().expect("first record");
    assert_eq!(
        first,
        PipeEntry {
            id: 1,
            message: "hello".to_string(),
        }
    );
    assert!(poll_flag.load(Ordering::SeqCst));
    let second = stream.next_record().await.unwrap().expect("second record");
    assert_eq!(
        second,
        PipeEntry {
            id: 2,
            message: "world".to_string(),
        }
    );
    assert_eq!(stream.next_record().await.unwrap(), None);
    let debug = format!("{:?}", stream);
    assert!(debug.contains("<record stream>"));
    assert!(!debug.contains("SECRET_RECORD_SENTINEL_MUST_NOT_APPEAR"));
}

#[tokio::test]
async fn generated_custom_record_request_and_response_round_trip() {
    let poll_flag = Arc::new(AtomicBool::new(false));
    let transport = RecordingTransport::new(ResponseFixture::streamed_with_content_type(
        vec![
            Bytes::from_static(b"1|left\n"),
            Bytes::from_static(b"2|right"),
        ],
        poll_flag.clone(),
        PipeText::CONTENT_TYPE,
    ));
    let api = RecordHelperApi::new_with_transport(transport.clone());

    let mut stream: RecordStream<PipeEntry> = api
        .pipe_mirror(RecordBody::<PipeEntry>::from_iter(vec![
            PipeEntry {
                id: 1,
                message: "left".to_string(),
            },
            PipeEntry {
                id: 2,
                message: "right".to_string(),
            },
        ]))
        .execute_records()
        .await
        .unwrap();

    assert!(!poll_flag.load(Ordering::SeqCst));
    assert_eq!(
        stream.next_record().await.unwrap().unwrap(),
        PipeEntry {
            id: 1,
            message: "left".to_string(),
        }
    );
    assert_eq!(
        stream.next_record().await.unwrap().unwrap(),
        PipeEntry {
            id: 2,
            message: "right".to_string(),
        }
    );
    assert_eq!(stream.next_record().await.unwrap(), None);

    assert_eq!(transport.send_count(), 1);
    let requests = transport.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].content_type.as_deref(),
        Some(PipeText::CONTENT_TYPE)
    );
    match &requests[0].body {
        CapturedBody::Stream(bytes) => {
            assert_eq!(
                bytes.as_ref(),
                pipe_bytes(&[
                    PipeEntry {
                        id: 1,
                        message: "left".to_string(),
                    },
                    PipeEntry {
                        id: 2,
                        message: "right".to_string(),
                    },
                ])
                .as_ref()
            );
        }
        other => panic!("expected pipe stream body, got {other:?}"),
    }
}

use bytes::Bytes;
use concord_core::advanced::{
    Mixed, MultipartBody, MultipartStream, RawResponsePart, StreamBody, Transport, TransportBody,
    TransportError, TransportErrorKind, TransportRequest, TransportRequestBody, TransportResponse,
};
use concord_core::prelude::{ApiClientError, Json};
use concord_macros::api;
use futures_core::Stream;
use http::{HeaderMap, HeaderValue, Method, StatusCode};
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

const MULTIPART_SENTINEL: &[u8] = b"SECRET_MULTIPART_SENTINEL_MUST_NOT_APPEAR";

mod multipart_helper_contract {
    #![allow(unused_imports)]
    use super::*;

    api! {
        client MultipartHelperApi {
            base "https://example.com"
        }

        POST UploadDefault(body: Multipart<RawResponsePart>)
            path ["upload-default"]
            -> Json<UploadResult>

        POST UploadMixed(body: Multipart<RawResponsePart, Mixed>)
            path ["upload-mixed"]
            -> Json<UploadResult>

        GET DownloadDefault
            path ["download-default"]
            -> Multipart<RawResponsePart>

        GET DownloadMixed
            path ["download-mixed"]
            -> Multipart<RawResponsePart, Mixed>

        POST MirrorDefault(body: Multipart<RawResponsePart>)
            path ["mirror-default"]
            -> Multipart<RawResponsePart>

        POST MirrorMixed(body: Multipart<RawResponsePart, Mixed>)
            path ["mirror-mixed"]
            -> Multipart<RawResponsePart, Mixed>

        GET RetryDefault(body: Multipart<RawResponsePart>)
            path ["retry-default"]
            -> Json<UploadResult>

        POST UploadLimitDefault(body: Multipart<RawResponsePart>)
            path ["upload-limit-default"]
            -> Json<UploadResult>

        GET DownloadLimitDefault
            path ["download-limit-default"]
            -> Multipart<RawResponsePart>
    }

    pub(super) use multipart_helper_api::MultipartHelperApi;
}

use multipart_helper_contract::MultipartHelperApi;

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
struct MultipartTransport {
    events: Arc<StdMutex<Vec<String>>>,
    requests: Arc<StdMutex<Vec<CapturedRequest>>>,
    response: ResponseFixture,
    send_count: Arc<AtomicUsize>,
}

#[derive(Clone)]
enum ResponseFixture {
    BufferedJson {
        status: StatusCode,
        headers: HeaderMap,
        body: Bytes,
        content_length: Option<u64>,
    },
    Multipart {
        status: StatusCode,
        headers: HeaderMap,
        chunks: Vec<Bytes>,
        content_length: Option<u64>,
        poll_flag: Arc<AtomicBool>,
    },
    Failure {
        message: &'static str,
    },
}

impl ResponseFixture {
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

    fn multipart(
        content_type: &'static str,
        chunks: Vec<Bytes>,
        poll_flag: Arc<AtomicBool>,
    ) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static(content_type),
        );
        let content_length = chunks.iter().map(|chunk| chunk.len() as u64).sum();
        Self::Multipart {
            status: StatusCode::OK,
            headers,
            chunks,
            content_length: Some(content_length),
            poll_flag,
        }
    }

    fn failing(message: &'static str) -> Self {
        Self::Failure { message }
    }

    fn content_length(mut self, content_length: Option<u64>) -> Self {
        match &mut self {
            ResponseFixture::BufferedJson {
                content_length: len,
                ..
            } => *len = content_length,
            ResponseFixture::Multipart {
                content_length: len,
                ..
            } => *len = content_length,
            ResponseFixture::Failure { .. } => {}
        }
        self
    }
}

impl MultipartTransport {
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

    fn send_count(&self) -> usize {
        self.send_count.load(Ordering::SeqCst)
    }
}

impl Transport for MultipartTransport {
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
                .push("transport_send".to_string());
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
                ResponseFixture::Multipart {
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
                ResponseFixture::Failure { message } => Err(TransportError::with_kind(
                    TransportErrorKind::Other,
                    std::io::Error::other(message),
                )),
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

fn boundary_from_content_type(content_type: &str) -> Option<String> {
    content_type
        .split(';')
        .map(str::trim)
        .find_map(|part| part.strip_prefix("boundary="))
        .map(|boundary| boundary.trim_matches('"').to_string())
}

fn multipart_response_chunks(boundary: &str) -> Vec<Bytes> {
    vec![
        Bytes::from(format!("--{boundary}\r\nContent-Ty")),
        Bytes::from(format!(
            "pe: text/plain\r\nContent-Disposition: attachment; filename=\"a.txt\"\r\n\r\nhello\r\n--{boundary}\r\nContent-Type: text/plain\r\n\r\nwo"
        )),
        Bytes::from(format!("rld\r\n--{boundary}--\r\n")),
    ]
}

fn multipart_request_body() -> MultipartBody {
    MultipartBody::new()
        .text("title", "hello")
        .bytes("file", Bytes::from_static(b"abc"))
}

fn multipart_request_body_with_stream() -> MultipartBody {
    MultipartBody::new().stream(
        "upload",
        StreamBody::from_bytes(Bytes::from_static(MULTIPART_SENTINEL)),
    )
}

#[tokio::test]
async fn generated_multipart_request_reaches_transport() {
    let transport = MultipartTransport::new(ResponseFixture::buffered_json(r#"{"ok":true}"#));
    let api = MultipartHelperApi::new_with_transport(transport.clone());

    let response = api
        .upload_default(multipart_request_body())
        .execute()
        .await
        .expect("multipart upload succeeds");
    assert!(response.ok);

    assert_eq!(transport.send_count(), 1);
    let requests = transport.requests();
    assert_eq!(requests.len(), 1);
    let content_type = requests[0]
        .content_type
        .as_deref()
        .expect("multipart content type");
    assert!(content_type.starts_with("multipart/form-data; boundary="));
    let boundary = boundary_from_content_type(content_type).expect("boundary parameter");
    match &requests[0].body {
        CapturedBody::Stream(bytes) => {
            let rendered = String::from_utf8(bytes.clone().to_vec()).expect("multipart bytes");
            assert!(rendered.contains("\r\n"));
            assert!(rendered.contains("Content-Disposition:"));
            assert!(rendered.contains("hello"));
            assert!(rendered.contains("abc"));
            assert!(rendered.starts_with(&format!("--{boundary}\r\n")));
            assert!(rendered.ends_with(&format!("--{boundary}--\r\n")));
        }
        other => panic!("expected stream body, got {other:?}"),
    }
    assert!(
        !requests[0]
            .debug
            .contains("SECRET_MULTIPART_SENTINEL_MUST_NOT_APPEAR")
    );
    assert!(!format!("{:?}", requests[0]).contains("SECRET_MULTIPART_SENTINEL_MUST_NOT_APPEAR"));
}

#[tokio::test]
async fn generated_multipart_response_execute_multipart_returns_stream_without_buffering() {
    let poll_flag = Arc::new(AtomicBool::new(false));
    let transport = MultipartTransport::new(ResponseFixture::multipart(
        "multipart/mixed; boundary=BOUNDARY",
        multipart_response_chunks("BOUNDARY"),
        poll_flag.clone(),
    ));
    let api = MultipartHelperApi::new_with_transport(transport.clone());

    let mut stream: MultipartStream<RawResponsePart> = api
        .download_mixed()
        .execute_multipart()
        .await
        .expect("multipart response succeeds");

    assert_eq!(stream.status(), StatusCode::OK);
    assert_eq!(
        stream
            .headers()
            .get(http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("multipart/mixed; boundary=BOUNDARY")
    );
    assert!(!poll_flag.load(Ordering::SeqCst));
    let debug = format!("{:?}", stream);
    assert!(!debug.contains("SECRET_MULTIPART_SENTINEL_MUST_NOT_APPEAR"));

    let first = stream
        .next_part()
        .await
        .expect("first part")
        .expect("first part");
    let first_debug = format!("{first:?}");
    assert!(!first_debug.contains("SECRET_MULTIPART_SENTINEL_MUST_NOT_APPEAR"));
    let first_bytes = first.bytes_limited(1024).await.expect("first part bytes");
    assert_eq!(first_bytes, Bytes::from_static(b"hello"));

    let second = stream
        .next_part()
        .await
        .expect("second part")
        .expect("second part");
    let second_bytes = second.bytes_limited(1024).await.expect("second part bytes");
    assert_eq!(second_bytes, Bytes::from_static(b"world"));

    assert!(stream.next_part().await.expect("stream end").is_none());
    assert!(poll_flag.load(Ordering::SeqCst));
}

#[tokio::test]
async fn generated_multipart_response_execute_returns_stream_without_buffering() {
    let poll_flag = Arc::new(AtomicBool::new(false));
    let transport = MultipartTransport::new(ResponseFixture::multipart(
        "multipart/form-data; boundary=BOUNDARY",
        multipart_response_chunks("BOUNDARY"),
        poll_flag.clone(),
    ));
    let api = MultipartHelperApi::new_with_transport(transport.clone());

    let mut stream: MultipartStream<RawResponsePart> = api
        .download_default()
        .execute()
        .await
        .expect("multipart response succeeds");

    assert_eq!(stream.status(), StatusCode::OK);
    assert_eq!(
        stream
            .headers()
            .get(http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("multipart/form-data; boundary=BOUNDARY")
    );
    assert!(!poll_flag.load(Ordering::SeqCst));
    let first = stream
        .next_part()
        .await
        .expect("first part")
        .expect("first part");
    let first_bytes = first.bytes_limited(1024).await.expect("first part bytes");
    assert_eq!(first_bytes, Bytes::from_static(b"hello"));
    assert!(poll_flag.load(Ordering::SeqCst));
}

#[tokio::test]
async fn generated_multipart_request_is_not_retried_or_replayed() {
    let transport =
        MultipartTransport::new(ResponseFixture::failing("multipart transport failure"));
    let api = MultipartHelperApi::new_with_transport(transport.clone()).configure(|cfg| {
        cfg.retry_policy(Arc::new(
            concord_core::advanced::ConfiguredRetryPolicy::new(
                concord_core::advanced::RetryConfig {
                    max_attempts: 2,
                    methods: vec![Method::GET],
                    statuses: Vec::new(),
                    transport_errors: vec![TransportErrorKind::Other],
                    backoff: concord_core::advanced::RetryBackoff::None,
                    respect_retry_after: false,
                    idempotency: concord_core::advanced::RetryIdempotency::SafeMethodsOnly,
                },
            ),
        ));
    });

    let err = api
        .retry_default(multipart_request_body_with_stream())
        .execute()
        .await
        .expect_err("multipart body should not be replayed");
    assert_eq!(transport.send_count(), 1);
    let requests = transport.requests();
    assert_eq!(requests.len(), 1);
    match &requests[0].body {
        CapturedBody::Stream(bytes) => {
            assert!(
                bytes
                    .as_ref()
                    .windows(MULTIPART_SENTINEL.len())
                    .any(|window| window == MULTIPART_SENTINEL)
            );
        }
        other => panic!("expected stream body, got {other:?}"),
    }
    assert!(!format!("{err:?}").contains("SECRET_MULTIPART_SENTINEL_MUST_NOT_APPEAR"));
}

#[tokio::test]
async fn generated_multipart_request_stream_limit_applies() {
    let transport = MultipartTransport::new(ResponseFixture::buffered_json(r#"{"ok":true}"#));
    let api = MultipartHelperApi::new_with_transport(transport.clone()).configure(|cfg| {
        cfg.max_stream_request_body_bytes(4);
    });

    let err = api
        .upload_limit_default(multipart_request_body())
        .execute()
        .await
        .expect_err("multipart request should exceed limit");
    assert!(matches!(
        err,
        ApiClientError::RequestBodyLimitExceeded { limit: 4, .. }
    ));
    assert_eq!(transport.send_count(), 1);
}

#[tokio::test]
async fn generated_multipart_response_stream_limit_applies() {
    let poll_flag = Arc::new(AtomicBool::new(false));
    let transport = MultipartTransport::new(
        ResponseFixture::multipart(
            "multipart/form-data; boundary=BOUNDARY",
            vec![Bytes::from_static(
                b"--BOUNDARY\r\nContent-Type: text/plain\r\n\r\nhello\r\n--BOUNDARY--\r\n",
            )],
            poll_flag.clone(),
        )
        .content_length(Some(200)),
    );
    let api = MultipartHelperApi::new_with_transport(transport.clone()).configure(|cfg| {
        cfg.max_stream_response_body_bytes(120);
    });

    let err = api
        .download_limit_default()
        .execute_multipart()
        .await
        .expect_err("multipart response should exceed limit before exposure");
    assert!(matches!(err, ApiClientError::ResponseTooLarge { .. }));
    assert!(!poll_flag.load(Ordering::SeqCst));
}

#[tokio::test]
async fn generated_multipart_request_and_response_round_trip() {
    let poll_flag = Arc::new(AtomicBool::new(false));
    let transport = MultipartTransport::new(ResponseFixture::multipart(
        "multipart/form-data; boundary=BOUNDARY",
        multipart_response_chunks("BOUNDARY"),
        poll_flag.clone(),
    ));
    let api = MultipartHelperApi::new_with_transport(transport.clone());

    let mut stream: MultipartStream<RawResponsePart> = api
        .mirror_default(multipart_request_body())
        .execute_multipart()
        .await
        .expect("multipart round trip succeeds");

    let first = stream
        .next_part()
        .await
        .expect("first part")
        .expect("first part");
    assert_eq!(
        first.content_type().and_then(|value| value.to_str().ok()),
        Some("text/plain")
    );
    assert!(poll_flag.load(Ordering::SeqCst));
}

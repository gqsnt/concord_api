use super::common::{TestAuthVars, TestCx, decode_string};
use bytes::Bytes;
use concord_core::advanced::{
    JsonSse, RateLimitContext, RateLimitFuture, RateLimitPermit, RateLimitResponseAction,
    RateLimitResponseContext, RateLimiter, Transport, TransportBody, TransportError,
    TransportErrorKind, TransportRequest, TransportResponse,
};
use concord_core::internal::{
    BodyPlan, EndpointMeta, EndpointPlan, RequestArgs, RequestOverrides, RequestPlan,
    ResolvedPolicy, ResolvedRoute, ResponsePlan,
};
use concord_core::prelude::{ApiClient, ApiClientError, DebugLevel};
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use serde::Deserialize;
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

#[derive(Clone)]
struct RecordingRateLimiter {
    events: Arc<StdMutex<Vec<String>>>,
}

impl RecordingRateLimiter {
    fn new(events: Arc<StdMutex<Vec<String>>>) -> Self {
        Self { events }
    }
}

impl RateLimiter for RecordingRateLimiter {
    fn acquire<'a>(
        &'a self,
        _ctx: RateLimitContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitPermit, ApiClientError>> {
        let events = self.events.clone();
        Box::pin(async move {
            events
                .lock()
                .expect("events lock")
                .push("rate_limit_acquire".to_string());
            Ok(RateLimitPermit)
        })
    }

    fn on_response<'a>(
        &'a self,
        _ctx: RateLimitResponseContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitResponseAction, ApiClientError>> {
        let events = self.events.clone();
        Box::pin(async move {
            events
                .lock()
                .expect("events lock")
                .push("rate_limit_response".to_string());
            Ok(RateLimitResponseAction::Continue)
        })
    }
}

#[derive(Clone, Debug)]
struct ResponseFixture {
    status: StatusCode,
    headers: HeaderMap,
    chunks: Vec<Bytes>,
    content_length: Option<u64>,
    poll_flag: Option<Arc<AtomicBool>>,
    read_count: Option<Arc<AtomicUsize>>,
}

impl ResponseFixture {
    fn sse(status: StatusCode, chunks: Vec<Bytes>) -> Self {
        let content_length = chunks.iter().fold(Some(0u64), |acc, chunk| {
            acc.and_then(|len| len.checked_add(chunk.len() as u64))
        });
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("text/event-stream"),
        );
        Self {
            status,
            headers,
            chunks,
            content_length,
            poll_flag: None,
            read_count: None,
        }
    }

    fn with_read_count(mut self, read_count: Arc<AtomicUsize>) -> Self {
        self.read_count = Some(read_count);
        self
    }

    fn content_type(mut self, value: Option<HeaderValue>) -> Self {
        self.headers = HeaderMap::new();
        if let Some(value) = value {
            self.headers.insert(http::header::CONTENT_TYPE, value);
        }
        self
    }

    fn content_length(mut self, value: Option<u64>) -> Self {
        self.content_length = value;
        self
    }
}

#[derive(Clone)]
struct SseTransport {
    events: Arc<StdMutex<Vec<String>>>,
    responses: Arc<StdMutex<VecDeque<ResponseFixture>>>,
    send_count: Arc<AtomicUsize>,
}

impl SseTransport {
    fn new(events: Arc<StdMutex<Vec<String>>>, responses: Vec<ResponseFixture>) -> Self {
        Self {
            events,
            responses: Arc::new(StdMutex::new(responses.into())),
            send_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn send_count(&self) -> usize {
        self.send_count.load(Ordering::SeqCst)
    }
}

impl Transport for SseTransport {
    fn send(
        &self,
        req: TransportRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send>> {
        let events = self.events.clone();
        let responses = self.responses.clone();
        let send_count = self.send_count.clone();
        Box::pin(async move {
            send_count.fetch_add(1, Ordering::SeqCst);
            events
                .lock()
                .expect("events lock")
                .push("transport_send".to_string());
            let mut responses = responses.lock().expect("responses lock");
            let response = responses.pop_front().ok_or_else(|| {
                TransportError::with_kind(
                    TransportErrorKind::Other,
                    std::io::Error::other("sse transport exhausted"),
                )
            })?;
            let body = ChunkBody::new(
                events.clone(),
                response.chunks,
                response.poll_flag,
                response.read_count,
            );
            Ok(TransportResponse {
                meta: req.meta,
                url: req.url,
                status: response.status,
                headers: response.headers,
                content_length: response.content_length,
                rate_limit: req.rate_limit,
                body: Box::new(body),
            })
        })
    }
}

struct ChunkBody {
    events: Arc<StdMutex<Vec<String>>>,
    chunks: VecDeque<Bytes>,
    poll_flag: Option<Arc<AtomicBool>>,
    read_count: Option<Arc<AtomicUsize>>,
}

impl ChunkBody {
    fn new(
        events: Arc<StdMutex<Vec<String>>>,
        chunks: Vec<Bytes>,
        poll_flag: Option<Arc<AtomicBool>>,
        read_count: Option<Arc<AtomicUsize>>,
    ) -> Self {
        Self {
            events,
            chunks: chunks.into(),
            poll_flag,
            read_count,
        }
    }
}

impl TransportBody for ChunkBody {
    fn next_chunk<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Bytes>, TransportError>> + Send + 'a>> {
        let events = self.events.clone();
        let poll_flag = self.poll_flag.clone();
        let read_count = self.read_count.clone();
        let chunk = self.chunks.pop_front();
        Box::pin(async move {
            if let Some(flag) = poll_flag {
                flag.store(true, Ordering::SeqCst);
            }
            if let Some(read_count) = &read_count {
                read_count.fetch_add(1, Ordering::SeqCst);
            }
            events
                .lock()
                .expect("events lock")
                .push("sse_chunk_poll".to_string());
            Ok(chunk)
        })
    }
}

fn sse_response_plan(
    name: &'static str,
    path: &'static str,
    policy: ResolvedPolicy,
    pagination: Option<concord_core::internal::PaginationMarker>,
) -> RequestPlan {
    RequestPlan {
        endpoint: EndpointPlan {
            meta: EndpointMeta {
                name,
                method: Method::GET,
                idempotent: true,
                facade_path: &[],
            },
            route: ResolvedRoute::new(http::uri::Scheme::HTTPS, "example.com", path),
            policy,
            body: BodyPlan::None,
            response: ResponsePlan {
                accept: Some(HeaderValue::from_static("text/event-stream")),
                no_content: false,
                format: concord_core::internal::Format::Text,
                decode: decode_string,
            },
            pagination,
        },
        args: RequestArgs::default(),
        overrides: RequestOverrides::default(),
    }
}

fn sse_no_content_plan(name: &'static str, path: &'static str) -> RequestPlan {
    let mut plan = sse_response_plan(name, path, ResolvedPolicy::default(), None);
    plan.endpoint.response.no_content = true;
    plan
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct LogEvent {
    id: u64,
    msg: String,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct FlagEvent {
    ok: bool,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct LinesEvent {
    lines: Vec<String>,
}

#[tokio::test]
async fn sse_response_yields_json_events_incrementally() -> Result<(), ApiClientError> {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let read_count = Arc::new(AtomicUsize::new(0));
    let transport = SseTransport::new(
        events.clone(),
        vec![
            ResponseFixture::sse(
                StatusCode::OK,
                vec![
                    Bytes::from_static(b"data: {\"id\":1,\"msg\":\"hel"),
                    Bytes::from_static(b"lo\"}\n\ndata: {\"id\":2,\"msg\":\"world\"}\n\n"),
                ],
            )
            .with_read_count(read_count.clone()),
        ],
    );
    let mut client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
    client.configure(|cfg| {
        cfg.rate_limiter(Arc::new(RecordingRateLimiter::new(events.clone())));
        cfg.debug(DebugLevel::VV);
    });

    let mut stream = <concord_core::advanced::SseResponse<LogEvent, JsonSse> as concord_core::advanced::ResponseEntity>::execute(&client, sse_response_plan(
            "SseIncremental",
            "/sse-incremental",
            ResolvedPolicy::default(),
            None,
        ))
        .await?;

    assert_eq!(read_count.load(Ordering::SeqCst), 0);
    let before = events.lock().expect("events lock").clone();
    assert!(
        !before.iter().any(|event| event == "sse_chunk_poll"),
        "response body should not be polled before return"
    );

    let first = stream.next_event().await?.expect("first event");
    assert_eq!(
        first.data,
        LogEvent {
            id: 1,
            msg: "hello".to_string()
        }
    );
    let second = stream.next_event().await?.expect("second event");
    assert_eq!(
        second.data,
        LogEvent {
            id: 2,
            msg: "world".to_string()
        }
    );
    assert!(stream.next_event().await?.is_none());

    assert!(read_count.load(Ordering::SeqCst) > 0);
    let events = events.lock().expect("events lock").clone();
    let acquire = events
        .iter()
        .position(|event| event == "rate_limit_acquire")
        .unwrap();
    let send = events
        .iter()
        .position(|event| event == "transport_send")
        .unwrap();
    let response = events
        .iter()
        .position(|event| event == "rate_limit_response")
        .unwrap();
    let poll = events
        .iter()
        .position(|event| event == "sse_chunk_poll")
        .unwrap();
    assert!(acquire < send);
    assert!(send < response);
    assert!(response < poll);
    Ok(())
}

#[tokio::test]
async fn sse_metadata_fields_are_parsed() -> Result<(), ApiClientError> {
    let transport = SseTransport::new(
        Arc::new(StdMutex::new(Vec::new())),
        vec![ResponseFixture::sse(
            StatusCode::OK,
            vec![Bytes::from_static(
                b"id: abc\nevent: update\nretry: 1500\ndata: {\"ok\":true}\n\n",
            )],
        )],
    );
    let client = ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport);

    let mut stream = <concord_core::advanced::SseResponse<FlagEvent, JsonSse> as concord_core::advanced::ResponseEntity>::execute(&client, sse_response_plan(
            "SseMetadata",
            "/sse-metadata",
            ResolvedPolicy::default(),
            None,
        ))
        .await?;

    let event = stream.next_event().await?.expect("event");
    assert_eq!(event.event.as_deref(), Some("update"));
    assert_eq!(event.id.as_deref(), Some("abc"));
    assert_eq!(event.retry, Some(std::time::Duration::from_millis(1500)));
    assert_eq!(event.data, FlagEvent { ok: true });
    Ok(())
}

#[tokio::test]
async fn sse_multi_line_data_joins_with_newline() -> Result<(), ApiClientError> {
    let transport = SseTransport::new(
        Arc::new(StdMutex::new(Vec::new())),
        vec![ResponseFixture::sse(
            StatusCode::OK,
            vec![Bytes::from_static(
                b"data: {\"lines\":[\"a\",\ndata: \"b\"]}\n\n",
            )],
        )],
    );
    let client = ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport);

    let mut stream = <concord_core::advanced::SseResponse<LinesEvent, JsonSse> as concord_core::advanced::ResponseEntity>::execute(&client, sse_response_plan(
            "SseMultiline",
            "/sse-multiline",
            ResolvedPolicy::default(),
            None,
        ))
        .await?;

    let event = stream.next_event().await?.expect("event");
    assert_eq!(
        event.data,
        LinesEvent {
            lines: vec!["a".to_string(), "b".to_string()]
        }
    );
    Ok(())
}

#[tokio::test]
async fn sse_comments_and_unknown_fields_are_ignored() -> Result<(), ApiClientError> {
    let transport = SseTransport::new(
        Arc::new(StdMutex::new(Vec::new())),
        vec![ResponseFixture::sse(
            StatusCode::OK,
            vec![Bytes::from_static(
                b": comment\nunknown: ignored\ndata: {\"ok\":true}\n\n",
            )],
        )],
    );
    let client = ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport);

    let mut stream = <concord_core::advanced::SseResponse<FlagEvent, JsonSse> as concord_core::advanced::ResponseEntity>::execute(&client, sse_response_plan(
            "SseComments",
            "/sse-comments",
            ResolvedPolicy::default(),
            None,
        ))
        .await?;

    let event = stream.next_event().await?.expect("event");
    assert_eq!(event.data, FlagEvent { ok: true });
    Ok(())
}

#[tokio::test]
async fn sse_final_event_without_blank_line_dispatches_on_eof() -> Result<(), ApiClientError> {
    let transport = SseTransport::new(
        Arc::new(StdMutex::new(Vec::new())),
        vec![ResponseFixture::sse(
            StatusCode::OK,
            vec![Bytes::from_static(b"data: {\"ok\":true}")],
        )],
    );
    let client = ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport);

    let mut stream = <concord_core::advanced::SseResponse<FlagEvent, JsonSse> as concord_core::advanced::ResponseEntity>::execute(&client, sse_response_plan(
            "SseFinalEvent",
            "/sse-final-event",
            ResolvedPolicy::default(),
            None,
        ))
        .await?;

    assert_eq!(
        stream.next_event().await?.expect("event").data,
        FlagEvent { ok: true }
    );
    assert!(stream.next_event().await?.is_none());
    Ok(())
}

#[tokio::test]
async fn sse_invalid_utf8_fails_body_safely() {
    let transport = SseTransport::new(
        Arc::new(StdMutex::new(Vec::new())),
        vec![ResponseFixture::sse(
            StatusCode::OK,
            vec![Bytes::from_static(b"data: \xFF\n\n")],
        )],
    );
    let client = ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport);

    let mut stream = <concord_core::advanced::SseResponse<FlagEvent, JsonSse> as concord_core::advanced::ResponseEntity>::execute(&client, sse_response_plan(
            "SseInvalidUtf8",
            "/sse-invalid-utf8",
            ResolvedPolicy::default(),
            None,
        ))
        .await
        .expect("stream should be returned");

    let err = stream
        .next_event()
        .await
        .expect_err("invalid UTF-8 should fail");
    assert!(matches!(err, ApiClientError::Codec { .. }));
    assert!(!format!("{err:?}").contains("SECRET_SSE_SENTINEL_MUST_NOT_APPEAR"));
    assert!(!format!("{err}").contains("SECRET_SSE_SENTINEL_MUST_NOT_APPEAR"));
}

#[tokio::test]
async fn sse_json_decode_error_is_sanitized() {
    let sentinel = "SECRET_SSE_SENTINEL_MUST_NOT_APPEAR";
    let transport = SseTransport::new(
        Arc::new(StdMutex::new(Vec::new())),
        vec![ResponseFixture::sse(
            StatusCode::OK,
            vec![Bytes::from(format!("data: {{\"bad\": {sentinel}}}\n\n"))],
        )],
    );
    let client = ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport);

    let mut stream = <concord_core::advanced::SseResponse<FlagEvent, JsonSse> as concord_core::advanced::ResponseEntity>::execute(&client, sse_response_plan(
            "SseJsonDecodeError",
            "/sse-json-decode-error",
            ResolvedPolicy::default(),
            None,
        ))
        .await
        .expect("stream should be returned");

    let err = stream
        .next_event()
        .await
        .expect_err("invalid JSON should fail");
    assert!(matches!(err, ApiClientError::Codec { .. }));
    assert!(!format!("{err:?}").contains(sentinel));
    assert!(!format!("{err}").contains(sentinel));
}

#[tokio::test]
async fn sse_wrong_content_type_is_rejected_before_body_exposure() {
    let read_count = Arc::new(AtomicUsize::new(0));
    let transport = SseTransport::new(
        Arc::new(StdMutex::new(Vec::new())),
        vec![
            ResponseFixture::sse(
                StatusCode::OK,
                vec![Bytes::from_static(b"data: {\"ok\":true}\n\n")],
            )
            .content_type(Some(HeaderValue::from_static("application/json")))
            .with_read_count(read_count.clone()),
        ],
    );
    let client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());

    let err = <concord_core::advanced::SseResponse<FlagEvent, JsonSse> as concord_core::advanced::ResponseEntity>::execute(&client, sse_response_plan(
            "SseWrongContentType",
            "/sse-wrong-content-type",
            ResolvedPolicy::default(),
            None,
        ))
        .await
        .expect_err("wrong content type should fail");

    assert!(matches!(err, ApiClientError::PolicyViolation { .. }));
    assert!(
        err.to_string()
            .contains("sse response content type did not match expected media type")
    );
    assert_eq!(read_count.load(Ordering::SeqCst), 0);
    assert_eq!(transport.send_count(), 1);
}

#[tokio::test]
async fn sse_missing_content_type_is_rejected_before_body_exposure() {
    let read_count = Arc::new(AtomicUsize::new(0));
    let transport = SseTransport::new(
        Arc::new(StdMutex::new(Vec::new())),
        vec![
            ResponseFixture::sse(
                StatusCode::OK,
                vec![Bytes::from_static(b"data: {\"ok\":true}\n\n")],
            )
            .content_type(None)
            .with_read_count(read_count.clone()),
        ],
    );
    let client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());

    let err = <concord_core::advanced::SseResponse<FlagEvent, JsonSse> as concord_core::advanced::ResponseEntity>::execute(&client, sse_response_plan(
            "SseMissingContentType",
            "/sse-missing-content-type",
            ResolvedPolicy::default(),
            None,
        ))
        .await
        .expect_err("missing content type should fail");

    assert!(matches!(err, ApiClientError::PolicyViolation { .. }));
    assert!(
        err.to_string()
            .contains("sse response content type did not match expected media type")
    );
    assert_eq!(read_count.load(Ordering::SeqCst), 0);
    assert_eq!(transport.send_count(), 1);
}

#[tokio::test]
async fn sse_content_length_preflight_applies() {
    let read_count = Arc::new(AtomicUsize::new(0));
    let transport = SseTransport::new(
        Arc::new(StdMutex::new(Vec::new())),
        vec![
            ResponseFixture::sse(
                StatusCode::OK,
                vec![Bytes::from_static(b"data: {\"ok\":true}\n\n")],
            )
            .content_length(Some(128))
            .with_read_count(read_count.clone()),
        ],
    );
    let mut client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
    client.configure(|cfg| {
        cfg.max_stream_response_body_bytes(8);
    });

    let err = <concord_core::advanced::SseResponse<FlagEvent, JsonSse> as concord_core::advanced::ResponseEntity>::execute(&client, sse_response_plan(
            "SseContentLengthLimit",
            "/sse-content-length-limit",
            ResolvedPolicy::default(),
            None,
        ))
        .await
        .expect_err("content length above limit should fail");

    assert!(matches!(err, ApiClientError::ResponseTooLarge { .. }));
    assert_eq!(read_count.load(Ordering::SeqCst), 0);
    assert_eq!(transport.send_count(), 1);
}

#[tokio::test]
async fn sse_unknown_length_limit_applies_while_reading() -> Result<(), ApiClientError> {
    let read_count = Arc::new(AtomicUsize::new(0));
    let transport = SseTransport::new(
        Arc::new(StdMutex::new(Vec::new())),
        vec![
            ResponseFixture::sse(
                StatusCode::OK,
                vec![
                    Bytes::from_static(b"data: {\"ok\":true}\n\n"),
                    Bytes::from_static(
                        b"data: {\"msg\":\"SECRET_SSE_SENTINEL_MUST_NOT_APPEAR\"}\n\n",
                    ),
                ],
            )
            .content_length(None)
            .with_read_count(read_count.clone()),
        ],
    );
    let mut client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
    client.configure(|cfg| {
        cfg.max_stream_response_body_bytes(20);
    });

    let mut stream = <concord_core::advanced::SseResponse<FlagEvent, JsonSse> as concord_core::advanced::ResponseEntity>::execute(&client, sse_response_plan(
            "SseUnknownLengthLimit",
            "/sse-unknown-length-limit",
            ResolvedPolicy::default(),
            None,
        ))
        .await?;

    assert_eq!(
        stream.next_event().await?.expect("first event").data,
        FlagEvent { ok: true }
    );
    let err = stream
        .next_event()
        .await
        .expect_err("limit should fail while reading");
    assert!(matches!(
        err,
        ApiClientError::ResponseBodyLimitExceeded { .. }
    ));
    assert!(!format!("{err:?}").contains("SECRET_SSE_SENTINEL_MUST_NOT_APPEAR"));
    assert!(!format!("{err}").contains("SECRET_SSE_SENTINEL_MUST_NOT_APPEAR"));
    assert!(read_count.load(Ordering::SeqCst) > 0);
    assert_eq!(transport.send_count(), 1);
    Ok(())
}

#[tokio::test]
async fn sse_pagination_is_rejected_before_transport() {
    let transport = SseTransport::new(Arc::new(StdMutex::new(Vec::new())), vec![]);
    let client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());

    let mut plan = sse_response_plan(
        "SsePagination",
        "/sse-pagination",
        ResolvedPolicy::default(),
        Some(concord_core::internal::PaginationMarker),
    );
    plan.endpoint.response.accept = Some(HeaderValue::from_static("text/event-stream"));

    let err = <concord_core::advanced::SseResponse<FlagEvent, JsonSse> as concord_core::advanced::ResponseEntity>::execute(&client, plan)
        .await
        .expect_err("pagination should fail");

    assert!(matches!(err, ApiClientError::PolicyViolation { .. }));
    assert_eq!(transport.send_count(), 0);
}

#[tokio::test]
async fn sse_no_content_is_rejected_before_transport() {
    let transport = SseTransport::new(Arc::new(StdMutex::new(Vec::new())), vec![]);
    let client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());

    let err = <concord_core::advanced::SseResponse<FlagEvent, JsonSse> as concord_core::advanced::ResponseEntity>::execute(&client, sse_no_content_plan(
            "SseNoContent",
            "/sse-no-content",
        ))
        .await
        .expect_err("no-content should fail");

    assert!(matches!(err, ApiClientError::PolicyViolation { .. }));
    assert_eq!(transport.send_count(), 0);
}

#[tokio::test]
async fn sse_debug_is_body_free() -> Result<(), ApiClientError> {
    let sentinel = "SECRET_SSE_SENTINEL_MUST_NOT_APPEAR";
    let transport = SseTransport::new(
        Arc::new(StdMutex::new(Vec::new())),
        vec![ResponseFixture::sse(
            StatusCode::OK,
            vec![Bytes::from(format!("data: {{\"msg\":\"{sentinel}\"}}\n\n"))],
        )],
    );
    let client = ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport);

    let stream = <concord_core::advanced::SseResponse<FlagEvent, JsonSse> as concord_core::advanced::ResponseEntity>::execute(&client, sse_response_plan(
            "SseDebug",
            "/sse-debug",
            ResolvedPolicy::default(),
            None,
        ))
        .await?;

    let debug = format!("{stream:?}");
    assert!(!debug.contains(sentinel));
    assert!(debug.contains("<sse stream>"));
    Ok(())
}

use super::common::{MockResponse, TestAuthVars, TestCx, auth_policy};
use bytes::Bytes;
use concord_core::advanced::{
    AuthPlacement, BodySizeHint, DebugSink, PostResponseHookContext, PreSendHookContext,
    RateLimitContext, RateLimitFuture, RateLimitPermit, RateLimitResponseAction,
    RateLimitResponseContext, RateLimiter, RuntimeHooks, StreamBody, Transport, TransportBody,
    TransportError, TransportErrorKind, TransportRequest, TransportRequestBody, TransportResponse,
};
use concord_core::internal::{
    BodyPlan, EndpointMeta, EndpointPlan, RequestArgs, RequestOverrides, RequestPlan,
    ResolvedPolicy, ResolvedRoute, ResponsePlan,
};
use concord_core::prelude::{ApiClient, ApiClientError, DebugLevel};
use futures_core::Stream;
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::task::{Context, Poll};

#[derive(Clone)]
struct RecordingDebugSink {
    events: Arc<StdMutex<Vec<String>>>,
}

impl RecordingDebugSink {
    fn new(events: Arc<StdMutex<Vec<String>>>) -> Self {
        Self { events }
    }
}

impl DebugSink for RecordingDebugSink {
    fn request_start(
        &self,
        dbg: concord_core::prelude::DebugLevel,
        _method: &Method,
        _url: &str,
        endpoint: &'static str,
        page_index: u32,
    ) {
        self.events
            .lock()
            .expect("debug events lock")
            .push(format!("debug_request:{dbg}:{endpoint}:{page_index}"));
    }

    fn request_headers(&self, dbg: concord_core::prelude::DebugLevel, _headers: &HeaderMap) {
        self.events
            .lock()
            .expect("debug events lock")
            .push(format!("debug_request_headers:{dbg}"));
    }

    fn response_status(
        &self,
        dbg: concord_core::prelude::DebugLevel,
        status: StatusCode,
        _url: &str,
        ok: bool,
    ) {
        self.events
            .lock()
            .expect("debug events lock")
            .push(format!("debug_response:{dbg}:{status}:{ok}"));
    }

    fn response_headers(&self, dbg: concord_core::prelude::DebugLevel, _headers: &HeaderMap) {
        self.events
            .lock()
            .expect("debug events lock")
            .push(format!("debug_response_headers:{dbg}"));
    }
}

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
                .expect("rate limit events lock")
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
                .expect("rate limit events lock")
                .push("rate_limit_response".to_string());
            Ok(RateLimitResponseAction::Continue)
        })
    }
}

#[derive(Clone)]
struct RecordingHooks {
    events: Arc<StdMutex<Vec<String>>>,
}

impl RecordingHooks {
    fn new(events: Arc<StdMutex<Vec<String>>>) -> Self {
        Self { events }
    }
}

impl RuntimeHooks for RecordingHooks {
    fn pre_send<'a>(
        &'a self,
        ctx: PreSendHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ApiClientError>> + Send + 'a>> {
        let events = self.events.clone();
        Box::pin(async move {
            events
                .lock()
                .expect("hook events lock")
                .push(format!("hook_pre_send:{}", ctx.meta.endpoint));
            Ok(())
        })
    }

    fn post_response<'a>(
        &'a self,
        ctx: PostResponseHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        let events = self.events.clone();
        Box::pin(async move {
            events
                .lock()
                .expect("hook events lock")
                .push(format!("hook_post_response:{}", ctx.meta.endpoint));
        })
    }

    fn transport_error<'a>(
        &'a self,
        ctx: concord_core::advanced::TransportErrorHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        let events = self.events.clone();
        Box::pin(async move {
            events
                .lock()
                .expect("hook events lock")
                .push(format!("hook_transport_error:{}", ctx.meta.endpoint));
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum CapturedBody {
    Empty,
    Bytes(Bytes),
    Stream(Bytes),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CapturedRequest {
    debug: String,
    content_type: Option<String>,
    body: CapturedBody,
}

#[derive(Clone)]
struct StreamTransport {
    events: Arc<StdMutex<Vec<String>>>,
    captured: Arc<StdMutex<Vec<CapturedRequest>>>,
    response: MockResponse,
    transport_error: Option<TransportErrorKind>,
    send_count: Arc<AtomicUsize>,
}

impl StreamTransport {
    fn success(events: Arc<StdMutex<Vec<String>>>, response: MockResponse) -> Self {
        Self {
            events,
            captured: Arc::new(StdMutex::new(Vec::new())),
            response,
            transport_error: None,
            send_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn transport_error(
        events: Arc<StdMutex<Vec<String>>>,
        response: MockResponse,
        kind: TransportErrorKind,
    ) -> Self {
        Self {
            events,
            captured: Arc::new(StdMutex::new(Vec::new())),
            response,
            transport_error: Some(kind),
            send_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn send_count(&self) -> usize {
        self.send_count.load(Ordering::SeqCst)
    }

    fn captured(&self) -> Vec<CapturedRequest> {
        self.captured
            .lock()
            .expect("captured requests lock")
            .clone()
    }
}

impl Transport for StreamTransport {
    fn send(
        &self,
        req: TransportRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send>> {
        let events = self.events.clone();
        let captured = self.captured.clone();
        let response = self.response.clone();
        let transport_error = self.transport_error;
        let send_count = self.send_count.clone();
        Box::pin(async move {
            send_count.fetch_add(1, Ordering::SeqCst);
            let debug = format!("{req:?}");
            events
                .lock()
                .expect("stream transport events lock")
                .push("transport".to_string());
            events
                .lock()
                .expect("stream transport events lock")
                .push(format!("transport_debug:{debug}"));
            let content_type = req
                .headers
                .get(http::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string);
            let body = match req.body {
                TransportRequestBody::Empty => CapturedBody::Empty,
                TransportRequestBody::Bytes(bytes) => CapturedBody::Bytes(bytes),
                TransportRequestBody::Stream(stream) => {
                    CapturedBody::Stream(collect_stream(stream, &events).await?)
                }
            };
            captured
                .lock()
                .expect("captured requests lock")
                .push(CapturedRequest {
                    debug,
                    content_type,
                    body,
                });

            if let Some(kind) = transport_error {
                return Err(TransportError::with_kind(
                    kind,
                    std::io::Error::other("stream transport failure"),
                ));
            }

            Ok(TransportResponse {
                meta: req.meta,
                url: req.url,
                status: response.status,
                headers: response.headers,
                content_length: Some(response.body.len() as u64),
                rate_limit: req.rate_limit,
                body: Box::new(StaticBody::new(Bytes::from(response.body))),
            })
        })
    }
}

struct StaticBody {
    next: Option<Bytes>,
}

impl StaticBody {
    fn new(body: Bytes) -> Self {
        Self { next: Some(body) }
    }
}

impl TransportBody for StaticBody {
    fn next_chunk<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Bytes>, TransportError>> + Send + 'a>> {
        Box::pin(async move { Ok(self.next.take()) })
    }
}

struct RecordingChunkStream {
    events: Arc<StdMutex<Vec<String>>>,
    chunk: Option<Bytes>,
}

impl RecordingChunkStream {
    fn new(events: Arc<StdMutex<Vec<String>>>, chunk: Bytes) -> Self {
        Self {
            events,
            chunk: Some(chunk),
        }
    }
}

impl futures_core::Stream for RecordingChunkStream {
    type Item = Result<Bytes, concord_core::advanced::StreamBodyError>;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.events
            .lock()
            .expect("stream events lock")
            .push("stream_poll".to_string());
        Poll::Ready(self.chunk.take().map(Ok))
    }
}

struct MultiChunkStream {
    chunks: VecDeque<Bytes>,
}

impl MultiChunkStream {
    fn new(chunks: Vec<Bytes>) -> Self {
        Self {
            chunks: chunks.into(),
        }
    }
}

impl futures_core::Stream for MultiChunkStream {
    type Item = Result<Bytes, concord_core::advanced::StreamBodyError>;

    fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(self.get_mut().chunks.pop_front().map(Ok))
    }
}

struct PollFlagStream {
    polled: Arc<AtomicBool>,
    chunk: Option<Bytes>,
}

impl PollFlagStream {
    fn new(polled: Arc<AtomicBool>, chunk: Bytes) -> Self {
        Self {
            polled,
            chunk: Some(chunk),
        }
    }
}

impl futures_core::Stream for PollFlagStream {
    type Item = Result<Bytes, concord_core::advanced::StreamBodyError>;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.polled.store(true, Ordering::SeqCst);
        Poll::Ready(self.chunk.take().map(Ok))
    }
}

async fn collect_stream(
    mut stream: concord_core::advanced::TransportByteStream,
    events: &Arc<StdMutex<Vec<String>>>,
) -> Result<Bytes, TransportError> {
    let mut out = Vec::new();
    loop {
        let next = std::future::poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)).await;
        match next {
            Some(Ok(chunk)) => {
                events
                    .lock()
                    .expect("stream transport events lock")
                    .push("stream_poll".to_string());
                out.extend_from_slice(&chunk);
            }
            Some(Err(error)) => return Err(error),
            None => break,
        }
    }
    Ok(Bytes::from(out))
}

fn push_event(events: &Arc<StdMutex<Vec<String>>>, event: impl Into<String>) {
    events.lock().expect("event log lock").push(event.into());
}

fn stream_request_plan(
    name: &'static str,
    method: Method,
    path: &'static str,
    policy: ResolvedPolicy,
    body: StreamBody,
    content_type: HeaderValue,
) -> RequestPlan {
    RequestPlan {
        endpoint: EndpointPlan {
            meta: EndpointMeta {
                name,
                method,
                idempotent: false,
                facade_path: &[],
            },
            route: ResolvedRoute::new(http::uri::Scheme::HTTPS, "example.com", path),
            policy,
            body: BodyPlan::RawStream { content_type },
            response: ResponsePlan {
                accept: Some(HeaderValue::from_static("text/plain")),
                no_content: false,
                format: concord_core::internal::Format::Text,
            },
            pagination: None,
        },
        args: RequestArgs::with_stream_body(body),
        overrides: RequestOverrides::default(),
        replayability: concord_core::internal::Replayability::NonReplayable,
    }
}

fn mismatched_request_plan(
    name: &'static str,
    method: Method,
    path: &'static str,
    policy: ResolvedPolicy,
    body: BodyPlan,
    args: RequestArgs,
) -> RequestPlan {
    let replayability = match &body {
        BodyPlan::None | BodyPlan::Encoded { .. } => {
            concord_core::internal::Replayability::Replayable
        }
        BodyPlan::RawStream { .. } | BodyPlan::Multipart { .. } | BodyPlan::Records { .. } => {
            concord_core::internal::Replayability::NonReplayable
        }
    };
    RequestPlan {
        endpoint: EndpointPlan {
            meta: EndpointMeta {
                name,
                method,
                idempotent: false,
                facade_path: &[],
            },
            route: ResolvedRoute::new(http::uri::Scheme::HTTPS, "example.com", path),
            policy,
            body,
            response: ResponsePlan {
                accept: Some(HeaderValue::from_static("text/plain")),
                no_content: false,
                format: concord_core::internal::Format::Text,
            },
            pagination: None,
        },
        args,
        overrides: RequestOverrides::default(),
        replayability,
    }
}

fn stream_retry_policy() -> ResolvedPolicy {
    ResolvedPolicy {
        retry: concord_core::internal::RetrySetting::Config(concord_core::advanced::RetryConfig {
            max_attempts: 2,
            methods: Vec::new(),
            statuses: Vec::new(),
            transport_errors: vec![TransportErrorKind::Other],
            backoff: concord_core::advanced::RetryBackoff::None,
            respect_retry_after: false,
            idempotency: concord_core::advanced::RetryIdempotency::SafeMethodsOnly,
        }),
        ..Default::default()
    }
}

#[tokio::test]
async fn raw_stream_request_reaches_transport_and_decodes_response() -> Result<(), ApiClientError> {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport =
        StreamTransport::success(events.clone(), MockResponse::text(StatusCode::OK, "ok"));
    let mut client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
    client.set_debug_sink(Arc::new(RecordingDebugSink::new(events.clone())));
    client.set_runtime_hooks(Arc::new(RecordingHooks::new(events.clone())));
    client.configure(|cfg| {
        cfg.rate_limiter(Arc::new(RecordingRateLimiter::new(events.clone())));
        cfg.debug(DebugLevel::VV);
    });

    let sentinel = Bytes::from_static(b"SECRET_STREAM_REQUEST_SENTINEL_MUST_NOT_APPEAR");
    push_event(&events, "build");
    let decoded = client
        .execute_plan::<concord_core::prelude::Text<String>>(stream_request_plan(
            "RawStreamSuccess",
            Method::POST,
            "/raw-stream",
            ResolvedPolicy::default(),
            StreamBody::from_bytes(sentinel.clone()),
            HeaderValue::from_static("application/octet-stream"),
        ))
        .await?;

    assert_eq!(decoded.into_value(), "ok");
    assert_eq!(transport.send_count(), 1);
    let captured = transport.captured();
    assert_eq!(captured.len(), 1);
    assert_eq!(
        captured[0].content_type.as_deref(),
        Some("application/octet-stream")
    );
    match &captured[0].body {
        CapturedBody::Stream(bytes) => assert_eq!(bytes, &sentinel),
        other => panic!("expected stream body, got {other:?}"),
    }
    assert!(
        !captured[0]
            .debug
            .contains("SECRET_STREAM_REQUEST_SENTINEL_MUST_NOT_APPEAR")
    );
    let events = events.lock().expect("event log lock").clone();
    assert!(events.iter().any(|event| event == "rate_limit_acquire"));
    assert!(
        events
            .iter()
            .any(|event| event == "hook_pre_send:RawStreamSuccess")
    );
    assert!(
        events
            .iter()
            .any(|event| event == "hook_post_response:RawStreamSuccess")
    );
    assert!(events.iter().any(|event| event.contains("debug_request:")));
    assert!(events.iter().any(|event| event.contains("debug_response:")));
    assert!(
        !events
            .iter()
            .any(|event| event.contains("SECRET_STREAM_REQUEST_SENTINEL_MUST_NOT_APPEAR"))
    );
    Ok(())
}

#[tokio::test]
async fn stream_request_debug_and_observation_surfaces_are_body_free() {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport = StreamTransport::transport_error(
        events.clone(),
        MockResponse::text(StatusCode::OK, "ok"),
        TransportErrorKind::Other,
    );
    let mut client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
    client.set_debug_sink(Arc::new(RecordingDebugSink::new(events.clone())));
    client.set_runtime_hooks(Arc::new(RecordingHooks::new(events.clone())));
    client.configure(|cfg| {
        cfg.rate_limiter(Arc::new(RecordingRateLimiter::new(events.clone())));
        cfg.debug(DebugLevel::VV);
    });

    let sentinel = Bytes::from_static(b"SECRET_STREAM_REQUEST_SENTINEL_MUST_NOT_APPEAR");
    let err = client
        .execute_plan::<concord_core::prelude::Text<String>>(stream_request_plan(
            "RawStreamError",
            Method::POST,
            "/raw-stream-error",
            ResolvedPolicy::default(),
            StreamBody::from_bytes(sentinel.clone()),
            HeaderValue::from_static("application/octet-stream"),
        ))
        .await
        .expect_err("transport error should surface");

    assert_eq!(transport.send_count(), 1);
    let captured = transport.captured();
    assert_eq!(captured.len(), 1);
    assert!(
        !captured[0]
            .debug
            .contains("SECRET_STREAM_REQUEST_SENTINEL_MUST_NOT_APPEAR")
    );
    match &captured[0].body {
        CapturedBody::Stream(bytes) => assert_eq!(bytes, &sentinel),
        other => panic!("expected stream body, got {other:?}"),
    }
    assert!(!format!("{err:?}").contains("SECRET_STREAM_REQUEST_SENTINEL_MUST_NOT_APPEAR"));
    assert!(!format!("{err}").contains("SECRET_STREAM_REQUEST_SENTINEL_MUST_NOT_APPEAR"));
    let events = events.lock().expect("event log lock").clone();
    assert!(
        !events
            .iter()
            .any(|event| event.contains("SECRET_STREAM_REQUEST_SENTINEL_MUST_NOT_APPEAR"))
    );
}

#[tokio::test]
async fn stream_is_not_polled_before_auth_collision_validation() {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let polled = Arc::new(AtomicBool::new(false));
    let transport =
        StreamTransport::success(events.clone(), MockResponse::text(StatusCode::OK, "ok"));
    let client = ApiClient::<TestCx, _>::with_transport(
        (),
        TestAuthVars {
            token: Some("token".to_string()),
            identity: "anon",
        },
        transport.clone(),
    );
    let mut policy = auth_policy(AuthPlacement::Bearer);
    policy.headers.insert(
        http::header::AUTHORIZATION,
        HeaderValue::from_static("public"),
    );

    let err = client
        .execute_plan::<concord_core::prelude::Text<String>>(mismatched_request_plan(
            "RawStreamCollision",
            Method::POST,
            "/raw-stream-collision",
            policy,
            BodyPlan::RawStream {
                content_type: HeaderValue::from_static("application/octet-stream"),
            },
            RequestArgs::with_stream_body(StreamBody::from_byte_stream(PollFlagStream::new(
                polled.clone(),
                Bytes::from_static(b"chunk"),
            ))),
        ))
        .await
        .expect_err("auth collision should fail before transport");

    assert!(matches!(err, ApiClientError::Auth { .. }));
    assert_eq!(transport.send_count(), 0);
    assert!(!polled.load(Ordering::SeqCst));
}

#[tokio::test]
async fn stream_is_not_polled_before_rate_limit_acquisition() -> Result<(), ApiClientError> {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport =
        StreamTransport::success(events.clone(), MockResponse::text(StatusCode::OK, "ok"));
    let mut client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
    client.configure(|cfg| {
        cfg.rate_limiter(Arc::new(RecordingRateLimiter::new(events.clone())));
        cfg.debug(DebugLevel::VV);
    });
    push_event(&events, "build");

    let decoded = client
        .execute_plan::<concord_core::prelude::Text<String>>(stream_request_plan(
            "RawStreamOrdering",
            Method::POST,
            "/raw-stream-ordering",
            ResolvedPolicy::default(),
            StreamBody::from_byte_stream(RecordingChunkStream::new(
                events.clone(),
                Bytes::from_static(b"chunk"),
            )),
            HeaderValue::from_static("application/octet-stream"),
        ))
        .await?;

    assert_eq!(decoded.into_value(), "ok");
    let events = events.lock().expect("event log lock").clone();
    let build = events.iter().position(|event| event == "build").unwrap();
    let rate_limit = events
        .iter()
        .position(|event| event == "rate_limit_acquire")
        .unwrap();
    let transport = events
        .iter()
        .position(|event| event == "transport")
        .unwrap();
    let stream_poll = events
        .iter()
        .position(|event| event == "stream_poll")
        .unwrap();
    assert!(build < rate_limit);
    assert!(rate_limit < transport);
    assert!(transport < stream_poll);
    Ok(())
}

#[tokio::test]
async fn stream_request_is_not_retried_on_transport_error() {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport = StreamTransport::transport_error(
        events.clone(),
        MockResponse::text(StatusCode::OK, "ok"),
        TransportErrorKind::Other,
    );
    let client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());

    let err = client
        .execute_plan::<concord_core::prelude::Text<String>>(stream_request_plan(
            "RawStreamTransportError",
            Method::POST,
            "/raw-stream-transport-error",
            stream_retry_policy(),
            StreamBody::from_bytes(Bytes::from_static(
                b"SECRET_STREAM_REQUEST_SENTINEL_MUST_NOT_APPEAR",
            )),
            HeaderValue::from_static("application/octet-stream"),
        ))
        .await
        .expect_err("transport error should be terminal for stream bodies");

    assert_eq!(transport.send_count(), 1);
    assert!(!format!("{err:?}").contains("SECRET_STREAM_REQUEST_SENTINEL_MUST_NOT_APPEAR"));
    assert!(!format!("{err}").contains("SECRET_STREAM_REQUEST_SENTINEL_MUST_NOT_APPEAR"));
}

#[tokio::test]
async fn stream_request_is_not_retried_after_auth_rejection() {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport = StreamTransport::success(
        events.clone(),
        MockResponse::text(StatusCode::UNAUTHORIZED, "nope"),
    );
    let client = ApiClient::<TestCx, _>::with_transport(
        (),
        TestAuthVars {
            token: Some("token".to_string()),
            identity: "refresh",
        },
        transport.clone(),
    );
    let mut policy = auth_policy(AuthPlacement::Bearer);
    policy.retry = concord_core::internal::RetrySetting::Off;

    let err = client
        .execute_plan::<concord_core::prelude::Text<String>>(stream_request_plan(
            "RawStreamAuthRejection",
            Method::POST,
            "/raw-stream-auth-rejection",
            policy,
            StreamBody::from_bytes(Bytes::from_static(b"secret-body")),
            HeaderValue::from_static("application/octet-stream"),
        ))
        .await
        .expect_err("auth rejection should not retry stream bodies");

    assert_eq!(transport.send_count(), 1);
    assert!(matches!(err, ApiClientError::Auth { .. }));
}

#[tokio::test]
async fn replayable_stream_body_plan_is_rejected_defensively() {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport =
        StreamTransport::success(events.clone(), MockResponse::text(StatusCode::OK, "ok"));
    let client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());

    let err = client
        .execute_plan::<concord_core::prelude::Text<String>>(RequestPlan {
            endpoint: EndpointPlan {
                meta: EndpointMeta {
                    name: "ReplayableStreamBody",
                    method: Method::POST,
                    idempotent: false,
                    facade_path: &[],
                },
                route: ResolvedRoute::new(
                    http::uri::Scheme::HTTPS,
                    "example.com",
                    "/replayable-stream-body",
                ),
                policy: ResolvedPolicy::default(),
                body: BodyPlan::RawStream {
                    content_type: HeaderValue::from_static("application/octet-stream"),
                },
                response: ResponsePlan {
                    accept: Some(HeaderValue::from_static("text/plain")),
                    no_content: false,
                    format: concord_core::internal::Format::Text,
                },
                pagination: None,
            },
            args: RequestArgs::with_stream_body(StreamBody::from_bytes(Bytes::from_static(
                b"replayable-stream",
            ))),
            overrides: RequestOverrides::default(),
            replayability: concord_core::internal::Replayability::Replayable,
        })
        .await
        .expect_err("replayable stream body plan should be rejected defensively");

    assert!(matches!(err, ApiClientError::PolicyViolation { .. }));
    assert_eq!(transport.send_count(), 0);
}

#[tokio::test]
async fn stream_request_size_hint_exceeds_limit_before_transport() {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let polled = Arc::new(AtomicBool::new(false));
    let transport =
        StreamTransport::success(events.clone(), MockResponse::text(StatusCode::OK, "ok"));
    let mut client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
    client.configure(|cfg| {
        cfg.max_stream_request_body_bytes(4);
        cfg.rate_limiter(Arc::new(RecordingRateLimiter::new(events.clone())));
        cfg.debug(DebugLevel::VV);
    });
    client.set_runtime_hooks(Arc::new(RecordingHooks::new(events.clone())));

    let err = client
        .execute_plan::<concord_core::prelude::Text<String>>(stream_request_plan(
            "RawStreamPreflightLimit",
            Method::POST,
            "/raw-stream-preflight-limit",
            ResolvedPolicy::default(),
            StreamBody::from_byte_stream(PollFlagStream::new(
                polled.clone(),
                Bytes::from_static(b"chunk"),
            ))
            .with_size_hint(BodySizeHint::exact(5)),
            HeaderValue::from_static("application/octet-stream"),
        ))
        .await
        .expect_err("size hint limit should fail before transport");

    assert_eq!(transport.send_count(), 0);
    assert!(!polled.load(Ordering::SeqCst));
    assert!(matches!(
        err,
        ApiClientError::RequestBodyLimitExceeded { .. }
    ));
    assert!(
        err.to_string()
            .contains("stream request body exceeded configured size limit")
    );
    let events = events.lock().expect("event log lock").clone();
    assert!(events.iter().any(|event| event == "rate_limit_acquire"));
    assert!(
        !events
            .iter()
            .any(|event| event == "hook_pre_send:RawStreamPreflightLimit")
    );
    assert!(!events.iter().any(|event| event == "transport"));
    assert!(!events.iter().any(|event| event == "stream_poll"));
    assert!(!format!("{err:?}").contains("SECRET_STREAM_REQUEST_SENTINEL_MUST_NOT_APPEAR"));
}

#[tokio::test]
async fn stream_request_is_counted_while_transport_consumes_it() {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport =
        StreamTransport::success(events.clone(), MockResponse::text(StatusCode::OK, "ok"));
    let mut client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
    client.configure(|cfg| {
        cfg.max_stream_request_body_bytes(5);
        cfg.rate_limiter(Arc::new(RecordingRateLimiter::new(events.clone())));
    });
    client.set_runtime_hooks(Arc::new(RecordingHooks::new(events.clone())));

    let err = client
        .execute_plan::<concord_core::prelude::Text<String>>(stream_request_plan(
            "RawStreamConsumeLimit",
            Method::POST,
            "/raw-stream-consume-limit",
            ResolvedPolicy::default(),
            StreamBody::from_byte_stream(MultiChunkStream::new(vec![
                Bytes::from_static(b"abcd"),
                Bytes::from_static(b"efgh"),
            ]))
            .with_size_hint(BodySizeHint::unknown()),
            HeaderValue::from_static("application/octet-stream"),
        ))
        .await;

    let err = err.expect_err("request body limit should fail while transport consumes");
    assert_eq!(transport.send_count(), 1);
    assert!(matches!(
        err,
        ApiClientError::RequestBodyLimitExceeded {
            limit: 5,
            actual: 8,
            ..
        }
    ));
    assert!(!format!("{err:?}").contains("SECRET_STREAM_REQUEST_SENTINEL_MUST_NOT_APPEAR"));
    let events = events.lock().expect("event log lock").clone();
    assert_eq!(
        events
            .iter()
            .position(|event| event == "rate_limit_acquire")
            .expect("rate limit acquire event"),
        0
    );
    assert_eq!(
        events
            .iter()
            .position(|event| event == "hook_pre_send:RawStreamConsumeLimit")
            .expect("pre-send event"),
        1
    );
    assert_eq!(
        events
            .iter()
            .position(|event| event == "transport")
            .expect("transport event"),
        2
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.as_str() == "stream_poll")
            .count(),
        1
    );
}

#[tokio::test]
async fn mismatched_body_plan_and_request_args_are_rejected() {
    let cases = vec![
        (
            "RawStreamMissingBody",
            BodyPlan::RawStream {
                content_type: HeaderValue::from_static("application/octet-stream"),
            },
            RequestArgs::empty(),
            "raw stream body plan requires a stream request body",
        ),
        (
            "EncodedWithStreamBody",
            BodyPlan::Encoded {
                content_type: Some(HeaderValue::from_static("application/json")),
                format: concord_core::internal::Format::Text,
            },
            RequestArgs::with_stream_body(StreamBody::from_bytes(Bytes::from_static(b"chunk"))),
            "encoded request body plan requires buffered bytes",
        ),
        (
            "NoneWithStreamBody",
            BodyPlan::None,
            RequestArgs::with_stream_body(StreamBody::from_bytes(Bytes::from_static(b"chunk"))),
            "request body is not allowed for this endpoint",
        ),
    ];

    for (name, body, args, expected) in cases {
        let events = Arc::new(StdMutex::new(Vec::new()));
        let transport = StreamTransport::success(events, MockResponse::text(StatusCode::OK, "ok"));
        let client =
            ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
        let err = client
            .execute_plan::<concord_core::prelude::Text<String>>(mismatched_request_plan(
                name,
                Method::POST,
                "/mismatch",
                ResolvedPolicy::default(),
                body,
                args,
            ))
            .await
            .expect_err("body plan mismatch should fail");

        assert_eq!(transport.send_count(), 0);
        assert!(matches!(err, ApiClientError::PolicyViolation { .. }));
        assert!(err.to_string().contains(expected), "{err}");
    }
}

use super::common::{MockResponse, TestAuthVars, TestCx, auth_policy};
use bytes::Bytes;
use concord_core::advanced::{
    AuthPlacement, DebugSink, ErrorContext, MultipartBody, MultipartRequest,
    PostResponseHookContext, PreSendHookContext, RateLimitContext, RateLimitFuture,
    RateLimitPermit, RateLimitResponseAction, RateLimitResponseContext, RateLimiter, RequestEntity,
    RuntimeHooks, StreamBody, Transport, TransportBody, TransportError, TransportErrorKind,
    TransportRequest, TransportRequestBody, TransportResponse,
};
use concord_core::internal::{
    EndpointMeta, EndpointPlan, RequestOverrides, RequestPlan, ResolvedPolicy, ResolvedRoute,
    ResponsePlan,
};
use concord_core::prelude::{ApiClient, ApiClientError, DebugLevel};
use futures_core::Stream;
use http::{HeaderValue, Method, StatusCode};
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

    fn request_headers(
        &self,
        dbg: concord_core::prelude::DebugLevel,
        _headers: concord_core::advanced::SanitizedHeaders<'_>,
    ) {
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

    fn response_headers(
        &self,
        dbg: concord_core::prelude::DebugLevel,
        _headers: concord_core::advanced::SanitizedHeaders<'_>,
    ) {
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
struct MultipartTransport {
    events: Arc<StdMutex<Vec<String>>>,
    captured: Arc<StdMutex<Vec<CapturedRequest>>>,
    response: MockResponse,
    transport_error: Option<TransportErrorKind>,
    send_count: Arc<AtomicUsize>,
}

impl MultipartTransport {
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
        self.captured.lock().expect("captured lock").clone()
    }
}

impl Transport for MultipartTransport {
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
            let TransportRequest {
                meta,
                url,
                headers,
                body,
                timeout: _timeout,
                rate_limit,
                extensions: _extensions,
            } = req;
            events
                .lock()
                .expect("multipart transport events lock")
                .push("transport".to_string());
            events
                .lock()
                .expect("multipart transport events lock")
                .push(format!("transport_debug:{debug}"));
            let content_type = headers
                .get(http::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string);
            let body = match body {
                TransportRequestBody::Empty => CapturedBody::Empty,
                TransportRequestBody::Bytes(bytes) => CapturedBody::Bytes(bytes),
                TransportRequestBody::Stream(stream) => {
                    CapturedBody::Stream(collect_stream(stream, &events).await?)
                }
            };
            captured
                .lock()
                .expect("captured lock")
                .push(CapturedRequest {
                    debug,
                    content_type,
                    body,
                });

            if let Some(kind) = transport_error {
                return Err(TransportError::with_kind(
                    kind,
                    std::io::Error::other("multipart transport failure"),
                ));
            }

            Ok(TransportResponse {
                meta,
                url,
                status: response.status,
                headers: response.headers,
                content_length: response.content_length,
                rate_limit,
                body: Box::new(StaticBody::new(response.body)),
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

struct RecordingChunkStream {
    events: Arc<StdMutex<Vec<String>>>,
    chunks: VecDeque<Bytes>,
}

impl RecordingChunkStream {
    fn new(events: Arc<StdMutex<Vec<String>>>, chunks: Vec<Bytes>) -> Self {
        Self {
            events,
            chunks: chunks.into(),
        }
    }
}

impl futures_core::Stream for RecordingChunkStream {
    type Item = Result<Bytes, concord_core::advanced::StreamBodyError>;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.events
            .lock()
            .expect("stream events lock")
            .push("multipart_part_poll".to_string());
        Poll::Ready(self.chunks.pop_front().map(Ok))
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
                    .expect("multipart transport events lock")
                    .push("multipart_part_poll".to_string());
                out.extend_from_slice(&chunk);
            }
            Some(Err(error)) => return Err(error),
            None => break,
        }
    }
    Ok(Bytes::from(out))
}

fn multipart_request_plan(
    name: &'static str,
    method: Method,
    path: &'static str,
    idempotent: bool,
    policy: ResolvedPolicy,
    body: MultipartBody,
) -> RequestPlan {
    let body = MultipartRequest::prepare(
        body,
        ErrorContext {
            endpoint: name,
            method: method.clone(),
        },
    )
    .expect("multipart body")
    .body;
    RequestPlan {
        endpoint: EndpointPlan {
            meta: EndpointMeta {
                name,
                method,
                idempotent,
                facade_path: &[],
            },
            route: ResolvedRoute::new(http::uri::Scheme::HTTPS, "example.com", path),
            policy,
            response: ResponsePlan {
                accept: Some(HeaderValue::from_static("text/plain")),
                no_content: false,
                format: concord_core::internal::Format::Text,
            },
            pagination: None,
        },
        body,
        overrides: RequestOverrides::default(),
    }
}

#[tokio::test]
async fn multipart_form_data_request_reaches_transport_and_is_body_free_in_debug()
-> Result<(), ApiClientError> {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport =
        MultipartTransport::success(events.clone(), MockResponse::text(StatusCode::OK, "ok"));
    let mut client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
    client.set_debug_sink(Arc::new(RecordingDebugSink::new(events.clone())));
    client.set_runtime_hooks(Arc::new(RecordingHooks::new(events.clone())));
    client.configure(|cfg| {
        cfg.rate_limiter(Arc::new(RecordingRateLimiter::new(events.clone())));
        cfg.debug(DebugLevel::VV);
    });

    let body = MultipartBody::new()
        .text("title", "hello")
        .bytes("file", Bytes::from_static(b"abc"));
    let rendered_body = format!("{body:?}");
    assert!(!rendered_body.contains("hello"));
    assert!(!rendered_body.contains("abc"));

    let decoded = client
        .execute_plan::<concord_core::prelude::Text<String>>(multipart_request_plan(
            "MultipartFormData",
            Method::POST,
            "/multipart-form-data",
            false,
            ResolvedPolicy::default(),
            body,
        ))
        .await?;

    assert_eq!(decoded.into_value(), "ok");
    assert_eq!(transport.send_count(), 1);
    let captured = transport.captured();
    assert_eq!(captured.len(), 1);
    let content_type = captured[0].content_type.as_deref().expect("content type");
    assert!(content_type.starts_with("multipart/form-data; boundary="));
    let boundary = captured[0]
        .content_type
        .as_deref()
        .and_then(|value| value.split("; boundary=").nth(1))
        .expect("multipart boundary");
    match &captured[0].body {
        CapturedBody::Stream(bytes) => {
            let rendered = String::from_utf8(bytes.clone().to_vec()).expect("multipart body");
            assert!(rendered.contains(&format!("--{boundary}\r\n")));
            assert!(rendered.contains("Content-Disposition: form-data; name=\"title\""));
            assert!(rendered.contains("Content-Disposition: form-data; name=\"file\""));
            assert!(rendered.contains("abc"));
            assert!(rendered.contains("\r\n"));
            assert!(rendered.ends_with(&format!("--{boundary}--\r\n")));
        }
        other => panic!("expected stream body, got {other:?}"),
    }
    assert!(
        !captured[0]
            .debug
            .contains("SECRET_MULTIPART_SENTINEL_MUST_NOT_APPEAR")
    );
    let events = events.lock().expect("event log lock").clone();
    assert!(events.iter().any(|event| event == "rate_limit_acquire"));
    assert!(events.iter().any(|event| event == "transport"));
    assert!(events.iter().any(|event| event == "rate_limit_response"));
    assert!(
        !events
            .iter()
            .any(|event| event.contains("SECRET_MULTIPART_SENTINEL_MUST_NOT_APPEAR"))
    );
    Ok(())
}

#[tokio::test]
async fn multipart_stream_part_is_not_polled_before_auth_collision_validation() {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let polled = Arc::new(AtomicBool::new(false));
    let transport =
        MultipartTransport::success(events.clone(), MockResponse::text(StatusCode::OK, "ok"));
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

    let body = MultipartBody::new().stream(
        "upload",
        StreamBody::from_byte_stream(PollFlagStream::new(
            polled.clone(),
            Bytes::from_static(b"chunk"),
        )),
    );

    let err = client
        .execute_plan::<concord_core::prelude::Text<String>>(multipart_request_plan(
            "MultipartAuthCollision",
            Method::POST,
            "/multipart-auth-collision",
            false,
            policy,
            body,
        ))
        .await
        .expect_err("auth collision should fail before transport");

    assert!(matches!(err, ApiClientError::Auth { .. }));
    assert_eq!(transport.send_count(), 0);
    assert!(!polled.load(Ordering::SeqCst));
}

#[tokio::test]
async fn multipart_stream_part_is_not_polled_before_rate_limit_acquisition()
-> Result<(), ApiClientError> {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport =
        MultipartTransport::success(events.clone(), MockResponse::text(StatusCode::OK, "ok"));
    let mut client = ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport);
    client.configure(|cfg| {
        cfg.rate_limiter(Arc::new(RecordingRateLimiter::new(events.clone())));
        cfg.debug(DebugLevel::VV);
    });
    client.set_runtime_hooks(Arc::new(RecordingHooks::new(events.clone())));

    let body = MultipartBody::new().stream(
        "upload",
        StreamBody::from_byte_stream(RecordingChunkStream::new(
            events.clone(),
            vec![Bytes::from_static(b"chunk")],
        )),
    );

    let _ = client
        .execute_plan::<concord_core::prelude::Text<String>>(multipart_request_plan(
            "MultipartOrdering",
            Method::POST,
            "/multipart-ordering",
            false,
            ResolvedPolicy::default(),
            body,
        ))
        .await?;

    let events = events.lock().expect("event log lock").clone();
    let rate_limit = events
        .iter()
        .position(|event| event == "rate_limit_acquire")
        .expect("rate limit acquisition event");
    let transport = events
        .iter()
        .position(|event| event == "transport")
        .expect("transport event");
    let multipart_part_poll = events
        .iter()
        .position(|event| event == "multipart_part_poll")
        .expect("multipart part poll event");
    assert!(rate_limit < transport);
    assert!(transport < multipart_part_poll);
    Ok(())
}

#[tokio::test]
async fn multipart_request_is_not_retried_or_replayed() {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport = MultipartTransport::transport_error(
        events.clone(),
        MockResponse::text(StatusCode::OK, "ok"),
        TransportErrorKind::Other,
    );
    let client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
    let policy = ResolvedPolicy {
        retry: concord_core::internal::RetrySetting::Config(concord_core::advanced::RetryConfig {
            max_attempts: 2,
            methods: vec![Method::GET],
            statuses: Vec::new(),
            transport_errors: vec![TransportErrorKind::Other],
            backoff: concord_core::advanced::RetryBackoff::None,
            respect_retry_after: false,
            idempotency: concord_core::advanced::RetryIdempotency::SafeMethodsOnly,
        }),
        ..Default::default()
    };

    let err = client
        .execute_plan::<concord_core::prelude::Text<String>>(multipart_request_plan(
            "MultipartNoReplay",
            Method::GET,
            "/multipart-no-replay",
            true,
            policy,
            MultipartBody::new().bytes("file", Bytes::from_static(b"abc")),
        ))
        .await
        .expect_err("multipart bodies must not be retried");

    assert_eq!(transport.send_count(), 1);
    assert!(!format!("{err:?}").contains("abc"));
    assert!(!format!("{err}").contains("abc"));
}

#[tokio::test]
async fn multipart_request_stream_limit_applies() {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport =
        MultipartTransport::success(events.clone(), MockResponse::text(StatusCode::OK, "ok"));
    let mut client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
    client.configure(|cfg| {
        cfg.max_stream_request_body_bytes(1);
        cfg.rate_limiter(Arc::new(RecordingRateLimiter::new(events.clone())));
    });
    client.set_runtime_hooks(Arc::new(RecordingHooks::new(events.clone())));

    let body = MultipartBody::new().text("title", "hello").bytes(
        "file",
        Bytes::from_static(b"SECRET_MULTIPART_SENTINEL_MUST_NOT_APPEAR"),
    );

    let err = client
        .execute_plan::<concord_core::prelude::Text<String>>(multipart_request_plan(
            "MultipartRequestLimit",
            Method::POST,
            "/multipart-request-limit",
            false,
            ResolvedPolicy::default(),
            body,
        ))
        .await
        .expect_err("multipart request should exceed limit");

    assert!(matches!(
        err,
        ApiClientError::RequestBodyLimitExceeded { limit: 1, .. }
    ));
    assert!(!format!("{err:?}").contains("SECRET_MULTIPART_SENTINEL_MUST_NOT_APPEAR"));
    assert!(!format!("{err}").contains("SECRET_MULTIPART_SENTINEL_MUST_NOT_APPEAR"));
    assert_eq!(transport.send_count(), 1);
}

#[tokio::test]
async fn multipart_invalid_part_metadata_is_rejected_body_safely() {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport =
        MultipartTransport::success(events.clone(), MockResponse::text(StatusCode::OK, "ok"));
    let client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
    let polled = Arc::new(AtomicBool::new(false));
    let body = MultipartBody::new().stream(
        "bad\r\nname",
        StreamBody::from_byte_stream(PollFlagStream::new(
            polled.clone(),
            Bytes::from_static(b"chunk"),
        )),
    );

    let err = MultipartRequest::prepare(
        body,
        ErrorContext {
            endpoint: "InvalidMultipart",
            method: Method::POST,
        },
    )
    .expect_err("invalid metadata should fail before transport");
    match &err {
        ApiClientError::Codec { source, .. } => assert_eq!(
            source
                .downcast_ref::<concord_core::advanced::MultipartBodyError>()
                .expect("multipart source")
                .kind(),
            concord_core::advanced::MultipartBodyErrorKind::InvalidPartName
        ),
        other => panic!("expected multipart codec error, got {other:?}"),
    }
    assert!(!err.to_string().contains("bad\r\nname"));
    assert!(!polled.load(Ordering::SeqCst));
    let _ = client;
}

#[tokio::test]
async fn multipart_content_type_conflict_is_rejected_before_body_polling_and_transport() {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport = MultipartTransport::success(events, MockResponse::text(StatusCode::OK, "ok"));
    let client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
    let polled = Arc::new(AtomicBool::new(false));
    let body = MultipartBody::new().stream(
        "file",
        StreamBody::from_byte_stream(PollFlagStream::new(
            polled.clone(),
            Bytes::from_static(b"multipart-secret"),
        )),
    );
    let mut policy = ResolvedPolicy::default();
    policy.headers.insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static("multipart/form-data; boundary=wrong-boundary"),
    );
    let err = client
        .execute_plan::<concord_core::prelude::Text<String>>(multipart_request_plan(
            "MultipartMediaConflict",
            Method::POST,
            "/multipart-media-conflict",
            false,
            policy,
            body,
        ))
        .await
        .expect_err("multipart media conflict");
    assert!(err.to_string().contains("Content-Type conflicts"));
    assert!(!polled.load(Ordering::SeqCst));
    assert_eq!(transport.send_count(), 0);
}

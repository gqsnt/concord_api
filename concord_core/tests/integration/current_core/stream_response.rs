use super::common::{TestAuthVars, TestCx, auth_policy, decode_string, retry_policy_for_statuses};
use bytes::Bytes;
use concord_core::advanced::{
    AuthPlacement, DebugSink, OctetStream, PostResponseHookContext, PreSendHookContext,
    RateLimitContext, RateLimitFuture, RateLimitPermit, RateLimitResponseAction,
    RateLimitResponseContext, RateLimiter, RuntimeHooks, Transport, TransportBody, TransportError,
    TransportErrorKind, TransportRequest, TransportResponse,
};
use concord_core::internal::{
    BodyPlan, EndpointMeta, EndpointPlan, RequestArgs, RequestOverrides, RequestPlan,
    ResolvedPolicy, ResolvedRoute, ResponsePlan,
};
use concord_core::prelude::{ApiClient, ApiClientError, DebugLevel};
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

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
            .expect("debug lock")
            .push(format!("debug_request:{dbg}:{endpoint}:{page_index}"));
    }

    fn request_headers(&self, dbg: concord_core::prelude::DebugLevel, _headers: &HeaderMap) {
        self.events
            .lock()
            .expect("debug lock")
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
            .expect("debug lock")
            .push(format!("debug_response:{dbg}:{status}:{ok}"));
    }

    fn response_headers(&self, dbg: concord_core::prelude::DebugLevel, _headers: &HeaderMap) {
        self.events
            .lock()
            .expect("debug lock")
            .push(format!("debug_response_headers:{dbg}"));
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
                .expect("hooks lock")
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
                .expect("hooks lock")
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
                .expect("hooks lock")
                .push(format!("hook_transport_error:{}", ctx.meta.endpoint));
        })
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
                .expect("rate limit lock")
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
                .expect("rate limit lock")
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
}

impl ResponseFixture {
    fn octet_stream(status: StatusCode, chunks: Vec<Bytes>) -> Self {
        let content_length = chunks.iter().fold(Some(0u64), |acc, chunk| {
            acc.and_then(|len| len.checked_add(chunk.len() as u64))
        });
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/octet-stream"),
        );
        Self {
            status,
            headers,
            chunks,
            content_length,
            poll_flag: None,
        }
    }

    fn with_flag(mut self, flag: Arc<AtomicBool>) -> Self {
        self.poll_flag = Some(flag);
        self
    }

    fn content_type(mut self, value: Option<HeaderValue>) -> Self {
        self.headers = HeaderMap::new();
        if let Some(value) = value {
            self.headers.insert(http::header::CONTENT_TYPE, value);
        }
        self
    }
}

#[derive(Clone)]
struct StreamTransport {
    events: Arc<StdMutex<Vec<String>>>,
    responses: Arc<StdMutex<VecDeque<ResponseFixture>>>,
    send_count: Arc<AtomicUsize>,
}

impl StreamTransport {
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

impl Transport for StreamTransport {
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
                .push("transport_response".to_string());
            let mut responses = responses.lock().expect("responses lock");
            let response = responses.pop_front().ok_or_else(|| {
                TransportError::with_kind(
                    TransportErrorKind::Other,
                    std::io::Error::other("stream transport exhausted"),
                )
            })?;
            let body = ChunkBody::new(events.clone(), response.chunks, response.poll_flag);
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
}

impl ChunkBody {
    fn new(
        events: Arc<StdMutex<Vec<String>>>,
        chunks: Vec<Bytes>,
        poll_flag: Option<Arc<AtomicBool>>,
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
            if let Some(flag) = poll_flag {
                flag.store(true, Ordering::SeqCst);
            }
            events
                .lock()
                .expect("events lock")
                .push("stream_chunk_poll".to_string());
            Ok(chunk)
        })
    }
}

fn stream_response_plan(
    name: &'static str,
    method: Method,
    path: &'static str,
    policy: ResolvedPolicy,
    body: BodyPlan,
    args: RequestArgs,
    accept: &'static str,
) -> RequestPlan {
    RequestPlan {
        endpoint: EndpointPlan {
            meta: EndpointMeta {
                name,
                method: method.clone(),
                idempotent: matches!(method, Method::GET | Method::HEAD),
                facade_path: &[],
            },
            route: ResolvedRoute::new(http::uri::Scheme::HTTPS, "example.com", path),
            policy,
            body,
            response: ResponsePlan {
                accept: Some(HeaderValue::from_static(accept)),
                no_content: false,
                format: concord_core::internal::Format::Text,
                decode: decode_string,
            },
            pagination: None,
        },
        args,
        overrides: RequestOverrides::default(),
    }
}

fn empty_response_plan(name: &'static str, path: &'static str) -> RequestPlan {
    stream_response_plan(
        name,
        Method::GET,
        path,
        ResolvedPolicy::default(),
        BodyPlan::None,
        RequestArgs::default(),
        "application/octet-stream",
    )
}

#[tokio::test]
async fn raw_stream_response_returns_metadata_and_chunks() -> Result<(), ApiClientError> {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport = StreamTransport::new(
        events.clone(),
        vec![ResponseFixture::octet_stream(
            StatusCode::OK,
            vec![
                Bytes::from_static(b"hello"),
                Bytes::from_static(b" "),
                Bytes::from_static(b"world"),
            ],
        )],
    );
    let mut client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
    client.set_debug_sink(Arc::new(RecordingDebugSink::new(events.clone())));
    client.set_runtime_hooks(Arc::new(RecordingHooks::new(events.clone())));
    client.configure(|cfg| {
        cfg.rate_limiter(Arc::new(RecordingRateLimiter::new(events.clone())));
        cfg.debug(DebugLevel::VV);
    });

    let mut response = client
        .execute_plan_stream::<OctetStream>(empty_response_plan(
            "RawStreamResponse",
            "/raw-stream-response",
        ))
        .await?;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.content_length(), Some(11));
    assert_eq!(
        response
            .headers()
            .get(http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/octet-stream"),
    );
    assert_eq!(response.media_type(), "application/octet-stream");
    assert!(!format!("{response:?}").contains("SECRET_STREAM_RESPONSE_SENTINEL_MUST_NOT_APPEAR"));

    let mut collected = Vec::new();
    while let Some(chunk) = response.next_chunk().await? {
        collected.extend_from_slice(&chunk);
    }
    assert_eq!(collected, b"hello world".to_vec());
    assert!(response.next_chunk().await?.is_none());
    assert_eq!(transport.send_count(), 1);
    Ok(())
}

#[tokio::test]
async fn raw_stream_response_is_not_buffered_before_return_and_order_is_preserved()
-> Result<(), ApiClientError> {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let polled = Arc::new(AtomicBool::new(false));
    let transport = StreamTransport::new(
        events.clone(),
        vec![
            ResponseFixture::octet_stream(StatusCode::OK, vec![Bytes::from_static(b"chunk")])
                .with_flag(polled.clone()),
        ],
    );
    let mut client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
    client.set_debug_sink(Arc::new(RecordingDebugSink::new(events.clone())));
    client.set_runtime_hooks(Arc::new(RecordingHooks::new(events.clone())));
    client.configure(|cfg| {
        cfg.rate_limiter(Arc::new(RecordingRateLimiter::new(events.clone())));
        cfg.debug(DebugLevel::VV);
    });

    let mut response = client
        .execute_plan_stream::<OctetStream>(empty_response_plan(
            "RawStreamOrdering",
            "/raw-stream-ordering",
        ))
        .await?;

    assert!(!polled.load(Ordering::SeqCst));
    let events_before = events.lock().expect("events lock").clone();
    assert!(
        !events_before
            .iter()
            .any(|event| event == "stream_chunk_poll")
    );
    assert_eq!(
        response.next_chunk().await?.as_deref(),
        Some(b"chunk".as_slice())
    );
    assert!(polled.load(Ordering::SeqCst));
    let events = events.lock().expect("events lock").clone();
    let transport_response = events
        .iter()
        .position(|event| event == "transport_response")
        .unwrap();
    let rate_limit_response = events
        .iter()
        .position(|event| event == "rate_limit_response")
        .unwrap();
    let stream_chunk_poll = events
        .iter()
        .position(|event| event == "stream_chunk_poll")
        .unwrap();
    assert!(transport_response < rate_limit_response);
    assert!(rate_limit_response < stream_chunk_poll);
    Ok(())
}

#[tokio::test]
async fn stream_response_content_type_mismatch_is_rejected_before_body_polling() {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let polled = Arc::new(AtomicBool::new(false));
    let transport = StreamTransport::new(
        events,
        vec![
            ResponseFixture::octet_stream(StatusCode::OK, vec![Bytes::from_static(b"ignored")])
                .content_type(Some(HeaderValue::from_static("application/json")))
                .with_flag(polled.clone()),
        ],
    );
    let client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());

    let err = client
        .execute_plan_stream::<OctetStream>(empty_response_plan(
            "RawStreamMismatch",
            "/raw-stream-mismatch",
        ))
        .await
        .expect_err("mismatched content type should fail");

    assert!(matches!(err, ApiClientError::PolicyViolation { .. }));
    assert!(
        err.to_string()
            .contains("stream response content type did not match expected media type")
    );
    assert!(!polled.load(Ordering::SeqCst));
    assert_eq!(transport.send_count(), 1);
}

#[tokio::test]
async fn stream_response_missing_content_type_is_rejected_before_body_polling() {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let polled = Arc::new(AtomicBool::new(false));
    let transport = StreamTransport::new(
        events,
        vec![
            ResponseFixture::octet_stream(StatusCode::OK, vec![Bytes::from_static(b"ignored")])
                .content_type(None)
                .with_flag(polled.clone()),
        ],
    );
    let client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());

    let err = client
        .execute_plan_stream::<OctetStream>(empty_response_plan(
            "RawStreamMissingContentType",
            "/raw-stream-missing-content-type",
        ))
        .await
        .expect_err("missing content type should fail");

    assert!(matches!(err, ApiClientError::PolicyViolation { .. }));
    assert!(
        err.to_string()
            .contains("stream response content type did not match expected media type")
    );
    assert!(!polled.load(Ordering::SeqCst));
    assert_eq!(transport.send_count(), 1);
}

#[tokio::test]
async fn stream_response_pagination_is_rejected_before_transport() {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport = StreamTransport::new(events, vec![]);
    let client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());

    let mut plan = empty_response_plan("RawStreamPagination", "/raw-stream-pagination");
    plan.endpoint.pagination = Some(concord_core::internal::PaginationPlan::Paged {
        page_key: "page".to_string(),
        per_page_key: "per_page".to_string(),
        page: 1,
        per_page: 10,
    });

    let err = client
        .execute_plan_stream::<OctetStream>(plan)
        .await
        .expect_err("paginated stream responses must be rejected");

    assert!(matches!(err, ApiClientError::PolicyViolation { .. }));
    assert!(
        err.to_string()
            .contains("stream responses do not support pagination")
    );
    assert_eq!(transport.send_count(), 0);
}

#[tokio::test]
async fn stream_response_no_content_plan_is_rejected_before_transport() {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport = StreamTransport::new(events, vec![]);
    let client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());

    let mut plan = empty_response_plan("RawStreamNoContent", "/raw-stream-no-content");
    plan.endpoint.response.no_content = true;

    let err = client
        .execute_plan_stream::<OctetStream>(plan)
        .await
        .expect_err("no-content stream responses must be rejected");

    assert!(matches!(err, ApiClientError::PolicyViolation { .. }));
    assert!(
        err.to_string()
            .contains("stream responses cannot use a no-content response plan")
    );
    assert_eq!(transport.send_count(), 0);
}

#[tokio::test]
async fn stream_response_debug_hooks_and_rate_limit_surfaces_are_body_free()
-> Result<(), ApiClientError> {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let sentinel = Bytes::from_static(b"SECRET_STREAM_RESPONSE_SENTINEL_MUST_NOT_APPEAR");
    let transport = StreamTransport::new(
        events.clone(),
        vec![ResponseFixture::octet_stream(
            StatusCode::OK,
            vec![sentinel.clone()],
        )],
    );
    let mut client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
    client.set_debug_sink(Arc::new(RecordingDebugSink::new(events.clone())));
    client.set_runtime_hooks(Arc::new(RecordingHooks::new(events.clone())));
    client.configure(|cfg| {
        cfg.rate_limiter(Arc::new(RecordingRateLimiter::new(events.clone())));
        cfg.debug(DebugLevel::VV);
    });

    let mut response = client
        .execute_plan_stream::<OctetStream>(empty_response_plan(
            "RawStreamRedaction",
            "/raw-stream-redaction",
        ))
        .await?;
    let rendered = format!("{response:?}");
    assert!(!rendered.contains("SECRET_STREAM_RESPONSE_SENTINEL_MUST_NOT_APPEAR"));
    let mut collected = Vec::new();
    while let Some(chunk) = response.next_chunk().await? {
        collected.extend_from_slice(&chunk);
    }
    assert_eq!(collected, sentinel.to_vec());
    let events = events.lock().expect("events lock").clone();
    assert!(events.iter().any(|event| event == "rate_limit_acquire"));
    assert!(events.iter().any(|event| event == "rate_limit_response"));
    assert!(
        events
            .iter()
            .any(|event| event == "hook_pre_send:RawStreamRedaction")
    );
    assert!(
        events
            .iter()
            .any(|event| event == "hook_post_response:RawStreamRedaction")
    );
    assert!(events.iter().any(|event| event.contains("debug_request:")));
    assert!(events.iter().any(|event| event.contains("debug_response:")));
    assert!(
        !events
            .iter()
            .any(|event| event.contains("SECRET_STREAM_RESPONSE_SENTINEL_MUST_NOT_APPEAR"))
    );
    Ok(())
}

#[tokio::test]
async fn stream_response_auth_rejection_is_handled_before_body_exposure()
-> Result<(), ApiClientError> {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let polled = Arc::new(AtomicBool::new(false));
    let transport = StreamTransport::new(
        events,
        vec![
            ResponseFixture::octet_stream(
                StatusCode::UNAUTHORIZED,
                vec![Bytes::from_static(b"challenge")],
            )
            .with_flag(polled.clone()),
            ResponseFixture::octet_stream(StatusCode::OK, vec![Bytes::from_static(b"ok")]),
        ],
    );
    let mut client = ApiClient::<TestCx, _>::with_transport(
        (),
        TestAuthVars {
            token: Some("token".to_string()),
            identity: "refresh",
        },
        transport.clone(),
    );
    client.set_debug_sink(Arc::new(RecordingDebugSink::new(transport.events.clone())));
    client.set_runtime_hooks(Arc::new(RecordingHooks::new(transport.events.clone())));
    client.configure(|cfg| {
        cfg.rate_limiter(Arc::new(RecordingRateLimiter::new(
            transport.events.clone(),
        )));
        cfg.debug(DebugLevel::VV);
    });

    let mut response = client
        .execute_plan_stream::<OctetStream>(stream_response_plan(
            "RawStreamAuth",
            Method::GET,
            "/raw-stream-auth",
            auth_policy(AuthPlacement::Bearer),
            BodyPlan::None,
            RequestArgs::default(),
            "application/octet-stream",
        ))
        .await?;

    assert!(!polled.load(Ordering::SeqCst));
    assert_eq!(transport.send_count(), 2);
    assert_eq!(
        response.next_chunk().await?.as_deref(),
        Some(b"ok".as_slice())
    );
    Ok(())
}

#[tokio::test]
async fn buffered_request_body_stream_response_retries_before_body_exposure()
-> Result<(), ApiClientError> {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let first_polled = Arc::new(AtomicBool::new(false));
    let transport = StreamTransport::new(
        events,
        vec![
            ResponseFixture::octet_stream(
                StatusCode::INTERNAL_SERVER_ERROR,
                vec![Bytes::from_static(b"retry-body")],
            )
            .with_flag(first_polled.clone()),
            ResponseFixture::octet_stream(StatusCode::OK, vec![Bytes::from_static(b"ok")]),
        ],
    );
    let client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
    let retry_header = http::HeaderName::from_static("idempotency-key");
    let mut policy_headers = HeaderMap::new();
    policy_headers.insert(retry_header.clone(), HeaderValue::from_static("stable-key"));
    let policy = ResolvedPolicy {
        headers: policy_headers,
        retry: concord_core::internal::RetrySetting::Config(concord_core::advanced::RetryConfig {
            max_attempts: 2,
            methods: vec![Method::POST],
            statuses: vec![StatusCode::INTERNAL_SERVER_ERROR],
            transport_errors: Vec::new(),
            backoff: concord_core::advanced::RetryBackoff::None,
            respect_retry_after: true,
            idempotency: concord_core::advanced::RetryIdempotency::Header(retry_header),
        }),
        ..Default::default()
    };

    let mut response = client
        .execute_plan_stream::<OctetStream>(stream_response_plan(
            "RawStreamRetry",
            Method::POST,
            "/raw-stream-retry",
            policy,
            BodyPlan::Encoded {
                content_type: Some(HeaderValue::from_static("application/octet-stream")),
                format: concord_core::internal::Format::Text,
            },
            RequestArgs::with_body_bytes(Bytes::from_static(b"buffered-request")),
            "application/octet-stream",
        ))
        .await?;

    assert!(!first_polled.load(Ordering::SeqCst));
    assert_eq!(transport.send_count(), 2);
    assert_eq!(
        response.next_chunk().await?.as_deref(),
        Some(b"ok".as_slice())
    );
    Ok(())
}

#[tokio::test]
async fn stream_request_body_stream_response_does_not_retry_or_replay() -> Result<(), ApiClientError>
{
    let events = Arc::new(StdMutex::new(Vec::new()));
    let first_polled = Arc::new(AtomicBool::new(false));
    let transport = StreamTransport::new(
        events,
        vec![
            ResponseFixture::octet_stream(
                StatusCode::INTERNAL_SERVER_ERROR,
                vec![Bytes::from_static(b"retry-body")],
            )
            .with_flag(first_polled.clone()),
        ],
    );
    let client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());

    let err = client
        .execute_plan_stream::<OctetStream>(stream_response_plan(
            "RawStreamNoReplay",
            Method::POST,
            "/raw-stream-no-replay",
            retry_policy_for_statuses(2, vec![StatusCode::INTERNAL_SERVER_ERROR]),
            BodyPlan::RawStream {
                content_type: HeaderValue::from_static("application/octet-stream"),
            },
            RequestArgs::with_stream_body(concord_core::advanced::StreamBody::from_bytes(
                Bytes::from_static(b"stream-request"),
            )),
            "application/octet-stream",
        ))
        .await
        .expect_err("stream request bodies must not retry");

    assert!(!first_polled.load(Ordering::SeqCst));
    assert_eq!(transport.send_count(), 1);
    assert!(err.to_string().contains("status 500"));
    Ok(())
}

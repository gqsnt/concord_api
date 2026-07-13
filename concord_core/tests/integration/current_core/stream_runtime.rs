use super::common::{MockResponse, TestAuthVars, TestCx, auth_policy};
use bytes::Bytes;
use concord_core::advanced::{
    AuthPlacement, DebugSink, DynBody, PostResponseHookContext, PreSendHookContext,
    RateLimitContext, RateLimitFuture, RateLimitPermit, RateLimitResponseAction,
    RateLimitResponseContext, RateLimiter, RuntimeHooks, StreamBody, Transport, TransportError,
    TransportErrorKind,
};
use concord_core::internal::{
    EndpointMeta, EndpointPlan, PreparedBody, RequestOverrides, RequestPlan, ResolvedPolicy,
    ResolvedRoute, ResponsePlan,
};
use concord_core::prelude::{ApiClient, ApiClientError, DebugLevel};
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
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CapturedRequest {
    debug: String,
    content_type: Option<String>,
    authorization_present: bool,
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
        req: http::Request<DynBody>,
    ) -> Pin<Box<dyn Future<Output = Result<http::Response<DynBody>, TransportError>> + Send>> {
        let events = self.events.clone();
        let captured = self.captured.clone();
        let response = self.response.clone();
        let transport_error = self.transport_error;
        let send_count = self.send_count.clone();
        Box::pin(async move {
            send_count.fetch_add(1, Ordering::SeqCst);
            let debug = "Request { body: <body>, .. }".to_string();
            events
                .lock()
                .expect("stream transport events lock")
                .push("transport".to_string());
            events
                .lock()
                .expect("stream transport events lock")
                .push(format!("transport_debug:{debug}"));
            let content_type = req
                .headers()
                .get(http::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string);
            let authorization_present = req.headers().contains_key(http::header::AUTHORIZATION);
            let bytes = collect_stream(req.into_body(), &events).await?;
            let body = if bytes.is_empty() {
                CapturedBody::Empty
            } else {
                CapturedBody::Bytes(bytes)
            };
            captured
                .lock()
                .expect("captured requests lock")
                .push(CapturedRequest {
                    debug,
                    content_type,
                    authorization_present,
                    body,
                });

            if let Some(kind) = transport_error {
                return Err(TransportError::with_kind(
                    kind,
                    std::io::Error::other("stream transport failure"),
                ));
            }

            let mut result = http::Response::new(DynBody::from_bytes(response.body));
            *result.status_mut() = response.status;
            *result.headers_mut() = response.headers;
            Ok(result)
        })
    }
}

#[derive(Clone)]
struct OrderingAuthVars {
    events: Arc<StdMutex<Vec<String>>>,
}

#[derive(Clone)]
struct OrderingAuthCx;

impl concord_core::prelude::ClientContext for OrderingAuthCx {
    type Vars = ();
    type AuthVars = OrderingAuthVars;
    type AuthState = ();
    const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
    const DOMAIN: &'static str = "example.com";

    fn init_auth_state(_vars: &Self::Vars, _auth: &Self::AuthVars) -> Self::AuthState {}

    fn prepare_auth_requirement<'a>(
        requirement: &'a concord_core::advanced::AuthRequirement,
        request: &'a mut concord_core::advanced::AuthApplicationRequest<'_>,
        _vars: &'a Self::Vars,
        auth: &'a Self::AuthVars,
        _auth_state: &'a Self::AuthState,
        _executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
        _meta: &'a concord_core::advanced::RequestMeta,
    ) -> concord_core::advanced::AuthFuture<
        'a,
        Result<concord_core::advanced::PreparedAuthCredential, concord_core::advanced::AuthError>,
    > {
        Box::pin(async move {
            push_event(&auth.events, "provider");
            let material = concord_core::prelude::ApiKey::new("ordering-secret");
            let application =
                concord_core::advanced::apply_secret_credential(request, requirement, &material)?;
            Ok(concord_core::advanced::PreparedAuthCredential::new(
                concord_core::advanced::AuthAppliedCredential {
                    credential_id: requirement.credential.id.clone(),
                    usage_id: requirement.usage_id.clone(),
                    step_id: requirement.step_id,
                    generation: Some(1),
                    provenance: requirement.provenance.clone(),
                },
                application,
            ))
        })
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
    mut stream: DynBody,
    events: &Arc<StdMutex<Vec<String>>>,
) -> Result<Bytes, TransportError> {
    let mut out = Vec::new();
    loop {
        let next = http_body_util::BodyExt::frame(&mut stream).await;
        match next {
            Some(Ok(frame)) => {
                let Ok(chunk) = frame.into_data() else {
                    continue;
                };
                events
                    .lock()
                    .expect("stream transport events lock")
                    .push("stream_poll".to_string());
                out.extend_from_slice(&chunk);
            }
            Some(Err(error)) => return Err(TransportError::new(error)),
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
            response: ResponsePlan {
                accept: Some(HeaderValue::from_static("text/plain")),
                no_content: false,
                format: concord_core::internal::Format::Text,
            },
            pagination: None,
        },
        body: PreparedBody::from_stream_body(body, Some(content_type)),
        overrides: RequestOverrides::default(),
    }
}

fn stream_retry_policy() -> ResolvedPolicy {
    ResolvedPolicy {
        retry: concord_core::internal::RetrySetting::Config(concord_core::advanced::RetryConfig {
            max_attempts: 2,
            methods: Vec::new(),
            statuses: Vec::new(),
            transport_errors: vec![TransportErrorKind::Other],
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
        CapturedBody::Bytes(bytes) => assert_eq!(bytes, &sentinel),
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
        CapturedBody::Bytes(bytes) => assert_eq!(bytes, &sentinel),
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
        .execute_plan::<concord_core::prelude::Text<String>>(stream_request_plan(
            "RawStreamCollision",
            Method::POST,
            "/raw-stream-collision",
            policy,
            StreamBody::from_byte_stream(PollFlagStream::new(
                polled.clone(),
                Bytes::from_static(b"chunk"),
            )),
            HeaderValue::from_static("application/octet-stream"),
        ))
        .await
        .expect_err("auth collision should fail before transport");

    assert!(matches!(err, ApiClientError::Auth { .. }));
    assert_eq!(transport.send_count(), 0);
    assert!(!polled.load(Ordering::SeqCst));
}

#[tokio::test]
async fn auth_collision_does_not_invoke_replay_factory() {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport = StreamTransport::success(
        events,
        MockResponse::text(StatusCode::OK, "should-not-send"),
    );
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
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let mut plan = stream_request_plan(
        "FactoryCollision",
        Method::POST,
        "/factory-collision",
        policy,
        StreamBody::from_bytes(Bytes::new()),
        HeaderValue::from_static("application/octet-stream"),
    );
    plan.body = PreparedBody::replay_factory(http_body::SizeHint::default(), None, move || {
        observed.fetch_add(1, Ordering::SeqCst);
        Ok(concord_core::advanced::DynBody::empty())
    });

    let error = client
        .execute_plan::<concord_core::prelude::Text<String>>(plan)
        .await
        .expect_err("collision must fail before factory production");

    assert!(matches!(error, ApiClientError::Auth { .. }));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(transport.send_count(), 0);
}

#[tokio::test]
async fn provider_failure_does_not_produce_request_body() {
    let transport = StreamTransport::success(
        Arc::new(StdMutex::new(Vec::new())),
        MockResponse::text(StatusCode::OK, "should-not-send"),
    );
    let client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let mut factory_plan = stream_request_plan(
        "ProviderFailureFactory",
        Method::POST,
        "/provider-failure-factory",
        auth_policy(AuthPlacement::Bearer),
        StreamBody::from_bytes(Bytes::new()),
        HeaderValue::from_static("application/octet-stream"),
    );
    factory_plan.body =
        PreparedBody::replay_factory(http_body::SizeHint::default(), None, move || {
            observed.fetch_add(1, Ordering::SeqCst);
            Ok(concord_core::advanced::DynBody::empty())
        });

    let error = client
        .execute_plan::<concord_core::prelude::Text<String>>(factory_plan)
        .await
        .expect_err("missing credential must fail before body factory");
    assert!(matches!(error, ApiClientError::Auth { .. }));
    assert_eq!(calls.load(Ordering::SeqCst), 0);

    let polled = Arc::new(AtomicBool::new(false));
    let one_shot_plan = stream_request_plan(
        "ProviderFailureOneShot",
        Method::POST,
        "/provider-failure-one-shot",
        auth_policy(AuthPlacement::Bearer),
        StreamBody::from_byte_stream(PollFlagStream::new(
            polled.clone(),
            Bytes::from_static(b"unconsumed"),
        )),
        HeaderValue::from_static("application/octet-stream"),
    );
    let error = client
        .execute_plan::<concord_core::prelude::Text<String>>(one_shot_plan)
        .await
        .expect_err("missing credential must fail before one-shot body");
    assert!(matches!(error, ApiClientError::Auth { .. }));
    assert!(!polled.load(Ordering::SeqCst));
    assert_eq!(transport.send_count(), 0);
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
async fn provider_body_rate_limit_observers_and_transport_follow_approved_order()
-> Result<(), ApiClientError> {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport =
        StreamTransport::success(events.clone(), MockResponse::text(StatusCode::OK, "ok"));
    let mut client = ApiClient::<OrderingAuthCx, _>::with_transport(
        (),
        OrderingAuthVars {
            events: events.clone(),
        },
        transport.clone(),
    );
    client.configure(|cfg| {
        cfg.rate_limiter(Arc::new(RecordingRateLimiter::new(events.clone())));
        cfg.debug(DebugLevel::VV);
    });
    client.set_debug_sink(Arc::new(RecordingDebugSink::new(events.clone())));
    client.set_runtime_hooks(Arc::new(RecordingHooks::new(events.clone())));
    let factory_events = events.clone();
    let mut plan = stream_request_plan(
        "ApprovedAttemptOrder",
        Method::GET,
        "/approved-attempt-order",
        auth_policy(AuthPlacement::Bearer),
        StreamBody::from_bytes(Bytes::new()),
        HeaderValue::from_static("application/octet-stream"),
    );
    plan.body = PreparedBody::replay_factory(
        http_body::SizeHint::default(),
        Some(HeaderValue::from_static("application/octet-stream")),
        move || {
            push_event(&factory_events, "body_factory");
            Ok(concord_core::advanced::DynBody::from_bytes(
                Bytes::from_static(b"attempt-body"),
            ))
        },
    );

    client
        .execute_plan::<concord_core::prelude::Text<String>>(plan)
        .await?;

    let events = events.lock().expect("event log lock").clone();
    let position = |needle: &str| {
        events
            .iter()
            .position(|event| event == needle || event.starts_with(needle))
            .unwrap_or_else(|| panic!("missing event `{needle}` in {events:?}"))
    };
    assert!(position("provider") < position("body_factory"));
    assert!(position("body_factory") < position("rate_limit_acquire"));
    assert!(position("rate_limit_acquire") < position("debug_request:"));
    assert!(position("rate_limit_acquire") < position("hook_pre_send:"));
    assert!(position("debug_request:") < position("transport"));
    assert!(position("hook_pre_send:") < position("transport"));
    assert!(transport.captured()[0].authorization_present);
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
            .with_size_hint(http_body::SizeHint::with_exact(5)),
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
            .with_size_hint(http_body::SizeHint::default()),
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
async fn buffered_bytes_are_not_subject_to_stream_request_limit() -> Result<(), ApiClientError> {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport = StreamTransport::success(events, MockResponse::text(StatusCode::OK, "ok"));
    let mut client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
    client.configure(|cfg| {
        cfg.max_stream_request_body_bytes(4);
    });
    let mut plan = stream_request_plan(
        "BufferedAboveStreamLimit",
        Method::POST,
        "/buffered-above-stream-limit",
        ResolvedPolicy::default(),
        StreamBody::from_bytes(Bytes::new()),
        HeaderValue::from_static("application/octet-stream"),
    );
    plan.body = PreparedBody::reusable_bytes(
        Bytes::from_static(b"buffered-body"),
        Some(HeaderValue::from_static("application/octet-stream")),
    );

    client
        .execute_plan::<concord_core::prelude::Text<String>>(plan)
        .await?;
    assert_eq!(transport.send_count(), 1);
    assert!(matches!(
        &transport.captured()[0].body,
        CapturedBody::Bytes(bytes) if bytes == &Bytes::from_static(b"buffered-body")
    ));
    Ok(())
}

#[tokio::test]
async fn prepared_media_type_conflict_rejects_encoded_and_stream_bodies_before_polling() {
    let conflicting_policy = || {
        let mut policy = ResolvedPolicy::default();
        policy.headers.insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain"),
        );
        policy
    };

    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport =
        StreamTransport::success(events.clone(), MockResponse::text(StatusCode::OK, "ok"));
    let client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
    let mut encoded = stream_request_plan(
        "EncodedMediaConflict",
        Method::POST,
        "/encoded-media-conflict",
        conflicting_policy(),
        StreamBody::from_bytes(Bytes::new()),
        HeaderValue::from_static("application/json"),
    );
    encoded.body = PreparedBody::reusable_bytes(
        Bytes::from_static(b"encoded"),
        Some(HeaderValue::from_static("application/json")),
    );
    let err = client
        .execute_plan::<concord_core::prelude::Text<String>>(encoded)
        .await
        .expect_err("encoded conflict");
    let encoded_rendered = err.to_string();
    assert!(
        encoded_rendered.contains("request Content-Type conflicts with prepared body media type"),
        "{encoded_rendered}"
    );
    assert_eq!(transport.send_count(), 0);

    let polled = Arc::new(AtomicBool::new(false));
    let stream = stream_request_plan(
        "StreamMediaConflict",
        Method::POST,
        "/stream-media-conflict",
        conflicting_policy(),
        StreamBody::from_byte_stream(PollFlagStream::new(
            polled.clone(),
            Bytes::from_static(b"secret-stream"),
        )),
        HeaderValue::from_static("application/octet-stream"),
    );
    let err = client
        .execute_plan::<concord_core::prelude::Text<String>>(stream)
        .await
        .expect_err("stream conflict");
    let stream_rendered = err.to_string();
    assert!(
        stream_rendered.contains("request Content-Type conflicts with prepared body media type"),
        "{stream_rendered}"
    );
    assert!(!polled.load(Ordering::SeqCst));
    assert_eq!(transport.send_count(), 0);
}

#[tokio::test]
async fn matching_explicit_content_type_is_accepted() -> Result<(), ApiClientError> {
    let mut policy = ResolvedPolicy::default();
    policy.headers.insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport = StreamTransport::success(events, MockResponse::text(StatusCode::OK, "ok"));
    let client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
    client
        .execute_plan::<concord_core::prelude::Text<String>>(stream_request_plan(
            "MatchingMediaType",
            Method::POST,
            "/matching-media-type",
            policy,
            StreamBody::from_bytes(Bytes::from_static(b"body")),
            HeaderValue::from_static("application/octet-stream"),
        ))
        .await?;
    assert_eq!(transport.send_count(), 1);
    Ok(())
}

#[tokio::test]
async fn replay_factory_runs_once_per_physical_attempt_while_one_shot_stays_single_use() {
    let retry_policy = || {
        let mut policy = stream_retry_policy();
        if let concord_core::internal::RetrySetting::Config(config) = &mut policy.retry {
            config.methods = vec![Method::PUT];
            config.statuses = vec![StatusCode::INTERNAL_SERVER_ERROR];
            config.transport_errors.clear();
        }
        policy
    };
    let response = MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, "retry");
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport = StreamTransport::success(events, response.clone());
    let client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let mut hint = http_body::SizeHint::new();
    hint.set_exact(12);
    let mut plan = stream_request_plan(
        "ReplayFactoryAttempts",
        Method::PUT,
        "/replay-factory-attempts",
        retry_policy(),
        StreamBody::from_bytes(Bytes::new()),
        HeaderValue::from_static("application/octet-stream"),
    );
    plan.endpoint.meta.idempotent = true;
    plan.body = PreparedBody::replay_factory(
        hint,
        Some(HeaderValue::from_static("application/octet-stream")),
        move || {
            observed.fetch_add(1, Ordering::SeqCst);
            Ok(concord_core::advanced::DynBody::from_bytes(
                Bytes::from_static(b"factory-body"),
            ))
        },
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let err = client
        .execute_plan::<concord_core::prelude::Text<String>>(plan)
        .await
        .expect_err("factory request exhausts retry response");
    assert!(matches!(err, ApiClientError::HttpStatus { .. }));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(transport.send_count(), 2);
    assert!(transport.captured().iter().all(|request| matches!(
        &request.body,
        CapturedBody::Bytes(bytes) if bytes == &Bytes::from_static(b"factory-body")
    )));

    let one_shot_transport =
        StreamTransport::success(Arc::new(StdMutex::new(Vec::new())), response);
    let one_shot_client = ApiClient::<TestCx, _>::with_transport(
        (),
        TestAuthVars::default(),
        one_shot_transport.clone(),
    );
    let mut one_shot_plan = stream_request_plan(
        "OneShotAttempts",
        Method::PUT,
        "/one-shot-attempts",
        retry_policy(),
        StreamBody::from_bytes(Bytes::from_static(b"one-shot-body")),
        HeaderValue::from_static("application/octet-stream"),
    );
    one_shot_plan.endpoint.meta.idempotent = true;
    let err = one_shot_client
        .execute_plan::<concord_core::prelude::Text<String>>(one_shot_plan)
        .await
        .expect_err("one-shot request is terminal");
    assert!(matches!(err, ApiClientError::HttpStatus { .. }));
    assert_eq!(one_shot_transport.send_count(), 1);
}

#[tokio::test]
async fn replay_factory_failure_is_pre_transport_and_not_one_shot_exhaustion() {
    let transport = StreamTransport::success(
        Arc::new(StdMutex::new(Vec::new())),
        MockResponse::text(StatusCode::OK, "ok"),
    );
    let client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
    let mut plan = stream_request_plan(
        "ReplayFactoryFailure",
        Method::POST,
        "/replay-factory-failure",
        ResolvedPolicy::default(),
        StreamBody::from_bytes(Bytes::new()),
        HeaderValue::from_static("application/octet-stream"),
    );
    plan.body = PreparedBody::replay_factory(http_body::SizeHint::new(), None, || {
        Err(concord_core::advanced::BodyError::invalid_configuration())
    });
    let err = client
        .execute_plan::<concord_core::prelude::Text<String>>(plan)
        .await
        .expect_err("factory failure");
    assert!(matches!(err, ApiClientError::PolicyViolation { .. }));
    assert!(err.to_string().contains("request body factory failed"));
    assert!(!err.to_string().contains("already been consumed"));
    assert_eq!(transport.send_count(), 0);
}

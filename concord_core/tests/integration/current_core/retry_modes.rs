use super::common::{
    TestAuthVars, TestCx, auth_policy, execute_buffered, native_mock, wait_bounded,
};
use bytes::Bytes;
use concord_core::advanced::{
    BodyCodec, CodecError, EncodeContext, EncodedBody, EncodedRequest, ErrorContext,
    PostResponseHookContext, PreSendHookContext, RateLimitContext, RateLimitFuture,
    RateLimitPermit, RateLimitResponseAction, RateLimitResponseContext, RateLimiter, RuntimeHooks,
    SafeProxy, TextContentType, TransportErrorHookContext,
};
use concord_core::internal::{
    EndpointMeta, EndpointPlan, PreparedBody, RequestOverrides, RequestPlan, ResolvedPolicy,
    ResolvedRoute, ResponsePlan,
};
use concord_core::prelude::{
    ApiClient, ApiClientError, ClientContext, Endpoint, IntoEndpointPlan, RetryMode,
    StatusRetryConfig,
};
use http::{HeaderValue, Method, StatusCode};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

static BYTE_ENCODER_CALLS: AtomicUsize = AtomicUsize::new(0);

struct CountingByteCodec;

impl BodyCodec for CountingByteCodec {
    type Value = Bytes;
    type Content = TextContentType;

    fn encode(value: Self::Value, _ctx: EncodeContext<'_>) -> Result<EncodedBody, CodecError> {
        BYTE_ENCODER_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(EncodedBody::from_bytes(value).text())
    }
}

#[derive(Clone)]
struct FixedRetryCx;

impl ClientContext for FixedRetryCx {
    type Vars = ();
    type AuthVars = TestAuthVars;
    type AuthState = ();

    const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTP;
    const DOMAIN: &'static str = "retry.example";
    const ORIGIN: concord_core::__private::v1::ApiOriginDescriptor =
        concord_core::__private::v1::ApiOriginDescriptor::FixedSingleOrigin(
            concord_core::__private::v1::FixedOriginDescriptor {
                scheme: concord_core::__private::v1::OriginScheme::Http,
                authority: "retry.example",
            },
        );

    fn init_auth_state(_vars: &Self::Vars, _auth: &Self::AuthVars) -> Self::AuthState {}

    fn prepare_auth_requirement<'a>(
        requirement: &'a concord_core::advanced::AuthRequirement,
        request: &'a mut concord_core::advanced::AuthApplicationRequest<'_>,
        vars: &'a Self::Vars,
        auth: &'a Self::AuthVars,
        auth_state: &'a Self::AuthState,
        executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
        meta: &'a concord_core::advanced::RequestMeta,
    ) -> concord_core::advanced::AuthFuture<
        'a,
        Result<concord_core::advanced::PreparedAuthCredential, concord_core::advanced::AuthError>,
    > {
        <TestCx as ClientContext>::prepare_auth_requirement(
            requirement,
            request,
            vars,
            auth,
            auth_state,
            executor,
            meta,
        )
    }

    fn plan_auth_response(
        requirement: &concord_core::advanced::AuthRequirement,
        applied: &concord_core::advanced::AuthAppliedCredential,
        vars: &Self::Vars,
        auth: &Self::AuthVars,
        meta: &concord_core::advanced::RequestMeta,
        status: StatusCode,
        headers: &http::HeaderMap,
    ) -> Result<concord_core::advanced::AuthRejectionAction, concord_core::advanced::AuthError>
    {
        <TestCx as ClientContext>::plan_auth_response(
            requirement,
            applied,
            vars,
            auth,
            meta,
            status,
            headers,
        )
    }

    fn apply_refresh_auth_action<'a>(
        action: &'a concord_core::advanced::AuthRejectionAction,
        requirement: &'a concord_core::advanced::AuthRequirement,
        applied: &'a concord_core::advanced::AuthAppliedCredential,
        vars: &'a Self::Vars,
        auth: &'a Self::AuthVars,
        auth_state: &'a Self::AuthState,
        executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
        meta: &'a concord_core::advanced::RequestMeta,
        status: StatusCode,
    ) -> concord_core::advanced::AuthFuture<'a, Result<(), concord_core::advanced::AuthError>> {
        <TestCx as ClientContext>::apply_refresh_auth_action(
            action,
            requirement,
            applied,
            vars,
            auth,
            auth_state,
            executor,
            meta,
            status,
        )
    }
}

#[derive(Clone)]
struct RetryEndpoint {
    method: Method,
}

struct BodyRetryEndpoint {
    body: PreparedBody,
}

impl Endpoint<FixedRetryCx> for BodyRetryEndpoint {
    type Response = String;

    fn execute<'a>(
        client: &'a ApiClient<FixedRetryCx>,
        plan: RequestPlan,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Response, ApiClientError>> + Send + 'a>> {
        execute_buffered::<_, concord_core::prelude::Text<String>>(client, plan)
    }
}

impl IntoEndpointPlan<FixedRetryCx> for BodyRetryEndpoint {
    fn into_plan(
        self,
        _ctx: &concord_core::internal::ClientPlanContext<'_, FixedRetryCx>,
    ) -> Result<RequestPlan, ApiClientError> {
        Ok(retry_plan(Method::GET, self.body))
    }
}

fn retry_plan(method: Method, body: PreparedBody) -> RequestPlan {
    RequestPlan {
        endpoint: EndpointPlan {
            meta: EndpointMeta {
                name: "RetryMode",
                method: method.clone(),
                idempotent: matches!(method, Method::GET | Method::HEAD | Method::OPTIONS),
                facade_path: &[],
            },
            route: ResolvedRoute::new(http::uri::Scheme::HTTP, "retry.example", "/status"),
            policy: ResolvedPolicy::default(),
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

impl Endpoint<FixedRetryCx> for RetryEndpoint {
    type Response = String;

    fn execute<'a>(
        client: &'a ApiClient<FixedRetryCx>,
        plan: RequestPlan,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Response, ApiClientError>> + Send + 'a>> {
        execute_buffered::<_, concord_core::prelude::Text<String>>(client, plan)
    }
}

impl concord_core::prelude::ReusableEndpoint<FixedRetryCx> for RetryEndpoint {
    fn plan(
        &self,
        _ctx: &concord_core::internal::ClientPlanContext<'_, FixedRetryCx>,
    ) -> Result<RequestPlan, ApiClientError> {
        Ok(retry_plan(self.method.clone(), PreparedBody::empty()))
    }
}

#[derive(Default)]
struct VisibleCounters {
    pre_send: AtomicUsize,
    post_response: AtomicUsize,
    acquire: AtomicUsize,
}

impl RuntimeHooks for VisibleCounters {
    fn pre_send<'a>(
        &'a self,
        _ctx: PreSendHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ApiClientError>> + Send + 'a>> {
        self.pre_send.fetch_add(1, Ordering::Relaxed);
        Box::pin(async { Ok(()) })
    }

    fn post_response<'a>(
        &'a self,
        _ctx: PostResponseHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        self.post_response.fetch_add(1, Ordering::Relaxed);
        Box::pin(async {})
    }

    fn transport_error<'a>(
        &'a self,
        _ctx: TransportErrorHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async {})
    }
}

struct CountingLimiter {
    counters: Arc<VisibleCounters>,
}

impl RateLimiter for CountingLimiter {
    fn acquire<'a>(
        &'a self,
        _ctx: RateLimitContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitPermit, ApiClientError>> {
        self.counters.acquire.fetch_add(1, Ordering::Relaxed);
        Box::pin(async { Ok(RateLimitPermit) })
    }

    fn on_response<'a>(
        &'a self,
        _ctx: RateLimitResponseContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitResponseAction, ApiClientError>> {
        Box::pin(async { Ok(RateLimitResponseAction::Continue) })
    }
}

fn proxy_client(
    server: &native_mock::MockServer,
    retry_mode: RetryMode,
) -> Result<ApiClient<FixedRetryCx>, concord_core::prelude::RetryModeError> {
    proxy_client_with_auth(server, retry_mode, TestAuthVars::default())
}

fn proxy_client_with_auth(
    server: &native_mock::MockServer,
    retry_mode: RetryMode,
    auth: TestAuthVars,
) -> Result<ApiClient<FixedRetryCx>, concord_core::prelude::RetryModeError> {
    let proxy = SafeProxy::all(server.base_url().as_str()).expect("loopback HTTP proxy is safe");
    ApiClient::with_reqwest_builder_and_retry_mode((), auth, retry_mode, |builder| {
        Ok(builder.proxy(proxy))
    })
}

#[tokio::test]
async fn status_mode_hidden_retry_is_one_visible_execution() {
    let (server, handle) = native_mock::mock()
        .replies([
            native_mock::MockReply::status(StatusCode::SERVICE_UNAVAILABLE)
                .with_header(http::header::RETRY_AFTER, HeaderValue::from_static("30")),
            native_mock::MockReply::ok_text(bytes::Bytes::from_static(b"ok")),
        ])
        .build();
    let mode = RetryMode::Status(
        StatusRetryConfig::new(1, [StatusCode::SERVICE_UNAVAILABLE]).expect("valid mode"),
    );
    let mut client = proxy_client(&server, mode).expect("fixed-origin status client");
    let counters = Arc::new(VisibleCounters::default());
    client.set_runtime_hooks(counters.clone());
    client.configure(|config| {
        config.rate_limiter(Arc::new(CountingLimiter {
            counters: counters.clone(),
        }));
    });

    let started = std::time::Instant::now();
    let value = wait_bounded(
        "status-mode hidden retry",
        client
            .request(RetryEndpoint {
                method: Method::GET,
            })
            .execute(),
    )
    .await
    .expect("hidden retry reaches the successful response");

    assert_eq!(value, "ok");
    assert!(started.elapsed() < std::time::Duration::from_secs(1));
    assert_eq!(handle.wire_request_count(), 2);
    assert_eq!(counters.pre_send.load(Ordering::Relaxed), 1);
    assert_eq!(counters.post_response.load(Ordering::Relaxed), 1);
    assert_eq!(counters.acquire.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn status_mode_cloneable_encoded_bytes_retry_hidden_with_identical_payload() {
    const PAYLOAD: &[u8] = b"cloneable-visible-payload";
    BYTE_ENCODER_CALLS.store(0, Ordering::SeqCst);
    let (server, handle) = native_mock::mock()
        .replies([
            native_mock::MockReply::status(StatusCode::SERVICE_UNAVAILABLE),
            native_mock::MockReply::ok_text(bytes::Bytes::from_static(b"ok")),
        ])
        .build();
    let mode = RetryMode::Status(
        StatusRetryConfig::new(1, [StatusCode::SERVICE_UNAVAILABLE]).expect("valid mode"),
    );
    let mut client = proxy_client(&server, mode).expect("fixed-origin status client");
    let counters = Arc::new(VisibleCounters::default());
    client.set_runtime_hooks(counters.clone());
    client.configure(|config| {
        config.rate_limiter(Arc::new(CountingLimiter {
            counters: counters.clone(),
        }));
    });
    let prepared =
        <EncodedRequest<CountingByteCodec> as concord_core::advanced::RequestEntity>::prepare(
            Bytes::from_static(PAYLOAD),
            ErrorContext {
                endpoint: "RetryMode",
                method: Method::GET,
            },
        )
        .expect("encoded reusable bytes");

    let value = client
        .request(BodyRetryEndpoint {
            body: prepared.body,
        })
        .execute()
        .await
        .expect("hidden status retry succeeds");

    assert_eq!(value, "ok");
    assert_eq!(BYTE_ENCODER_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(handle.wire_request_count(), 2);
    let requests = handle.recorded();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].body.as_ref(), PAYLOAD);
    assert_eq!(requests[1].body.as_ref(), PAYLOAD);
    assert_eq!(counters.pre_send.load(Ordering::Relaxed), 1);
    assert_eq!(counters.post_response.load(Ordering::Relaxed), 1);
    assert_eq!(counters.acquire.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn status_mode_honors_the_approved_two_retry_maximum() {
    let (server, handle) = native_mock::mock()
        .replies([
            native_mock::MockReply::status(StatusCode::BAD_GATEWAY),
            native_mock::MockReply::status(StatusCode::BAD_GATEWAY),
            native_mock::MockReply::ok_text(bytes::Bytes::from_static(b"third-send")),
        ])
        .build();
    let mode = RetryMode::Status(
        StatusRetryConfig::new(2, [StatusCode::BAD_GATEWAY]).expect("valid mode"),
    );
    let client = proxy_client(&server, mode).expect("fixed-origin status client");

    let value = client
        .request(RetryEndpoint {
            method: Method::OPTIONS,
        })
        .execute()
        .await
        .expect("the second and final hidden retry succeeds");

    assert_eq!(value, "third-send");
    assert_eq!(handle.wire_request_count(), 3);
}

#[tokio::test]
async fn authentication_recovery_gets_an_independent_status_retry_envelope() {
    let (server, handle) = native_mock::mock()
        .replies([
            native_mock::MockReply::status(StatusCode::SERVICE_UNAVAILABLE),
            native_mock::MockReply::status(StatusCode::UNAUTHORIZED),
            native_mock::MockReply::status(StatusCode::SERVICE_UNAVAILABLE),
            native_mock::MockReply::ok_text(bytes::Bytes::from_static(b"recovered")),
        ])
        .build();
    let mode = RetryMode::Status(
        StatusRetryConfig::new(1, [StatusCode::SERVICE_UNAVAILABLE]).expect("valid mode"),
    );
    let mut client = proxy_client_with_auth(
        &server,
        mode,
        TestAuthVars {
            token: Some("AUTH_STATUS_RETRY_SENTINEL".to_string()),
            identity: "refresh",
        },
    )
    .expect("fixed-origin status client");
    let counters = Arc::new(VisibleCounters::default());
    client.set_runtime_hooks(counters.clone());
    client.configure(|config| {
        config.rate_limiter(Arc::new(CountingLimiter {
            counters: counters.clone(),
        }));
    });
    // Add authentication without making it endpoint-specific retry policy.
    let mut plan = retry_plan(Method::GET, PreparedBody::empty());
    plan.endpoint.policy = auth_policy(concord_core::advanced::AuthPlacement::Bearer);

    let response = client
        .execute_plan::<concord_core::prelude::Text<String>>(plan)
        .await
        .expect("one bounded authentication recovery succeeds");

    assert_eq!(response.value(), "recovered");
    assert_eq!(handle.wire_request_count(), 4);
    assert_eq!(counters.pre_send.load(Ordering::Relaxed), 2);
    assert_eq!(counters.post_response.load(Ordering::Relaxed), 2);
    assert_eq!(counters.acquire.load(Ordering::Relaxed), 2);
}

#[tokio::test]
async fn disabled_mode_has_one_wire_request_per_visible_execution() {
    let (server, handle) = native_mock::mock()
        .reply(native_mock::MockReply::status(
            StatusCode::SERVICE_UNAVAILABLE,
        ))
        .build();
    let client = proxy_client(&server, RetryMode::Disabled).expect("disabled retry client");

    let error = client
        .request(RetryEndpoint {
            method: Method::GET,
        })
        .execute()
        .await
        .expect_err("503 remains terminal with retries disabled");

    assert!(matches!(error, ApiClientError::HttpStatus { .. }));
    assert_eq!(handle.wire_request_count(), 1);
}

#[tokio::test]
async fn status_mode_never_retries_an_unsafe_method() {
    let (server, handle) = native_mock::mock()
        .reply(native_mock::MockReply::status(
            StatusCode::SERVICE_UNAVAILABLE,
        ))
        .build();
    let mode = RetryMode::Status(
        StatusRetryConfig::new(2, [StatusCode::SERVICE_UNAVAILABLE]).expect("valid mode"),
    );
    let client = proxy_client(&server, mode).expect("fixed-origin status client");

    let error = client
        .request(RetryEndpoint {
            method: Method::POST,
        })
        .execute()
        .await
        .expect_err("unsafe method must not be retried");

    assert!(matches!(error, ApiClientError::HttpStatus { .. }));
    assert_eq!(handle.wire_request_count(), 1);
}

#[tokio::test]
async fn status_mode_does_not_resend_a_direct_stream_body() {
    let (server, handle) = native_mock::mock()
        .reply(native_mock::MockReply::status(
            StatusCode::SERVICE_UNAVAILABLE,
        ))
        .build();
    let mode = RetryMode::Status(
        StatusRetryConfig::new(2, [StatusCode::SERVICE_UNAVAILABLE]).expect("valid mode"),
    );
    let client = proxy_client(&server, mode).expect("fixed-origin status client");
    let body = PreparedBody::from_stream_body(
        concord_core::advanced::StreamBody::from_bytes(bytes::Bytes::from_static(b"stream")),
        Some(HeaderValue::from_static("application/octet-stream")),
    );

    let error = client
        .request(BodyRetryEndpoint { body })
        .execute()
        .await
        .expect_err("Reqwest cannot clone a direct stream body");

    assert!(matches!(error, ApiClientError::HttpStatus { .. }));
    assert_eq!(handle.wire_request_count(), 1);
}

#[cfg(feature = "multipart")]
#[tokio::test]
async fn status_mode_does_not_resend_reusable_multipart() {
    use concord_core::advanced::{ErrorContext, MultipartBody, MultipartRequest, RequestEntity};

    let (server, handle) = native_mock::mock()
        .reply(native_mock::MockReply::status(
            StatusCode::SERVICE_UNAVAILABLE,
        ))
        .build();
    let mode = RetryMode::Status(
        StatusRetryConfig::new(2, [StatusCode::SERVICE_UNAVAILABLE]).expect("valid mode"),
    );
    let client = proxy_client(&server, mode).expect("fixed-origin status client");
    let body = MultipartRequest::prepare(
        MultipartBody::new().bytes("payload", bytes::Bytes::from_static(b"multipart")),
        ErrorContext {
            endpoint: "RetryMode",
            method: Method::GET,
        },
    )
    .expect("rebuildable multipart recipe")
    .body;

    let error = client
        .request(BodyRetryEndpoint { body })
        .execute()
        .await
        .expect_err("Reqwest cannot clone a materialized multipart form");

    assert!(matches!(error, ApiClientError::HttpStatus { .. }));
    assert_eq!(handle.wire_request_count(), 1);
}

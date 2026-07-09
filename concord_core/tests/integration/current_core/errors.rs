use super::common::{
    CapturedTransportRequest, CursorItems, CursorItemsEndpoint, MockOutcome, MockResponse,
    MockTransport, ObservationRateLimiter, ObservationRuntimeHooks, PaginationVariant,
    TestAuthVars, TestCx, TextEndpoint, auth_policy, buffered_endpoint_execute,
    buffered_endpoint_response_terminal, client, request_plan, retry_policy,
    retry_policy_for_statuses,
};
use crate::support::assert_text_does_not_contain_any;
use bytes::Bytes;
use concord_core::advanced::{
    DebugSink, RateLimitBucketUse, RateLimitContext, RateLimitFuture, RateLimitKey,
    RateLimitKeyPart, RateLimitPermit, RateLimitWindow, RateLimiter, RouteBuilder, Transport,
    TransportBody, TransportError, TransportErrorKind, TransportRequest, TransportResponse,
};
use concord_core::error::ErrorCategory;
use concord_core::internal::{
    BodyPlan, ClientPlanContext, Format, RequestArgs, RequestPlan, ResolvedPolicy, ResolvedRoute,
};
use concord_core::prelude::{
    ApiClient, ApiClientError, CursorPagination, DebugLevel, Endpoint, ReusableEndpoint,
};
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use std::error::Error;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::Mutex;

const RAW_AUTH_SENTINEL_PR77: &str = "RAW_AUTH_SENTINEL_PR77";
const REQUEST_BODY_SENTINEL_PR77: &str = "REQUEST_BODY_SENTINEL_PR77";
const RESPONSE_BODY_SENTINEL_PR77: &str = "RESPONSE_BODY_SENTINEL_PR77";
const RESPONSE_BODY_READ_SENTINEL_PR77: &str = "LEAK_SENTINEL_RESPONSE_BODY_READ";
const QUERY_SECRET_SENTINEL_PR77: &str = "QUERY_SECRET_SENTINEL_PR77";
const HEADER_SECRET_SENTINEL_PR77: &str = "HEADER_SECRET_SENTINEL_PR77";

#[derive(Clone)]
struct InvalidParamEndpoint;

impl Endpoint<TestCx> for InvalidParamEndpoint {
    type Response = String;

    buffered_endpoint_execute!(TestCx, concord_core::prelude::Text<String>);
}

buffered_endpoint_response_terminal!(
    InvalidParamEndpoint,
    TestCx,
    concord_core::prelude::Text<String>
);

impl ReusableEndpoint<TestCx> for InvalidParamEndpoint {
    fn plan(&self, _ctx: &ClientPlanContext<'_, TestCx>) -> Result<RequestPlan, ApiClientError> {
        Err(ApiClientError::invalid_param(
            concord_core::advanced::ErrorContext {
                endpoint: "InvalidParam",
                method: Method::GET,
            },
            "id",
        ))
    }
}

#[derive(Clone)]
struct DynamicPathEndpoint {
    required: String,
    optional: Option<String>,
    formatted: String,
}

impl Endpoint<TestCx> for DynamicPathEndpoint {
    type Response = String;

    buffered_endpoint_execute!(TestCx, concord_core::prelude::Text<String>);
}

buffered_endpoint_response_terminal!(
    DynamicPathEndpoint,
    TestCx,
    concord_core::prelude::Text<String>
);

impl ReusableEndpoint<TestCx> for DynamicPathEndpoint {
    fn plan(&self, _ctx: &ClientPlanContext<'_, TestCx>) -> Result<RequestPlan, ApiClientError> {
        build_dynamic_path_request_plan(
            "DynamicPath",
            Method::GET,
            &self.required,
            self.optional.as_deref(),
            &self.formatted,
        )
    }
}

#[derive(Default)]
struct CapturingDebugSink {
    events: Arc<Mutex<Vec<String>>>,
}

impl CapturingDebugSink {
    fn new(events: Arc<Mutex<Vec<String>>>) -> Self {
        Self { events }
    }
}

impl DebugSink for CapturingDebugSink {
    fn request_start(
        &self,
        dbg: DebugLevel,
        method: &Method,
        url: &str,
        endpoint: &'static str,
        page_index: u32,
    ) {
        self.events
            .try_lock()
            .expect("debug events lock")
            .push(format!(
                "debug_request:{dbg}:{method}:{url}:{endpoint}:{page_index}"
            ));
    }

    fn request_headers(
        &self,
        _dbg: DebugLevel,
        headers: concord_core::advanced::SanitizedHeaders<'_>,
    ) {
        self.events
            .try_lock()
            .expect("debug events lock")
            .push(format!("debug_request_headers:{headers:?}"));
    }

    fn response_status(&self, dbg: DebugLevel, status: StatusCode, url: &str, ok: bool) {
        self.events
            .try_lock()
            .expect("debug events lock")
            .push(format!("debug_response:{dbg}:{status}:{url}:{ok}"));
    }

    fn response_headers(
        &self,
        _dbg: DebugLevel,
        headers: concord_core::advanced::SanitizedHeaders<'_>,
    ) {
        self.events
            .try_lock()
            .expect("debug events lock")
            .push(format!("debug_response_headers:{headers:?}"));
    }
}

struct FailingRateLimiter {
    events: Arc<Mutex<Vec<String>>>,
}

impl FailingRateLimiter {
    fn new(events: Arc<Mutex<Vec<String>>>) -> Self {
        Self { events }
    }
}

impl RateLimiter for FailingRateLimiter {
    fn acquire<'a>(
        &'a self,
        _ctx: RateLimitContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitPermit, ApiClientError>> {
        Box::pin(async move {
            self.events.lock().await.push("rate_acquire".to_string());
            Err(ApiClientError::RuntimeState {
                ctx: concord_core::advanced::ErrorContext {
                    endpoint: "Text",
                    method: Method::GET,
                },
                subsystem: "rate-limit",
                msg: "synthetic acquire failure",
            })
        })
    }
}

struct ResponseActionFailingRateLimiter {
    events: Arc<Mutex<Vec<String>>>,
}

impl ResponseActionFailingRateLimiter {
    fn new(events: Arc<Mutex<Vec<String>>>) -> Self {
        Self { events }
    }
}

impl RateLimiter for ResponseActionFailingRateLimiter {
    fn on_response<'a>(
        &'a self,
        _ctx: concord_core::advanced::RateLimitResponseContext<'a>,
    ) -> RateLimitFuture<'a, Result<concord_core::advanced::RateLimitResponseAction, ApiClientError>>
    {
        Box::pin(async move {
            self.events.lock().await.push("rate_response".to_string());
            Err(ApiClientError::RuntimeState {
                ctx: concord_core::advanced::ErrorContext {
                    endpoint: "Text",
                    method: Method::GET,
                },
                subsystem: "rate-limit",
                msg: "synthetic response-action failure",
            })
        })
    }
}

fn diagnostics(err: &ApiClientError) -> String {
    let mut rendered = format!("{err}\n{err:?}\n{err:#?}\n");
    let mut source = err.source();
    while let Some(current) = source {
        rendered.push_str(&current.to_string());
        rendered.push('\n');
        rendered.push_str(&format!("{current:?}\n"));
        source = current.source();
    }
    rendered
}

async fn observed(events: &Arc<Mutex<Vec<String>>>) -> String {
    events.lock().await.join("\n")
}

fn assert_error_safe(err: &ApiClientError) {
    let rendered = diagnostics(err);
    for sentinel in [
        RAW_AUTH_SENTINEL_PR77,
        REQUEST_BODY_SENTINEL_PR77,
        RESPONSE_BODY_SENTINEL_PR77,
        RESPONSE_BODY_READ_SENTINEL_PR77,
        QUERY_SECRET_SENTINEL_PR77,
        HEADER_SECRET_SENTINEL_PR77,
    ] {
        assert!(
            !rendered.contains(sentinel),
            "diagnostics leaked sentinel {sentinel}: {rendered}"
        );
    }
}

async fn assert_observers_safe(events: &Arc<Mutex<Vec<String>>>) {
    let rendered = observed(events).await;
    for sentinel in [
        RAW_AUTH_SENTINEL_PR77,
        REQUEST_BODY_SENTINEL_PR77,
        RESPONSE_BODY_SENTINEL_PR77,
        RESPONSE_BODY_READ_SENTINEL_PR77,
        QUERY_SECRET_SENTINEL_PR77,
        HEADER_SECRET_SENTINEL_PR77,
    ] {
        assert!(
            !rendered.contains(sentinel),
            "observer events leaked sentinel {sentinel}: {rendered}"
        );
    }
}

#[derive(Clone, Copy)]
struct LeakyResponseBodyReadError(&'static str);

impl std::fmt::Debug for LeakyResponseBodyReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

impl std::fmt::Display for LeakyResponseBodyReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for LeakyResponseBodyReadError {}

struct FailingResponseBody;

impl TransportBody for FailingResponseBody {
    fn next_chunk<'a>(
        &'a mut self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Option<Bytes>, TransportError>> + Send + 'a>,
    > {
        Box::pin(async move {
            Err(TransportError::with_kind(
                TransportErrorKind::Request,
                LeakyResponseBodyReadError(RESPONSE_BODY_READ_SENTINEL_PR77),
            ))
        })
    }
}

#[derive(Clone)]
struct BodyReadFailingTransport {
    requests: Arc<Mutex<Vec<CapturedTransportRequest>>>,
}

impl BodyReadFailingTransport {
    fn new() -> Self {
        Self {
            requests: Arc::new(Mutex::new(Vec::new())),
        }
    }

    async fn sent_count(&self) -> usize {
        self.requests.lock().await.len()
    }

    async fn requests(&self) -> Vec<CapturedTransportRequest> {
        let mut requests = self.requests.lock().await;
        std::mem::take(&mut *requests)
    }
}

impl Transport for BodyReadFailingTransport {
    fn send(
        &self,
        req: TransportRequest,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<TransportResponse, TransportError>> + Send>,
    > {
        let requests = self.requests.clone();
        Box::pin(async move {
            let TransportRequest {
                meta,
                url,
                headers,
                body,
                timeout,
                rate_limit,
                transport_auth,
                extensions,
            } = req;
            requests.lock().await.push(CapturedTransportRequest {
                meta: meta.clone(),
                url: url.clone(),
                headers: headers.clone(),
                body,
                timeout,
                rate_limit: rate_limit.clone(),
                transport_auth: transport_auth.clone(),
                extensions: extensions.clone(),
            });
            let mut response_headers = HeaderMap::new();
            response_headers.insert(
                http::header::CONTENT_TYPE,
                HeaderValue::from_static("text/plain"),
            );
            Ok(TransportResponse {
                meta,
                url,
                status: StatusCode::OK,
                headers: response_headers,
                content_length: None,
                rate_limit,
                body: Box::new(FailingResponseBody),
            })
        })
    }
}

fn body_read_count(counter: &Arc<AtomicUsize>) -> usize {
    counter.load(Ordering::Relaxed)
}

fn body_read_failure_policy() -> ResolvedPolicy {
    let mut policy = auth_policy(concord_core::advanced::AuthPlacement::Bearer);
    policy.headers.insert(
        http::HeaderName::from_static("x-response-body-read"),
        HeaderValue::from_static(HEADER_SECRET_SENTINEL_PR77),
    );
    policy
}

fn validate_dynamic_path_segment(
    ctx: concord_core::advanced::ErrorContext,
    label: &'static str,
    segment: &str,
    path: &mut concord_core::advanced::UrlPath,
) -> Result<(), ApiClientError> {
    if segment.is_empty()
        || segment == "."
        || segment == ".."
        || segment.contains('/')
        || segment.contains('\\')
    {
        return Err(ApiClientError::invalid_param(ctx, label));
    }

    path.push_segment_encoded(segment);
    Ok(())
}

fn build_dynamic_path_request_plan(
    name: &'static str,
    method: Method,
    required: &str,
    optional: Option<&str>,
    formatted: &str,
) -> Result<RequestPlan, ApiClientError> {
    let ctx = concord_core::advanced::ErrorContext {
        endpoint: name,
        method: method.clone(),
    };
    let mut route = RouteBuilder::new();
    route.path_mut().push_raw("users");
    validate_dynamic_path_segment(ctx.clone(), "vars.id", required, route.path_mut())?;
    if let Some(optional) = optional {
        validate_dynamic_path_segment(ctx.clone(), "ep.id", optional, route.path_mut())?;
    }
    validate_dynamic_path_segment(ctx, "fmt", formatted, route.path_mut())?;
    route.path_mut().push_raw("posts");

    let mut plan = request_plan(name, method, "/users", ResolvedPolicy::default(), None);
    plan.endpoint.route = ResolvedRoute::new(
        http::uri::Scheme::HTTPS,
        "example.com",
        route.path().as_str().to_string(),
    );
    Ok(plan)
}

fn rate_policy() -> ResolvedPolicy {
    ResolvedPolicy {
        rate_limit: {
            let mut plan = concord_core::advanced::RateLimitPlan::new();
            plan.push_bucket(
                RateLimitBucketUse::new(
                    "test",
                    "errors",
                    RateLimitKey::new(vec![RateLimitKeyPart::endpoint()]),
                )
                .with_window(RateLimitWindow::new(
                    NonZeroU32::new(10).expect("non-zero"),
                    std::time::Duration::from_secs(1),
                )),
            );
            plan
        },
        ..Default::default()
    }
}

fn with_runtime_observers(
    client: &mut concord_core::prelude::ApiClient<TestCx, MockTransport>,
    events: Arc<Mutex<Vec<String>>>,
) {
    client.configure(|cfg| {
        cfg.debug_level(DebugLevel::VV)
            .debug_sink(Arc::new(CapturingDebugSink::new(events.clone())))
            .rate_limiter(Arc::new(ObservationRateLimiter::new(events.clone())))
            .runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events)));
    });
}

#[tokio::test]
async fn error_taxonomy_variant_snapshot() {
    let ctx = concord_core::advanced::ErrorContext {
        endpoint: "Snapshot",
        method: Method::GET,
    };
    let mut headers = HeaderMap::new();
    headers.insert("x-public", HeaderValue::from_static("safe"));
    let auth_err = concord_core::auth::AuthError::new(
        concord_core::auth::AuthErrorKind::RejectedCredential,
        "auth challenge rejected",
    );

    let cases: Vec<(&str, ApiClientError, ErrorCategory)> = vec![
        (
            "url/param validation",
            ApiClientError::invalid_param(ctx.clone(), "id"),
            ErrorCategory::Config,
        ),
        (
            "auth rejection",
            ApiClientError::Auth {
                ctx: ctx.clone(),
                source: auth_err,
            },
            ErrorCategory::AuthRejected,
        ),
        (
            "transport",
            ApiClientError::Transport {
                ctx: ctx.clone(),
                source: concord_core::advanced::TransportError::with_kind(
                    TransportErrorKind::Connect,
                    std::io::Error::other("connect failed"),
                ),
            },
            ErrorCategory::Transport,
        ),
        (
            "http status",
            ApiClientError::HttpStatus {
                ctx: ctx.clone(),
                status: StatusCode::INTERNAL_SERVER_ERROR,
                headers: Box::new(headers.clone()),
                rate_limit: None,
            },
            ErrorCategory::HttpStatus,
        ),
        (
            "content-length body limit",
            ApiClientError::ResponseTooLarge {
                ctx: ctx.clone(),
                limit: 4,
                actual: 5,
            },
            ErrorCategory::Decode,
        ),
        (
            "streaming body limit",
            ApiClientError::ResponseBodyLimitExceeded {
                ctx: ctx.clone(),
                limit: 4,
            },
            ErrorCategory::Decode,
        ),
        (
            "request streaming body limit",
            ApiClientError::RequestBodyLimitExceeded {
                ctx: ctx.clone(),
                limit: 4,
                actual: 5,
            },
            ErrorCategory::Config,
        ),
        (
            "decode",
            ApiClientError::decode_error(
                ctx.clone(),
                StatusCode::OK,
                Some("text/plain"),
                std::io::Error::new(std::io::ErrorKind::InvalidData, "bad payload"),
            ),
            ErrorCategory::Decode,
        ),
        (
            "response contract",
            ApiClientError::response_contract(
                ctx.clone(),
                "stream response content type did not match expected media type",
            ),
            ErrorCategory::ResponseContract,
        ),
        (
            "pagination",
            ApiClientError::pagination(
                ctx.clone(),
                concord_core::error::PaginationErrorKind::NonProgress,
                "pagination did not make progress",
            ),
            ErrorCategory::Pagination,
        ),
        (
            "runtime state",
            ApiClientError::RuntimeState {
                ctx: ctx.clone(),
                subsystem: "rate-limit",
                msg: "state unavailable",
            },
            ErrorCategory::InternalInvariant,
        ),
    ];

    for (name, err, category) in cases {
        assert_eq!(err.category(), category, "{name}");
        assert_error_safe(&err);
    }

    let pagination_err = ApiClientError::pagination(
        ctx,
        concord_core::error::PaginationErrorKind::PageLimitExceeded,
        "pagination hard page cap exceeded",
    );
    assert_eq!(pagination_err.category(), ErrorCategory::Pagination);
    assert_eq!(
        pagination_err.pagination_error_kind(),
        Some(concord_core::error::PaginationErrorKind::PageLimitExceeded)
    );
}

#[tokio::test]
async fn no_content_status_mismatch_remains_structured_and_response_contract_categorized() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::NO_CONTENT, "ignored")],
    );
    let mut client = client(TestAuthVars::default(), transport.clone());
    with_runtime_observers(&mut client, events.clone());

    let err = client
        .request(TextEndpoint::default())
        .response()
        .await
        .expect_err("no-content status mismatch should fail");

    assert!(matches!(
        err,
        ApiClientError::NoContentStatusRequiresNoContent { .. }
    ));
    assert_eq!(err.category(), ErrorCategory::ResponseContract);
}

#[tokio::test]
async fn request_construction_errors_are_typed_and_pre_transport() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "ok")],
    );
    let mut client = client(TestAuthVars::default(), transport.clone());
    with_runtime_observers(&mut client, events.clone());

    let err = client
        .request(InvalidParamEndpoint)
        .response()
        .await
        .expect_err("invalid request construction should fail");

    assert!(matches!(err, ApiClientError::InvalidParam { .. }));
    assert_eq!(err.category(), ErrorCategory::Config);
    assert_eq!(transport.sent_count().await, 0);
    let events = events.lock().await.clone();
    assert!(!events.iter().any(|event| event == "rate_acquire"));
    assert!(!events.iter().any(|event| event == "transport"));
    assert_error_safe(&err);
    assert_observers_safe(&Arc::new(Mutex::new(events))).await;
}

#[tokio::test]
async fn required_dynamic_path_segments_reject_empty_and_reserved_values_before_transport() {
    for (value, label) in [
        ("", "vars.id"),
        (".", "vars.id"),
        ("..", "vars.id"),
        ("a/b", "vars.id"),
        ("a\\b", "vars.id"),
    ] {
        let events = Arc::new(Mutex::new(Vec::new()));
        let transport = MockTransport::new(
            events.clone(),
            vec![MockResponse::text(StatusCode::OK, "ok")],
        );
        let client = client(
            TestAuthVars {
                token: Some(RAW_AUTH_SENTINEL_PR77.to_string()),
                identity: "auth",
            },
            transport.clone(),
        );
        let endpoint = DynamicPathEndpoint {
            required: value.to_string(),
            optional: None,
            formatted: "tail".to_string(),
        };

        let err = client
            .request(endpoint)
            .response()
            .await
            .expect_err("invalid dynamic path segment should fail before transport");

        assert!(matches!(err, ApiClientError::InvalidParam { .. }));
        assert_eq!(err.category(), ErrorCategory::Config);
        assert_eq!(err.context().endpoint, "DynamicPath");
        assert_eq!(err.context().method, Method::GET);
        assert_eq!(transport.sent_count().await, 0);
        assert!(err.to_string().contains(label));
        assert_error_safe(&err);
        assert_observers_safe(&events).await;
    }
}

#[tokio::test]
async fn optional_empty_dynamic_path_segments_fail_while_none_still_omits_the_segment() {
    let invalid_transport = MockTransport::new(
        Arc::new(Mutex::new(Vec::new())),
        vec![MockResponse::text(StatusCode::OK, "ok")],
    );
    let invalid_client = client(
        TestAuthVars {
            token: Some(RAW_AUTH_SENTINEL_PR77.to_string()),
            identity: "auth",
        },
        invalid_transport.clone(),
    );
    let invalid_endpoint = DynamicPathEndpoint {
        required: "alpha".to_string(),
        optional: Some(String::new()),
        formatted: "tail".to_string(),
    };

    let invalid_err = invalid_client
        .request(invalid_endpoint)
        .response()
        .await
        .expect_err("empty optional segment should fail before transport");

    assert!(matches!(invalid_err, ApiClientError::InvalidParam { .. }));
    assert_eq!(invalid_err.context().endpoint, "DynamicPath");
    assert_eq!(invalid_transport.sent_count().await, 0);
    assert!(invalid_err.to_string().contains("ep.id"));
    assert_error_safe(&invalid_err);

    let valid_transport = MockTransport::new(
        Arc::new(Mutex::new(Vec::new())),
        vec![MockResponse::text(StatusCode::OK, "ok")],
    );
    let valid_client = client(TestAuthVars::default(), valid_transport.clone());
    let valid_endpoint = DynamicPathEndpoint {
        required: "alpha beta".to_string(),
        optional: None,
        formatted: "tail".to_string(),
    };

    let response = valid_client
        .request(valid_endpoint)
        .response()
        .await
        .expect("dynamic path should be percent-encoded and sent");

    assert_eq!(response.value(), "ok");
    assert_eq!(valid_transport.sent_count().await, 1);
    let requests = valid_transport.requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].url.path(), "/users/alpha%20beta/tail/posts");
}

#[tokio::test]
async fn formatted_empty_dynamic_path_segments_fail_before_transport() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "ok")],
    );
    let client = client(
        TestAuthVars {
            token: Some(RAW_AUTH_SENTINEL_PR77.to_string()),
            identity: "auth",
        },
        transport.clone(),
    );
    let endpoint = DynamicPathEndpoint {
        required: "alpha".to_string(),
        optional: Some("beta".to_string()),
        formatted: String::new(),
    };

    let err = client
        .request(endpoint)
        .response()
        .await
        .expect_err("empty formatted segment should fail before transport");

    assert!(matches!(err, ApiClientError::InvalidParam { .. }));
    assert_eq!(err.context().endpoint, "DynamicPath");
    assert_eq!(err.context().method, Method::GET);
    assert_eq!(transport.sent_count().await, 0);
    assert!(err.to_string().contains("fmt"));
    assert_error_safe(&err);
    assert_observers_safe(&events).await;
}

#[tokio::test]
async fn auth_collision_errors_are_typed_and_pre_rate_transport() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "ok")],
    );
    let mut client = client(
        TestAuthVars {
            token: Some(RAW_AUTH_SENTINEL_PR77.to_string()),
            identity: "auth",
        },
        transport.clone(),
    );
    with_runtime_observers(&mut client, events.clone());
    let mut policy = auth_policy(concord_core::advanced::AuthPlacement::Bearer);
    policy.rate_limit = rate_policy().rate_limit;
    policy.headers.insert(
        http::header::AUTHORIZATION,
        HeaderValue::from_static("public-value"),
    );

    let err = client
        .request(TextEndpoint {
            policy,
            ..TextEndpoint::default()
        })
        .response()
        .await
        .expect_err("auth collision should fail before side effects");

    assert!(matches!(err, ApiClientError::Auth { .. }));
    assert_eq!(err.category(), ErrorCategory::AuthRejected);
    assert_eq!(transport.sent_count().await, 0);
    let events = events.lock().await.clone();
    assert!(!events.iter().any(|event| event == "rate_acquire"));
    assert!(!events.iter().any(|event| event == "transport"));
    assert_error_safe(&err);
    assert_observers_safe(&Arc::new(Mutex::new(events))).await;
}

#[tokio::test]
async fn auth_rejection_error_is_distinct_from_status_retry() {
    for status in [StatusCode::UNAUTHORIZED, StatusCode::FORBIDDEN] {
        let events = Arc::new(Mutex::new(Vec::new()));
        let response_reads = Arc::new(AtomicUsize::new(0));
        let transport = MockTransport::new(
            events.clone(),
            vec![
                MockResponse::text(status, RESPONSE_BODY_SENTINEL_PR77)
                    .with_read_count(response_reads.clone()),
            ],
        );
        let mut client = client(
            TestAuthVars {
                token: Some(RAW_AUTH_SENTINEL_PR77.to_string()),
                identity: "auth",
            },
            transport.clone(),
        );
        with_runtime_observers(&mut client, events.clone());
        let mut policy = auth_policy(concord_core::advanced::AuthPlacement::Bearer);
        policy.rate_limit = rate_policy().rate_limit;
        policy.retry = retry_policy_for_statuses(2, vec![status]).retry;

        let err = client
            .request(TextEndpoint {
                policy,
                ..TextEndpoint::default()
            })
            .response()
            .await
            .expect_err("auth rejection should be terminal");

        assert!(matches!(err, ApiClientError::Auth { .. }));
        assert_eq!(err.category(), ErrorCategory::AuthRejected);
        assert_eq!(body_read_count(&response_reads), 0);
        assert_eq!(transport.sent_count().await, 1);
        let events = events.lock().await.clone();
        assert!(events.iter().any(|event| event == "rate_acquire"));
        assert!(!events.iter().any(|event| event.starts_with("retry_ctx")));
        assert_error_safe(&err);
        assert_observers_safe(&Arc::new(Mutex::new(events))).await;
    }
}

#[tokio::test]
async fn transport_error_is_distinct_from_http_status_error() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::with_outcomes(
        events.clone(),
        vec![MockOutcome::TransportError(TransportErrorKind::Connect)],
    );
    let mut client = client(TestAuthVars::default(), transport);
    with_runtime_observers(&mut client, events.clone());

    let err = client
        .request(TextEndpoint {
            policy: rate_policy(),
            ..TextEndpoint::default()
        })
        .response()
        .await
        .expect_err("transport error should surface");

    assert!(matches!(err, ApiClientError::Transport { .. }));
    assert_eq!(err.category(), ErrorCategory::Transport);
    let rendered = observed(&events).await;
    assert!(rendered.contains("transport_error"));
    assert!(!rendered.contains("rate_status"));
    assert_error_safe(&err);
    assert_observers_safe(&events).await;
}

#[tokio::test]
async fn response_body_read_transport_error_is_sanitized() {
    let transport = BodyReadFailingTransport::new();
    let sent = transport.clone();
    let client: ApiClient<TestCx, BodyReadFailingTransport> = ApiClient::with_transport(
        (),
        TestAuthVars {
            token: Some(RAW_AUTH_SENTINEL_PR77.to_string()),
            identity: "auth",
        },
        transport,
    );

    let mut plan = request_plan(
        "ResponseBodyRead",
        Method::POST,
        "/response-body-read",
        body_read_failure_policy(),
        None,
    );
    plan.endpoint.body = BodyPlan::Encoded {
        content_type: Some(HeaderValue::from_static("application/json")),
        format: Format::Text,
    };
    plan.args =
        RequestArgs::with_body_bytes(Bytes::from_static(REQUEST_BODY_SENTINEL_PR77.as_bytes()));

    let err = client
        .execute_plan::<concord_core::prelude::Text<String>>(plan)
        .await
        .expect_err("response body read failure should surface");

    assert!(matches!(err, ApiClientError::Transport { .. }));
    assert_eq!(err.category(), ErrorCategory::Transport);
    assert_eq!(err.context().endpoint, "ResponseBodyRead");
    assert_eq!(err.context().method, Method::POST);
    assert_eq!(sent.sent_count().await, 1);
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(
        request
            .headers
            .get(http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok()),
        Some("Bearer RAW_AUTH_SENTINEL_PR77")
    );
    assert_eq!(
        request
            .headers
            .get(http::HeaderName::from_static("x-response-body-read"))
            .and_then(|value| value.to_str().ok()),
        Some(HEADER_SECRET_SENTINEL_PR77)
    );
    assert_eq!(
        request.body.as_bytes().map(Bytes::as_ref),
        Some(REQUEST_BODY_SENTINEL_PR77.as_bytes())
    );
    let rendered = diagnostics(&err);
    assert!(rendered.contains("response body read failed"));
    assert!(!rendered.contains(RAW_AUTH_SENTINEL_PR77));
    assert!(!rendered.contains(REQUEST_BODY_SENTINEL_PR77));
    assert!(!rendered.contains(HEADER_SECRET_SENTINEL_PR77));
    assert!(!rendered.contains(RESPONSE_BODY_READ_SENTINEL_PR77));
    match &err {
        ApiClientError::Transport { source, .. } => {
            assert_eq!(source.kind(), TransportErrorKind::Request);
            assert!(source.to_string().contains("response body read failed"));
            assert!(
                !source
                    .to_string()
                    .contains(RESPONSE_BODY_READ_SENTINEL_PR77)
            );
            assert_text_does_not_contain_any(
                &format!("{source:?}\n{source:#?}"),
                &[RESPONSE_BODY_READ_SENTINEL_PR77],
            );
        }
        _ => panic!("expected transport error"),
    }
    assert_error_safe(&err);
}

#[tokio::test]
async fn http_status_error_is_distinct_from_transport_and_auth() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let response_reads = Arc::new(AtomicUsize::new(0));
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(
                StatusCode::INTERNAL_SERVER_ERROR,
                RESPONSE_BODY_SENTINEL_PR77,
            )
            .with_read_count(response_reads.clone()),
        ],
    );
    let mut client = client(TestAuthVars::default(), transport);
    with_runtime_observers(&mut client, events.clone());

    let err = client
        .request(TextEndpoint {
            policy: rate_policy(),
            ..TextEndpoint::default()
        })
        .response()
        .await
        .expect_err("500 should surface as status error");

    assert!(matches!(err, ApiClientError::HttpStatus { .. }));
    assert_eq!(err.category(), ErrorCategory::HttpStatus);
    assert_eq!(err.http_status(), Some(StatusCode::INTERNAL_SERVER_ERROR));
    assert_eq!(body_read_count(&response_reads), 0);
    let rendered = observed(&events).await;
    assert!(rendered.contains("rate_status:500 Internal Server Error"));
    assert_error_safe(&err);
    assert_observers_safe(&events).await;
}

#[tokio::test]
async fn http_status_error_sanitizes_public_headers_before_storage() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let mut response = MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, "status-body");
    response.headers.insert(
        http::header::SET_COOKIE,
        HeaderValue::from_static("session=LEAK_SENTINEL_COOKIE"),
    );
    response.headers.insert(
        http::header::WWW_AUTHENTICATE,
        HeaderValue::from_static("Bearer error_description=\"LEAK_SENTINEL_WWW_AUTH\""),
    );
    response.headers.insert(
        http::HeaderName::from_static("x-refresh-token"),
        HeaderValue::from_static("LEAK_SENTINEL_REFRESH"),
    );
    response.headers.insert(
        http::HeaderName::from_static("x-custom-session-token"),
        HeaderValue::from_static("LEAK_SENTINEL_CUSTOM"),
    );
    response.headers.insert(
        http::HeaderName::from_static("x-public"),
        HeaderValue::from_static("public-value"),
    );
    response
        .headers
        .insert(http::header::RETRY_AFTER, HeaderValue::from_static("1"));
    let transport = MockTransport::new(events.clone(), vec![response]);
    let mut client = client(TestAuthVars::default(), transport.clone());
    with_runtime_observers(&mut client, events.clone());

    let err = client
        .request(TextEndpoint {
            policy: rate_policy(),
            ..TextEndpoint::default()
        })
        .response()
        .await
        .expect_err("status error should surface");

    assert!(matches!(err, ApiClientError::HttpStatus { .. }));
    assert_eq!(err.category(), ErrorCategory::HttpStatus);
    assert_eq!(err.http_status(), Some(StatusCode::INTERNAL_SERVER_ERROR));
    assert_eq!(
        err.http_headers()
            .and_then(|headers| headers.get(http::header::SET_COOKIE))
            .and_then(|value| value.to_str().ok()),
        Some("<redacted>")
    );
    assert_eq!(
        err.http_headers()
            .and_then(|headers| headers.get(http::header::WWW_AUTHENTICATE))
            .and_then(|value| value.to_str().ok()),
        Some("<redacted>")
    );
    assert_eq!(
        err.http_headers()
            .and_then(|headers| headers.get(http::HeaderName::from_static("x-refresh-token")))
            .and_then(|value| value.to_str().ok()),
        Some("<redacted>")
    );
    assert_eq!(
        err.http_headers()
            .and_then(|headers| headers.get(http::HeaderName::from_static("x-custom-session-token")))
            .and_then(|value| value.to_str().ok()),
        Some("<redacted>")
    );
    assert_eq!(
        err.http_headers()
            .and_then(|headers| headers.get(http::HeaderName::from_static("x-public")))
            .and_then(|value| value.to_str().ok()),
        Some("public-value")
    );
    assert_eq!(
        err.http_headers()
            .and_then(|headers| headers.get(http::header::RETRY_AFTER))
            .and_then(|value| value.to_str().ok()),
        Some("1")
    );
    match &err {
        ApiClientError::HttpStatus { headers, .. } => {
            assert_eq!(
                headers
                    .get(http::header::SET_COOKIE)
                    .and_then(|value| value.to_str().ok()),
                Some("<redacted>")
            );
            assert_eq!(
                headers
                    .get(http::header::WWW_AUTHENTICATE)
                    .and_then(|value| value.to_str().ok()),
                Some("<redacted>")
            );
            assert_eq!(
                headers
                    .get(http::HeaderName::from_static("x-refresh-token"))
                    .and_then(|value| value.to_str().ok()),
                Some("<redacted>")
            );
            assert_eq!(
                headers
                    .get(http::HeaderName::from_static("x-custom-session-token"))
                    .and_then(|value| value.to_str().ok()),
                Some("<redacted>")
            );
            assert_eq!(
                headers
                    .get(http::HeaderName::from_static("x-public"))
                    .and_then(|value| value.to_str().ok()),
                Some("public-value")
            );
        }
        _ => unreachable!(),
    }

    let rendered = format!("{err}\n{err:?}\n{err:#?}");
    assert!(!rendered.contains("LEAK_SENTINEL_COOKIE"));
    assert!(!rendered.contains("LEAK_SENTINEL_WWW_AUTH"));
    assert!(!rendered.contains("LEAK_SENTINEL_REFRESH"));
    assert!(!rendered.contains("LEAK_SENTINEL_CUSTOM"));
    assert_error_safe(&err);
}

#[tokio::test]
async fn retry_exhaustion_returns_documented_final_error_and_safe_diagnostics() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, ""),
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, ""),
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, ""),
        ],
    );
    let mut client = client(TestAuthVars::default(), transport.clone());
    with_runtime_observers(&mut client, events.clone());
    let err = client
        .request(TextEndpoint {
            policy: retry_policy(3),
            ..TextEndpoint::default()
        })
        .response()
        .await
        .expect_err("retry exhaustion should return final status error");

    assert!(matches!(err, ApiClientError::HttpStatus { .. }));
    assert_eq!(err.category(), ErrorCategory::HttpStatus);
    assert_eq!(transport.sent_count().await, 3);
    assert_error_safe(&err);
    assert_observers_safe(&events).await;
}

#[tokio::test]
async fn rate_limit_acquire_error_is_typed_and_pre_transport() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "ok")],
    );
    let mut client = client(TestAuthVars::default(), transport.clone());
    client.configure(|cfg| {
        cfg.rate_limiter(Arc::new(FailingRateLimiter::new(events.clone())))
            .runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())))
            .debug_level(DebugLevel::VV)
            .debug_sink(Arc::new(CapturingDebugSink::new(events.clone())));
    });

    let err = client
        .request(TextEndpoint {
            policy: rate_policy(),
            ..TextEndpoint::default()
        })
        .response()
        .await
        .expect_err("rate limiter acquire should fail before transport");

    assert!(matches!(err, ApiClientError::RateLimit { .. }));
    assert_eq!(err.category(), ErrorCategory::RateLimit);
    assert_eq!(
        err.rate_limit_error().map(|err| err.kind()),
        Some(concord_core::advanced::RateLimitErrorKind::AcquireFailed)
    );
    assert_eq!(transport.sent_count().await, 0);
    let events = events.lock().await.clone();
    assert!(events.iter().any(|event| event == "rate_acquire"));
    assert!(!events.iter().any(|event| event == "transport"));
    assert_error_safe(&err);
    assert_observers_safe(&Arc::new(Mutex::new(events))).await;
}

#[tokio::test]
async fn rate_limit_response_action_error_is_typed_and_post_transport() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "ok")],
    );
    let mut client = client(TestAuthVars::default(), transport.clone());
    client.configure(|cfg| {
        cfg.rate_limiter(Arc::new(ResponseActionFailingRateLimiter::new(
            events.clone(),
        )))
        .runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())))
        .debug_level(DebugLevel::VV)
        .debug_sink(Arc::new(CapturingDebugSink::new(events.clone())));
    });

    let err = client
        .request(TextEndpoint::default())
        .response()
        .await
        .expect_err("rate limiter response action should fail after transport");

    assert!(matches!(err, ApiClientError::RateLimit { .. }));
    assert_eq!(err.category(), ErrorCategory::RateLimit);
    assert_eq!(
        err.rate_limit_error().map(|err| err.kind()),
        Some(concord_core::advanced::RateLimitErrorKind::ResponseActionFailed)
    );
    assert_eq!(transport.sent_count().await, 1);
    let events = events.lock().await.clone();
    assert!(events.iter().any(|event| event == "rate_response"));
    assert_error_safe(&err);
    assert_observers_safe(&Arc::new(Mutex::new(events))).await;
}

#[tokio::test]
async fn body_limit_errors_are_distinguishable_and_safe() {
    let content_length_reads = Arc::new(AtomicUsize::new(0));
    let streaming_reads = Arc::new(AtomicUsize::new(0));

    let cases = [
        (
            MockResponse::text(StatusCode::OK, RESPONSE_BODY_SENTINEL_PR77)
                .with_content_length(Some(5))
                .with_read_count(content_length_reads.clone()),
            "content-length",
        ),
        (
            MockResponse::text(StatusCode::OK, Bytes::new())
                .with_content_length(None)
                .with_chunks(vec![
                    Bytes::from_static(b"abcd"),
                    Bytes::from_static(RESPONSE_BODY_SENTINEL_PR77.as_bytes()),
                ])
                .with_read_count(streaming_reads.clone()),
            "streaming",
        ),
    ];

    for (response, name) in cases {
        let events = Arc::new(Mutex::new(Vec::new()));
        let transport = MockTransport::new(events.clone(), vec![response]);
        let mut client = client(TestAuthVars::default(), transport);
        with_runtime_observers(&mut client, events.clone());
        client.configure(|cfg| {
            cfg.max_response_body_bytes(4);
        });

        let err = match client
            .request(TextEndpoint {
                policy: rate_policy(),
                ..TextEndpoint::default()
            })
            .response()
            .await
        {
            Ok(_) => panic!("{name} over-limit response should fail"),
            Err(err) => err,
        };

        match name {
            "content-length" => assert!(matches!(err, ApiClientError::ResponseTooLarge { .. })),
            "streaming" => assert!(matches!(
                err,
                ApiClientError::ResponseBodyLimitExceeded { .. }
            )),
            _ => unreachable!(),
        }
        assert_eq!(err.category(), ErrorCategory::Decode);
        let events = events.lock().await.clone();
        assert_error_safe(&err);
        assert_observers_safe(&Arc::new(Mutex::new(events))).await;
    }

    assert_eq!(body_read_count(&content_length_reads), 0);
    assert_eq!(body_read_count(&streaming_reads), 2);
}

#[tokio::test]
async fn decode_errors_are_distinct_and_safe() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let mut invalid_utf8_payload = RESPONSE_BODY_SENTINEL_PR77.as_bytes().to_vec();
    invalid_utf8_payload.push(0xff);
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::OK, Bytes::from(invalid_utf8_payload)),
            MockResponse::text(StatusCode::OK, RESPONSE_BODY_SENTINEL_PR77),
        ],
    );
    let mut client = client(TestAuthVars::default(), transport);
    with_runtime_observers(&mut client, events.clone());
    client.configure(|cfg| {
        cfg.max_response_body_bytes(128);
    });

    let decode_err = client
        .request(TextEndpoint {
            policy: rate_policy(),
            ..TextEndpoint::default()
        })
        .response()
        .await
        .expect_err("invalid utf-8 under limit should decode-error");
    assert!(matches!(decode_err, ApiClientError::Decode { .. }));
    assert_eq!(decode_err.category(), ErrorCategory::Decode);
    assert_error_safe(&decode_err);

    let events = events.lock().await.clone();
    assert_observers_safe(&Arc::new(Mutex::new(events))).await;
}

#[tokio::test]
async fn pagination_error_is_distinct_and_safe() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::OK, "a,b|next=next-1"),
            MockResponse::text(StatusCode::OK, "c,d|next=next-1"),
        ],
    );
    let mut client = client(TestAuthVars::default(), transport.clone());
    with_runtime_observers(&mut client, events.clone());

    let err = client
        .request(CursorItemsEndpoint {
            policy: rate_policy(),
            pagination: PaginationVariant::cursor::<CursorItems>(CursorPagination {
                cursor: Some("start".to_string()),
                per_page: 2,
                send_cursor_on_first: true,
                stop_when_cursor_missing: true,
            }),
            ..Default::default()
        })
        .paginate(concord_core::prelude::PaginationTermination::hard_page_cap(
            100,
        ))
        .detect_loops(false)
        .collect()
        .await
        .expect_err("repeated cursor should be a pagination error");

    assert!(matches!(err, ApiClientError::Pagination { .. }));
    assert_eq!(err.category(), ErrorCategory::Pagination);
    assert_eq!(transport.sent_count().await, 2);
    assert_error_safe(&err);
    assert_observers_safe(&events).await;
}

#[cfg(feature = "dangerous-raw-response")]
#[tokio::test]
async fn execute_raw_error_taxonomy_matches_documented_behavior() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let response_reads = Arc::new(AtomicUsize::new(0));
    let transport = MockTransport::with_outcomes(
        events.clone(),
        vec![
            MockOutcome::TransportError(TransportErrorKind::Dns),
            MockResponse::text(StatusCode::OK, RESPONSE_BODY_SENTINEL_PR77)
                .with_content_length(Some(5))
                .with_read_count(response_reads.clone())
                .into(),
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, "").into(),
        ],
    );
    let mut raw_client = client(TestAuthVars::default(), transport.clone());
    with_runtime_observers(&mut raw_client, events.clone());
    raw_client.configure(|cfg| {
        cfg.max_response_body_bytes(4);
    });

    let transport_err = raw_client
        .request(TextEndpoint {
            policy: rate_policy(),
            ..TextEndpoint::default()
        })
        .execute_raw_response()
        .await
        .expect_err("raw transport error should surface as transport");
    assert!(matches!(transport_err, ApiClientError::Transport { .. }));
    assert_eq!(transport_err.category(), ErrorCategory::Transport);

    let body_err = raw_client
        .request(TextEndpoint {
            policy: rate_policy(),
            ..TextEndpoint::default()
        })
        .execute_raw_response()
        .await
        .expect_err("raw body limit should still be enforced");
    assert!(matches!(body_err, ApiClientError::ResponseTooLarge { .. }));
    assert_eq!(body_err.category(), ErrorCategory::Decode);
    assert_eq!(body_read_count(&response_reads), 0);

    let status_err = raw_client
        .request(TextEndpoint {
            policy: rate_policy(),
            ..TextEndpoint::default()
        })
        .execute_raw_response()
        .await
        .expect_err("raw HTTP status should surface as status error");
    assert!(matches!(status_err, ApiClientError::HttpStatus { .. }));
    assert_eq!(status_err.category(), ErrorCategory::HttpStatus);
    assert_eq!(
        status_err.http_status(),
        Some(StatusCode::INTERNAL_SERVER_ERROR)
    );

    let events = events.lock().await.clone();
    assert_error_safe(&transport_err);
    assert_error_safe(&body_err);
    assert_error_safe(&status_err);
    assert_observers_safe(&Arc::new(Mutex::new(events))).await;

    let rate_events = Arc::new(Mutex::new(Vec::new()));
    let rate_transport = MockTransport::new(
        rate_events.clone(),
        vec![MockResponse::text(StatusCode::OK, "unused")],
    );
    let mut rate_client = client(TestAuthVars::default(), rate_transport.clone());
    rate_client.configure(|cfg| {
        cfg.rate_limiter(Arc::new(FailingRateLimiter::new(rate_events.clone())))
            .runtime_hooks(Arc::new(ObservationRuntimeHooks::new(rate_events.clone())))
            .debug_level(DebugLevel::VV)
            .debug_sink(Arc::new(CapturingDebugSink::new(rate_events.clone())));
    });

    let rate_err = rate_client
        .request(TextEndpoint {
            policy: rate_policy(),
            ..TextEndpoint::default()
        })
        .execute_raw_response()
        .await
        .expect_err("raw rate-limit acquire failure should surface");

    assert!(matches!(rate_err, ApiClientError::RateLimit { .. }));
    assert_eq!(rate_err.category(), ErrorCategory::RateLimit);
    assert_eq!(
        rate_err.rate_limit_error().map(|err| err.kind()),
        Some(concord_core::advanced::RateLimitErrorKind::AcquireFailed)
    );
    assert_eq!(rate_transport.sent_count().await, 0);
    let rate_events = rate_events.lock().await.clone();
    assert!(rate_events.iter().any(|event| event == "rate_acquire"));
    assert!(!rate_events.iter().any(|event| event == "transport"));
    assert_error_safe(&rate_err);
    assert_observers_safe(&Arc::new(Mutex::new(rate_events))).await;
}

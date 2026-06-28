use super::common::*;
use bytes::Bytes;
use concord_core::advanced::{
    AuthPlacement, DebugSink, NoopRateLimiter, RetryContext, RetryDecision, RetryPolicy,
    TransportErrorKind,
};
use concord_core::internal::{
    BodyPlan, ClientPlanContext, EndpointMeta, EndpointPlan, RequestArgs, RequestOverrides,
    RequestPlan, ResolvedPolicy, ResolvedRoute, ResponsePlan,
};
use concord_core::prelude::{ApiClientError, DebugLevel, Endpoint};
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
use tokio::sync::Mutex;

tokio::task_local! {
    static ORDER_EVENTS: Arc<StdMutex<Vec<String>>>;
}

fn record_order_event(event: &str) {
    ORDER_EVENTS.with(|events| {
        events
            .lock()
            .expect("order events lock")
            .push(event.to_string());
    });
}

#[derive(Clone, Copy)]
enum MapMode {
    Fail,
}

#[derive(Clone)]
struct OrderingEndpoint {
    policy: ResolvedPolicy,
    map_mode: MapMode,
}

impl Endpoint<TestCx> for OrderingEndpoint {
    type Response = String;

    fn plan(&self, _ctx: &ClientPlanContext<'_, TestCx>) -> Result<RequestPlan, ApiClientError> {
        let decode = match self.map_mode {
            MapMode::Fail => decode_ordering_fail,
        };
        Ok(request_plan(
            "Ordering",
            Method::GET,
            "/ordering",
            self.policy.clone(),
            None,
            decode,
        ))
    }
}

#[derive(Clone)]
struct ObservationFailureEndpoint {
    policy: ResolvedPolicy,
    request_body: Bytes,
}

impl Endpoint<TestCx> for ObservationFailureEndpoint {
    type Response = String;

    fn plan(&self, _ctx: &ClientPlanContext<'_, TestCx>) -> Result<RequestPlan, ApiClientError> {
        Ok(RequestPlan {
            endpoint: EndpointPlan {
                meta: EndpointMeta {
                    name: "ObservationFailure",
                    method: Method::POST,
                    idempotent: false,
                    facade_path: &[],
                },
                route: ResolvedRoute::new(
                    http::uri::Scheme::HTTPS,
                    "example.com",
                    "/observation-failure",
                ),
                policy: self.policy.clone(),
                body: BodyPlan::Encoded {
                    content_type: Some(HeaderValue::from_static("application/json")),
                    format: concord_core::internal::Format::Text,
                },
                response: ResponsePlan {
                    accept: Some(HeaderValue::from_static("application/json")),
                    no_content: false,
                    format: concord_core::internal::Format::Text,
                    decode: decode_observation_failure,
                },
                pagination: None,
            },
            args: RequestArgs {
                body: Some(self.request_body.clone()),
            },
            overrides: RequestOverrides::default(),
        })
    }
}

fn decode_observation_failure(
    resp: concord_core::advanced::BuiltResponse,
    ctx: concord_core::advanced::ErrorContext,
) -> Result<Box<dyn std::any::Any + Send>, ApiClientError> {
    let content_type = resp
        .headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok());
    let _ = std::str::from_utf8(&resp.body)
        .map_err(|e| ApiClientError::decode_error(ctx.clone(), resp.status, content_type, e))?;
    Err(ApiClientError::decode_error(
        ctx,
        resp.status,
        content_type,
        std::io::Error::other("invalid JSON payload"),
    ))
}

fn decode_ordering_fail(
    resp: concord_core::advanced::BuiltResponse,
    ctx: concord_core::advanced::ErrorContext,
) -> Result<Box<dyn std::any::Any + Send>, ApiClientError> {
    record_order_event("decode");
    let content_type = resp
        .headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok());
    let value = std::str::from_utf8(&resp.body)
        .map(str::to_string)
        .map_err(|e| ApiClientError::decode_error(ctx.clone(), resp.status, content_type, e))?;
    record_order_event("map");
    Err(ApiClientError::Transform {
        ctx,
        source: std::io::Error::other(format!("mapping failed for {value}")).into(),
    })
}

fn positions(events: &[String], needle: &str) -> Vec<usize> {
    events
        .iter()
        .enumerate()
        .filter_map(|(idx, event)| (event == needle).then_some(idx))
        .collect()
}

fn first_position(events: &[String], needle: &str) -> usize {
    events
        .iter()
        .position(|event| event == needle)
        .unwrap_or_else(|| panic!("missing event `{needle}` in {events:?}"))
}

fn body_reads(counter: &Arc<AtomicUsize>) -> usize {
    counter.load(AtomicOrdering::SeqCst)
}

#[derive(Clone)]
struct UnsafeEndpoint {
    policy: ResolvedPolicy,
}

impl Endpoint<TestCx> for UnsafeEndpoint {
    type Response = String;

    fn plan(&self, _ctx: &ClientPlanContext<'_, TestCx>) -> Result<RequestPlan, ApiClientError> {
        Ok(RequestPlan {
            endpoint: EndpointPlan {
                meta: EndpointMeta {
                    name: "Unsafe",
                    method: Method::POST,
                    idempotent: false,
                    facade_path: &[],
                },
                route: ResolvedRoute::new(http::uri::Scheme::HTTPS, "example.com", "/unsafe"),
                policy: self.policy.clone(),
                body: BodyPlan::None,
                response: ResponsePlan {
                    accept: Some(http::HeaderValue::from_static("text/plain")),
                    no_content: false,
                    format: concord_core::internal::Format::Text,
                    decode: decode_string,
                },
                pagination: None,
            },
            args: RequestArgs::default(),
            overrides: RequestOverrides::default(),
        })
    }
}

#[derive(Clone)]
struct BodyDebugEndpoint {
    request_body: Bytes,
    policy: ResolvedPolicy,
}

impl Endpoint<TestCx> for BodyDebugEndpoint {
    type Response = String;

    fn plan(&self, _ctx: &ClientPlanContext<'_, TestCx>) -> Result<RequestPlan, ApiClientError> {
        Ok(RequestPlan {
            endpoint: EndpointPlan {
                meta: EndpointMeta {
                    name: "BodyDebug",
                    method: Method::POST,
                    idempotent: false,
                    facade_path: &[],
                },
                route: ResolvedRoute::new(http::uri::Scheme::HTTPS, "example.com", "/body-debug"),
                policy: self.policy.clone(),
                body: BodyPlan::Encoded {
                    content_type: Some(http::HeaderValue::from_static("text/plain")),
                    format: concord_core::internal::Format::Text,
                },
                response: ResponsePlan {
                    accept: Some(http::HeaderValue::from_static("text/plain")),
                    no_content: false,
                    format: concord_core::internal::Format::Text,
                    decode: decode_string,
                },
                pagination: None,
            },
            args: RequestArgs {
                body: Some(self.request_body.clone()),
            },
            overrides: RequestOverrides {
                debug_level: Some(DebugLevel::VV),
                ..Default::default()
            },
        })
    }
}

struct HugeDelayRetryPolicy;

impl RetryPolicy for HugeDelayRetryPolicy {
    fn max_retries(&self) -> u32 {
        1
    }

    fn should_retry(&self, _ctx: &RetryContext<'_>) -> RetryDecision {
        RetryDecision::RetryAfter(Duration::MAX)
    }
}

struct ZeroDelayRetryPolicy;

impl RetryPolicy for ZeroDelayRetryPolicy {
    fn max_retries(&self) -> u32 {
        1
    }

    fn should_retry(&self, _ctx: &RetryContext<'_>) -> RetryDecision {
        RetryDecision::RetryAfter(Duration::ZERO)
    }
}

#[tokio::test]
async fn retry_decision_happens_before_decode() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(
                StatusCode::INTERNAL_SERVER_ERROR,
                Bytes::from_static(b"\xff"),
            ),
            MockResponse::text(StatusCode::OK, "ok"),
        ],
    );
    let sent_transport = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let endpoint = TextEndpoint {
        policy: retry_policy(2),
        ..Default::default()
    };
    let decoded = client.request(endpoint).execute_decoded().await?;

    assert_eq!(decoded.value(), "ok");
    assert_eq!(sent_transport.sent_count().await, 2);
    Ok(())
}

#[tokio::test]
async fn custom_retry_policy_huge_retry_after_returns_typed_error() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, "retry-me"),
            MockResponse::text(StatusCode::OK, "should-not-send"),
        ],
    );
    let sent = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    client.set_retry_policy(Arc::new(HugeDelayRetryPolicy));

    let err = client
        .request(TextEndpoint::default())
        .execute_decoded()
        .await
        .expect_err("huge custom retry delay should be rejected before sleeping");

    assert_eq!(err.category(), concord_core::error::ErrorCategory::Config);
    assert!(err.to_string().contains("retry policy duration overflowed"));
    assert_eq!(sent.sent_count().await, 1);
}

#[tokio::test]
async fn custom_retry_policy_valid_retry_after_still_retries() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, "retry-me"),
            MockResponse::text(StatusCode::OK, "ok"),
        ],
    );
    let sent = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    client.set_retry_policy(Arc::new(ZeroDelayRetryPolicy));

    let decoded = client
        .request(TextEndpoint::default())
        .execute_decoded()
        .await?;

    assert_eq!(decoded.value(), "ok");
    assert_eq!(sent.sent_count().await, 2);
    Ok(())
}

#[tokio::test]
async fn custom_retry_policy_huge_delay_returns_typed_error() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let retry_events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, "retry-me"),
            MockResponse::text(StatusCode::OK, "should-not-send"),
        ],
    );
    let sent = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    client.set_retry_policy(Arc::new(RecordingRetryPolicy::new(
        retry_events.clone(),
        RetryDecision::RetryAfter(Duration::MAX),
        8,
    )));

    let err = client
        .request(TextEndpoint::default())
        .execute_decoded()
        .await
        .expect_err("huge custom retry delay should be rejected before sleeping");

    assert_eq!(err.category(), concord_core::error::ErrorCategory::Config);
    assert!(err.to_string().contains("retry policy duration overflowed"));
    assert_eq!(sent.sent_count().await, 1);
    let retry_events = retry_events.lock().await.clone();
    assert_eq!(retry_events.len(), 5);
    assert!(
        retry_events
            .iter()
            .any(|event| event.starts_with("retry_decision:RetryAfter"))
    );
}

#[tokio::test]
async fn custom_retry_policy_zero_delay_is_allowed_and_retries() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let retry_events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, "retry-me"),
            MockResponse::text(StatusCode::OK, "ok"),
        ],
    );
    let sent = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    client.set_retry_policy(Arc::new(RecordingRetryPolicy::new(
        retry_events.clone(),
        RetryDecision::RetryAfter(Duration::ZERO),
        8,
    )));

    let decoded = client
        .request(TextEndpoint::default())
        .execute_decoded()
        .await?;

    assert_eq!(decoded.value(), "ok");
    assert_eq!(sent.sent_count().await, 2);
    let retry_events = retry_events.lock().await.clone();
    assert!(
        retry_events
            .iter()
            .any(|event| event.starts_with("retry_decision:RetryAfter"))
    );
    Ok(())
}

#[tokio::test]
async fn custom_retry_policy_cannot_exceed_configured_max_attempts() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let retry_events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, "retry-1"),
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, "retry-2"),
            MockResponse::text(StatusCode::OK, "should-not-send"),
        ],
    );
    let sent = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    client.set_retry_policy(Arc::new(RecordingRetryPolicy::new(
        retry_events.clone(),
        RetryDecision::Retry,
        1,
    )));

    let err = client
        .request(TextEndpoint::default())
        .execute_decoded()
        .await
        .expect_err("retry budget exhaustion should stop after one retry");

    assert!(matches!(
        err,
        ApiClientError::HttpStatus {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            ..
        }
    ));
    assert_eq!(sent.sent_count().await, 2);
    let retry_events = retry_events.lock().await.clone();
    assert_eq!(positions(&retry_events, "retry_decision:Retry").len(), 1);
}

#[tokio::test]
async fn custom_retry_decision_happens_after_hook_and_rate_limit_observation()
-> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let limiter = Arc::new(ObservationRateLimiter::new(events.clone()));
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, "retry-me"),
            MockResponse::text(StatusCode::OK, "ok"),
        ],
    );
    let sent = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    client.set_runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())));
    configure_runtime(&mut client, Some(limiter));
    client.set_retry_policy(Arc::new(RecordingRetryPolicy::new(
        events.clone(),
        RetryDecision::Retry,
        8,
    )));

    let decoded = client
        .request(TextEndpoint::default())
        .execute_decoded()
        .await?;

    assert_eq!(decoded.value(), "ok");
    assert_eq!(sent.sent_count().await, 2);
    let events = events.lock().await.clone();
    let hook = first_event_with_prefix(&events, "hook_status:500 Internal Server Error");
    let rate = first_event_with_prefix(&events, "rate_status:500 Internal Server Error");
    let retry = first_event_with_prefix(&events, "retry_decision:Retry");
    let second_send = positions(&events, "transport")[1];
    assert!(hook < rate);
    assert!(rate < retry);
    assert!(retry < second_send);
    assert!(!events.iter().any(|event| event.contains("PR66_")));
    Ok(())
}

#[tokio::test]
async fn configured_transport_error_kind_retries_then_succeeds() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::with_outcomes(
        events,
        vec![
            MockOutcome::TransportError(TransportErrorKind::Timeout),
            MockResponse::text(StatusCode::OK, "ok").into(),
        ],
    );
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let decoded = client
        .request(TextEndpoint {
            policy: retry_policy_for_transport_errors(2, vec![TransportErrorKind::Timeout]),
            ..Default::default()
        })
        .execute_decoded()
        .await?;

    assert_eq!(decoded.value(), "ok");
    assert_eq!(sent.sent_count().await, 2);
    Ok(())
}

#[tokio::test]
async fn unconfigured_transport_error_kind_does_not_retry() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::with_outcomes(
        events,
        vec![
            MockOutcome::TransportError(TransportErrorKind::Connect),
            MockResponse::text(StatusCode::OK, "should-not-send").into(),
        ],
    );
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let err = client
        .request(TextEndpoint {
            policy: retry_policy_for_transport_errors(2, vec![TransportErrorKind::Timeout]),
            ..Default::default()
        })
        .execute_decoded()
        .await
        .expect_err("unconfigured transport error kind should not retry");

    assert!(err.to_string().contains("transport"));
    assert_eq!(sent.sent_count().await, 1);
}

#[tokio::test]
async fn transport_error_retry_budget_exhaustion_returns_final_typed_error() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::with_outcomes(
        events,
        vec![
            MockOutcome::TransportError(TransportErrorKind::Timeout),
            MockOutcome::TransportError(TransportErrorKind::Timeout),
        ],
    );
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let err = client
        .request(TextEndpoint {
            policy: retry_policy_for_transport_errors(2, vec![TransportErrorKind::Timeout]),
            ..Default::default()
        })
        .execute_decoded()
        .await
        .expect_err("retry budget exhaustion should return final transport error");

    assert_eq!(err.category(), concord_core::error::ErrorCategory::Timeout);
    assert_eq!(sent.sent_count().await, 2);
}

#[tokio::test]
async fn unsafe_method_without_idempotency_header_does_not_retry() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, "do-not-retry"),
            MockResponse::text(StatusCode::OK, "should-not-send"),
        ],
    );
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);
    let policy = ResolvedPolicy {
        retry: concord_core::internal::RetrySetting::Config(concord_core::advanced::RetryConfig {
            max_attempts: 2,
            methods: vec![Method::POST],
            statuses: vec![StatusCode::INTERNAL_SERVER_ERROR],
            transport_errors: Vec::new(),
            backoff: concord_core::advanced::RetryBackoff::None,
            respect_retry_after: false,
            idempotency: concord_core::advanced::RetryIdempotency::SafeMethodsOnly,
        }),
        ..Default::default()
    };

    let err = client
        .request(UnsafeEndpoint { policy })
        .execute_decoded()
        .await
        .expect_err("unsafe request without idempotency signal should not retry");

    assert!(err.to_string().contains("500"));
    assert_eq!(sent.sent_count().await, 1);
}

#[tokio::test]
async fn unsafe_method_with_idempotency_header_retries_with_stable_value()
-> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, "retry-me"),
            MockResponse::text(StatusCode::OK, "ok"),
        ],
    );
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);
    let header = http::HeaderName::from_static("idempotency-key");
    let mut headers = HeaderMap::new();
    headers.insert(header.clone(), http::HeaderValue::from_static("stable-key"));
    let policy = ResolvedPolicy {
        headers,
        retry: concord_core::internal::RetrySetting::Config(concord_core::advanced::RetryConfig {
            max_attempts: 2,
            methods: vec![Method::POST],
            statuses: vec![StatusCode::INTERNAL_SERVER_ERROR],
            transport_errors: Vec::new(),
            backoff: concord_core::advanced::RetryBackoff::None,
            respect_retry_after: false,
            idempotency: concord_core::advanced::RetryIdempotency::Header(header.clone()),
        }),
        ..Default::default()
    };

    let decoded = client
        .request(UnsafeEndpoint { policy })
        .execute_decoded()
        .await?;

    assert_eq!(decoded.value(), "ok");
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0]
            .headers
            .get(&header)
            .and_then(|v| v.to_str().ok()),
        Some("stable-key")
    );
    assert_eq!(
        requests[1]
            .headers
            .get(&header)
            .and_then(|v| v.to_str().ok()),
        Some("stable-key")
    );
    Ok(())
}

#[tokio::test]
async fn rate_limit_acquire_runs_before_each_transport_attempt() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let limiter = Arc::new(RecordingRateLimiter::new(events.clone()));
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, "retry-me"),
            MockResponse::text(StatusCode::OK, "ok"),
        ],
    );
    let sent_transport = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    client.set_runtime_hooks(Arc::new(RecordingRuntimeHooks::new(events.clone())));
    configure_runtime(&mut client, Some(limiter));

    let decoded = client
        .request(TextEndpoint {
            policy: retry_policy(2),
            ..Default::default()
        })
        .execute_decoded()
        .await?;

    assert_eq!(decoded.value(), "ok");
    assert_eq!(sent_transport.sent_count().await, 2);
    let events = events.lock().await.clone();
    let rate_acquires = positions(&events, "rate_acquire");
    let transports = positions(&events, "transport");
    assert_eq!(rate_acquires.len(), 2);
    assert_eq!(transports.len(), 2);
    assert!(rate_acquires[0] < transports[0]);
    assert!(rate_acquires[1] < transports[1]);
    Ok(())
}

#[tokio::test]
async fn rate_limit_observation_runs_before_retry_decision() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let limiter = Arc::new(RecordingRateLimiter::new(events.clone()));
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::TOO_MANY_REQUESTS, "slow-down"),
            MockResponse::text(StatusCode::OK, "ok"),
        ],
    );
    let mut client = client(TestAuthVars::default(), transport);
    client.set_runtime_hooks(Arc::new(RecordingRuntimeHooks::new(events.clone())));
    configure_runtime(&mut client, Some(limiter));

    let decoded = client
        .request(TextEndpoint {
            policy: retry_policy_for_statuses(2, vec![StatusCode::TOO_MANY_REQUESTS]),
            ..Default::default()
        })
        .execute_decoded()
        .await?;

    assert_eq!(decoded.value(), "ok");
    let events = events.lock().await.clone();
    let post_response = first_position(&events, "classify_response");
    let first_observe = first_position(&events, "rate_response");
    let second_acquire = positions(&events, "rate_acquire")[1];
    assert!(post_response < first_observe);
    assert!(first_observe < second_acquire);
    Ok(())
}

fn first_event_with_prefix(events: &[String], prefix: &str) -> usize {
    events
        .iter()
        .position(|event| event.starts_with(prefix))
        .unwrap_or_else(|| panic!("missing event prefix `{prefix}` in {events:?}"))
}

#[tokio::test]
async fn runtime_hooks_observe_200_before_decode_failure() {
    const REQUEST_SENTINEL: &str = "PR65_REQUEST_BODY_SENTINEL_DO_NOT_LEAK";
    const RESPONSE_SENTINEL: &str = "PR65_RESPONSE_BODY_SENTINEL_DO_NOT_LEAK";

    let events = Arc::new(Mutex::new(Vec::new()));
    let mut response = MockResponse::text(StatusCode::OK, RESPONSE_SENTINEL);
    response.headers.insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    let transport = MockTransport::new(events.clone(), vec![response]);
    let mut client = client(TestAuthVars::default(), transport);
    client.set_runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())));
    configure_runtime(&mut client, None);

    let err = client
        .request(ObservationFailureEndpoint {
            policy: ResolvedPolicy::default(),
            request_body: Bytes::from_static(REQUEST_SENTINEL.as_bytes()),
        })
        .execute_decoded()
        .await
        .expect_err("invalid payload should fail decode");

    assert_eq!(err.category(), concord_core::error::ErrorCategory::Decode);
    assert!(err.to_string().contains("decode error"));
    let events = events.lock().await.clone();
    assert!(
        events
            .iter()
            .any(|event| event.starts_with("hook_status:200 OK"))
    );
    assert!(!events.iter().any(|event| event.contains("PR65_")));
    assert!(
        events
            .iter()
            .any(|event| event.starts_with("hook_headers:"))
    );
    assert!(!events.iter().any(|event| event.contains(REQUEST_SENTINEL)));
    assert!(!events.iter().any(|event| event.contains(RESPONSE_SENTINEL)));
}

#[tokio::test]
async fn runtime_hooks_observe_retryable_status_before_retry() -> Result<(), ApiClientError> {
    const FIRST_SENTINEL: &str = "PR65_FIRST_RETRYABLE_BODY_DO_NOT_LEAK";
    const SECOND_SENTINEL: &str = "PR65_SECOND_RETRYABLE_BODY_DO_NOT_LEAK";

    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, FIRST_SENTINEL),
            MockResponse::text(StatusCode::OK, SECOND_SENTINEL),
        ],
    );
    let sent_transport = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    client.set_runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())));

    let decoded = client
        .request(TextEndpoint {
            policy: retry_policy(2),
            ..Default::default()
        })
        .execute_decoded()
        .await?;

    assert_eq!(decoded.value(), SECOND_SENTINEL);
    assert_eq!(sent_transport.sent_count().await, 2);
    let events = events.lock().await.clone();
    let first = first_event_with_prefix(&events, "hook_status:500 Internal Server Error");
    let second = first_event_with_prefix(&events, "hook_status:200 OK");
    assert!(first < second);
    assert!(!events.iter().any(|event| event.contains(FIRST_SENTINEL)));
    assert!(!events.iter().any(|event| event.contains(SECOND_SENTINEL)));
    Ok(())
}

#[tokio::test]
async fn runtime_hooks_observe_auth_rejection_before_auth_handling() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::UNAUTHORIZED, "unauthorized"),
            MockResponse::text(StatusCode::OK, "recovered"),
        ],
    );
    let sent_transport = transport.clone();
    let mut client = concord_core::prelude::ApiClient::<ObservationAuthCx, _>::with_transport(
        (),
        ObservationAuthVars::bearer("PR65_BEARER_SECRET_DO_NOT_LEAK", "refresh", events.clone()),
        transport,
    );
    client.set_runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())));

    let decoded = client
        .request(TextEndpoint {
            policy: auth_policy(AuthPlacement::Bearer),
            ..Default::default()
        })
        .execute_decoded()
        .await?;

    assert_eq!(decoded.value(), "recovered");
    assert_eq!(sent_transport.sent_count().await, 2);
    let events = events.lock().await.clone();
    let hook = first_event_with_prefix(&events, "hook_status:401 Unauthorized");
    let auth = first_position(&events, "auth_rejection:401 Unauthorized");
    assert!(hook < auth);
    assert!(events.iter().any(|event| event.starts_with("hook_meta:")));
    assert!(
        events
            .iter()
            .any(|event| event.starts_with("hook_headers:"))
    );
    assert!(events.iter().any(|event| event == "auth_retry"));
    assert!(
        !events
            .iter()
            .any(|event| event.contains("PR65_BEARER_SECRET_DO_NOT_LEAK"))
    );
    Ok(())
}

#[tokio::test]
async fn auth_rejection_preempts_custom_retry_policy_for_401() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let retry_events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::UNAUTHORIZED, "unauthorized"),
            MockResponse::text(StatusCode::OK, "recovered"),
        ],
    );
    let sent_transport = transport.clone();
    let mut client = concord_core::prelude::ApiClient::<ObservationAuthCx, _>::with_transport(
        (),
        ObservationAuthVars::bearer("PR66_BEARER_401_DO_NOT_LEAK", "refresh", events.clone()),
        transport,
    );
    client.set_runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())));
    configure_runtime(
        &mut client,
        Some(Arc::new(ObservationRateLimiter::new(events.clone()))),
    );
    client.set_retry_policy(Arc::new(RecordingRetryPolicy::new(
        retry_events.clone(),
        RetryDecision::Retry,
        8,
    )));

    let decoded = client
        .request(TextEndpoint {
            policy: auth_policy(AuthPlacement::Bearer),
            ..Default::default()
        })
        .execute_decoded()
        .await?;

    assert_eq!(decoded.value(), "recovered");
    assert_eq!(sent_transport.sent_count().await, 2);
    let events = events.lock().await.clone();
    let hook = first_event_with_prefix(&events, "hook_status:401 Unauthorized");
    let rate = first_event_with_prefix(&events, "rate_status:401 Unauthorized");
    let auth = first_position(&events, "auth_rejection:401 Unauthorized");
    assert!(hook < rate);
    assert!(rate < auth);
    assert!(events.iter().any(|event| event == "auth_retry"));
    assert!(retry_events.lock().await.is_empty());
    Ok(())
}

#[tokio::test]
async fn auth_rejection_preempts_custom_retry_policy_for_403() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let retry_events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::FORBIDDEN, "forbidden"),
            MockResponse::text(StatusCode::OK, "recovered"),
        ],
    );
    let sent_transport = transport.clone();
    let mut client = concord_core::prelude::ApiClient::<ObservationAuthCx, _>::with_transport(
        (),
        ObservationAuthVars::bearer("PR66_BEARER_403_DO_NOT_LEAK", "refresh", events.clone()),
        transport,
    );
    client.set_runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())));
    configure_runtime(
        &mut client,
        Some(Arc::new(ObservationRateLimiter::new(events.clone()))),
    );
    client.set_retry_policy(Arc::new(RecordingRetryPolicy::new(
        retry_events.clone(),
        RetryDecision::Retry,
        8,
    )));

    let decoded = client
        .request(TextEndpoint {
            policy: auth_policy(AuthPlacement::Bearer),
            ..Default::default()
        })
        .execute_decoded()
        .await?;

    assert_eq!(decoded.value(), "recovered");
    assert_eq!(sent_transport.sent_count().await, 2);
    let events = events.lock().await.clone();
    let hook = first_event_with_prefix(&events, "hook_status:403 Forbidden");
    let rate = first_event_with_prefix(&events, "rate_status:403 Forbidden");
    let auth = first_position(&events, "auth_rejection:403 Forbidden");
    assert!(hook < rate);
    assert!(rate < auth);
    assert!(events.iter().any(|event| event == "auth_retry"));
    assert!(retry_events.lock().await.is_empty());
    Ok(())
}

#[tokio::test]
async fn never_refresh_auth_rejection_does_not_fall_through_to_custom_retry() {
    for status in [StatusCode::UNAUTHORIZED, StatusCode::FORBIDDEN] {
        let events = Arc::new(Mutex::new(Vec::new()));
        let retry_events = Arc::new(Mutex::new(Vec::new()));
        let transport =
            MockTransport::new(events.clone(), vec![MockResponse::text(status, "rejected")]);
        let sent_transport = transport.clone();
        let mut client = concord_core::prelude::ApiClient::<ObservationAuthCx, _>::with_transport(
            (),
            ObservationAuthVars::bearer(
                "PR66_BEARER_REJECTION_DO_NOT_LEAK",
                "user-a",
                events.clone(),
            ),
            transport,
        );
        client.set_runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())));
        configure_runtime(
            &mut client,
            Some(Arc::new(ObservationRateLimiter::new(events.clone()))),
        );
        client.set_retry_policy(Arc::new(RecordingRetryPolicy::new(
            retry_events.clone(),
            RetryDecision::Retry,
            8,
        )));

        let err = client
            .request(TextEndpoint {
                policy: {
                    let mut policy = auth_policy(AuthPlacement::Bearer);
                    policy.auth.requirements[0].challenge =
                        concord_core::advanced::AuthChallengePolicy::NeverRefresh;
                    policy.retry = retry_policy_for_statuses(8, vec![status]).retry;
                    policy
                },
                ..Default::default()
            })
            .execute_decoded()
            .await
            .expect_err("terminal auth rejection should win");

        assert!(err.to_string().contains("auth challenge rejected"));
        assert_eq!(sent_transport.sent_count().await, 1);
        let events = events.lock().await.clone();
        assert!(events.iter().any(|event| event.starts_with("rate_status:")));
        assert!(events.iter().any(|event| event.starts_with("auth_fail")));
        assert!(retry_events.lock().await.is_empty());
    }
}

#[tokio::test]
async fn auth_rejection_does_not_read_body() -> Result<(), ApiClientError> {
    const RESPONSE_SENTINEL: &str = "PR74_AUTH_REJECTION_BODY_SENTINEL";

    for status in [StatusCode::UNAUTHORIZED, StatusCode::FORBIDDEN] {
        let events = Arc::new(Mutex::new(Vec::new()));
        let read_count = Arc::new(AtomicUsize::new(0));
        let transport = MockTransport::new(
            events.clone(),
            vec![
                MockResponse::text(status, RESPONSE_SENTINEL)
                    .with_content_length(None)
                    .with_chunks(vec![
                        Bytes::from_static(b"abcd"),
                        Bytes::from_static(RESPONSE_SENTINEL.as_bytes()),
                    ])
                    .with_read_count(read_count.clone()),
            ],
        );
        let sent_transport = transport.clone();
        let mut client = concord_core::prelude::ApiClient::<ObservationAuthCx, _>::with_transport(
            (),
            ObservationAuthVars::bearer(
                "PR74_AUTH_REJECTION_BODY_SENTINEL_AUTH",
                "user-a",
                events.clone(),
            ),
            transport,
        );
        client.set_runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())));
        configure_runtime(
            &mut client,
            Some(Arc::new(ObservationRateLimiter::new(events.clone()))),
        );

        let err = client
            .request(TextEndpoint {
                policy: auth_policy(AuthPlacement::Bearer),
                ..Default::default()
            })
            .execute_decoded()
            .await
            .expect_err("auth rejection should happen before body read");

        assert!(matches!(err, ApiClientError::Auth { .. }));
        assert!(err.to_string().contains("auth challenge rejected"));
        assert_eq!(sent_transport.sent_count().await, 1);
        assert_eq!(body_reads(&read_count), 0);
        let events = events.lock().await.clone();
        assert!(events.iter().any(|event| event.starts_with("hook_meta:")));
        assert!(events.iter().any(|event| event.starts_with("rate_meta:")));
        assert!(events.iter().any(|event| event.starts_with("hook_status:")));
        assert!(events.iter().any(|event| event.starts_with("rate_status:")));
        assert!(!events.iter().any(|event| event.contains(RESPONSE_SENTINEL)));
        assert!(!format!("{events:?}").contains(RESPONSE_SENTINEL));
    }

    Ok(())
}

#[tokio::test]
async fn runtime_hooks_do_not_observe_body_on_http_status_error() {
    const RESPONSE_SENTINEL: &str = "PR65_RESPONSE_BODY_SENTINEL_DO_NOT_LEAK";

    let events = Arc::new(Mutex::new(Vec::new()));
    let mut response = MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, RESPONSE_SENTINEL);
    response.headers.insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    let transport = MockTransport::new(events.clone(), vec![response]);
    let mut client = client(TestAuthVars::default(), transport);
    client.set_runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())));

    let err = client
        .request(TextEndpoint::default())
        .execute_decoded()
        .await
        .expect_err("HTTP status error should remain terminal");

    assert!(matches!(err, ApiClientError::HttpStatus { .. }));
    let events = events.lock().await.clone();
    assert!(
        events
            .iter()
            .any(|event| event.starts_with("hook_status:500 Internal Server Error"))
    );
    assert!(events.iter().any(|event| event.starts_with("hook_meta:")));
    assert!(
        events
            .iter()
            .any(|event| event.starts_with("hook_headers:"))
    );
    assert!(!events.iter().any(|event| event.contains(RESPONSE_SENTINEL)));
}

#[tokio::test]
async fn transport_observation_does_not_leak_basic_auth_material() -> Result<(), ApiClientError> {
    const BASIC_USERNAME: &str = "PR65_BASIC_USERNAME_DO_NOT_LEAK";
    const BASIC_PASSWORD: &str = "PR65_BASIC_PASSWORD_DO_NOT_LEAK";

    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "basic-ok")],
    );
    let mut client = concord_core::prelude::ApiClient::<ObservationAuthCx, _>::with_transport(
        (),
        ObservationAuthVars::basic(BASIC_USERNAME, BASIC_PASSWORD, "basic", events.clone()),
        transport,
    );
    client.set_runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())));
    configure_runtime(
        &mut client,
        Some(Arc::new(ObservationRateLimiter::new(events.clone()))),
    );

    let decoded = client
        .request(TextEndpoint {
            policy: auth_policy(AuthPlacement::Basic),
            ..Default::default()
        })
        .execute_decoded()
        .await?;

    assert_eq!(decoded.value(), "basic-ok");
    let events = events.lock().await.clone();
    assert!(
        events
            .iter()
            .any(|event| event.starts_with("hook_status:200 OK"))
    );
    assert!(
        events
            .iter()
            .any(|event| event.starts_with("rate_status:200 OK"))
    );
    assert!(events.iter().any(|event| event.starts_with("hook_meta:")));
    assert!(
        events
            .iter()
            .any(|event| event.starts_with("hook_headers:"))
    );
    assert!(events.iter().any(|event| event.starts_with("rate_meta:")));
    assert!(
        events
            .iter()
            .any(|event| event.starts_with("rate_headers:"))
    );
    assert!(!events.iter().any(|event| event.contains(BASIC_USERNAME)));
    assert!(!events.iter().any(|event| event.contains(BASIC_PASSWORD)));
    Ok(())
}

#[tokio::test]
async fn custom_retry_context_does_not_expose_bearer_auth() {
    const BEARER_SECRET: &str = "PR66_BEARER_SECRET_DO_NOT_LEAK";
    const RESPONSE_SENTINEL: &str = "PR66_RESPONSE_BODY_SENTINEL_DO_NOT_LEAK";

    let events = Arc::new(Mutex::new(Vec::new()));
    let retry_events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![MockResponse::text(
            StatusCode::INTERNAL_SERVER_ERROR,
            RESPONSE_SENTINEL,
        )],
    );
    let mut client = concord_core::prelude::ApiClient::<ObservationAuthCx, _>::with_transport(
        (),
        ObservationAuthVars::bearer(BEARER_SECRET, "user-a", Arc::new(Mutex::new(Vec::new()))),
        transport,
    );
    client.set_retry_policy(Arc::new(RecordingRetryPolicy::new(
        retry_events.clone(),
        RetryDecision::Stop,
        8,
    )));

    let err = client
        .request(TextEndpoint {
            policy: auth_policy(AuthPlacement::Bearer),
            ..Default::default()
        })
        .execute_decoded()
        .await
        .expect_err("http status error should remain terminal");

    assert!(matches!(err, ApiClientError::HttpStatus { .. }));
    let retry_events = retry_events.lock().await.clone();
    assert!(
        retry_events
            .iter()
            .any(|event| event.starts_with("retry_ctx:"))
    );
    assert!(
        !retry_events
            .iter()
            .any(|event| event.contains(BEARER_SECRET))
    );
    assert!(
        !retry_events
            .iter()
            .any(|event| event.contains(RESPONSE_SENTINEL))
    );
}

#[tokio::test]
async fn custom_retry_context_does_not_expose_query_auth() {
    const QUERY_SECRET: &str = "PR66_QUERY_SECRET_DO_NOT_LEAK";
    const RESPONSE_SENTINEL: &str = "PR66_RESPONSE_BODY_SENTINEL_DO_NOT_LEAK";

    let events = Arc::new(Mutex::new(Vec::new()));
    let retry_events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![MockResponse::text(
            StatusCode::INTERNAL_SERVER_ERROR,
            RESPONSE_SENTINEL,
        )],
    );
    let mut client = concord_core::prelude::ApiClient::<ObservationAuthCx, _>::with_transport(
        (),
        ObservationAuthVars::bearer(QUERY_SECRET, "user-a", Arc::new(Mutex::new(Vec::new()))),
        transport,
    );
    client.set_retry_policy(Arc::new(RecordingRetryPolicy::new(
        retry_events.clone(),
        RetryDecision::Stop,
        8,
    )));

    let err = client
        .request(TextEndpoint {
            policy: auth_policy(AuthPlacement::Query("api_key")),
            ..Default::default()
        })
        .execute_decoded()
        .await
        .expect_err("http status error should remain terminal");

    assert!(matches!(err, ApiClientError::HttpStatus { .. }));
    let retry_events = retry_events.lock().await.clone();
    assert!(
        retry_events
            .iter()
            .any(|event| event.starts_with("retry_ctx:"))
    );
    assert!(
        !retry_events
            .iter()
            .any(|event| event.contains(QUERY_SECRET))
    );
    assert!(
        !retry_events
            .iter()
            .any(|event| event.contains(RESPONSE_SENTINEL))
    );
}

#[tokio::test]
async fn custom_retry_context_does_not_expose_basic_auth_material() {
    const BASIC_USERNAME: &str = "PR66_BASIC_USERNAME_DO_NOT_LEAK";
    const BASIC_PASSWORD: &str = "PR66_BASIC_PASSWORD_DO_NOT_LEAK";
    const RESPONSE_SENTINEL: &str = "PR66_RESPONSE_BODY_SENTINEL_DO_NOT_LEAK";

    let events = Arc::new(Mutex::new(Vec::new()));
    let retry_events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![MockResponse::text(
            StatusCode::INTERNAL_SERVER_ERROR,
            RESPONSE_SENTINEL,
        )],
    );
    let mut client = concord_core::prelude::ApiClient::<ObservationAuthCx, _>::with_transport(
        (),
        ObservationAuthVars::basic(
            BASIC_USERNAME,
            BASIC_PASSWORD,
            "basic",
            Arc::new(Mutex::new(Vec::new())),
        ),
        transport,
    );
    client.set_retry_policy(Arc::new(RecordingRetryPolicy::new(
        retry_events.clone(),
        RetryDecision::Stop,
        8,
    )));

    let err = client
        .request(TextEndpoint {
            policy: auth_policy(AuthPlacement::Basic),
            ..Default::default()
        })
        .execute_decoded()
        .await
        .expect_err("http status error should remain terminal");

    assert!(matches!(err, ApiClientError::HttpStatus { .. }));
    let retry_events = retry_events.lock().await.clone();
    assert!(
        retry_events
            .iter()
            .any(|event| event.starts_with("retry_ctx:"))
    );
    assert!(
        !retry_events
            .iter()
            .any(|event| event.contains(BASIC_USERNAME))
    );
    assert!(
        !retry_events
            .iter()
            .any(|event| event.contains(BASIC_PASSWORD))
    );
    assert!(
        !retry_events
            .iter()
            .any(|event| event.contains(RESPONSE_SENTINEL))
    );
}

#[tokio::test]
async fn oversized_live_body_fails_typed() {
    const LIVE_SENTINEL: &str = "PR74_OVERSIZED_LIVE_BODY_SENTINEL";

    let events = Arc::new(Mutex::new(Vec::new()));
    let read_count = Arc::new(AtomicUsize::new(0));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, Bytes::new())
                .with_content_length(None)
                .with_chunks(vec![
                    Bytes::from_static(b"abcd"),
                    Bytes::from_static(LIVE_SENTINEL.as_bytes()),
                ])
                .with_read_count(read_count.clone()),
        ],
    );
    let sent_transport = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    configure_runtime(&mut client, None);
    client.configure(|cfg| {
        cfg.max_response_body_bytes(4);
    });

    let err = client
        .request(TextEndpoint::default())
        .execute_decoded()
        .await
        .expect_err("oversized live body should fail typed");

    assert!(matches!(
        err,
        ApiClientError::ResponseBodyLimitExceeded { limit: 4, .. }
    ));
    assert_eq!(sent_transport.sent_count().await, 1);
    assert_eq!(body_reads(&read_count), 2);
    assert!(!err.to_string().contains(LIVE_SENTINEL));
}

#[tokio::test]
async fn custom_retry_policy_not_invoked_for_decode_failure() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let retry_events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::OK, Bytes::from_static(b"\xff")),
            MockResponse::text(StatusCode::OK, "should-not-send"),
        ],
    );
    let sent_transport = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    configure_runtime(&mut client, None);
    client.set_retry_policy(Arc::new(RecordingRetryPolicy::new(
        retry_events.clone(),
        RetryDecision::Retry,
        8,
    )));

    let err = client
        .request(TextEndpoint {
            policy: ResolvedPolicy::default(),
            ..Default::default()
        })
        .execute_decoded()
        .await
        .expect_err("decode failure should be terminal");

    assert_eq!(err.category(), concord_core::error::ErrorCategory::Decode);
    assert_eq!(sent_transport.sent_count().await, 1);
    assert!(retry_events.lock().await.is_empty());
}

#[tokio::test]
async fn decode_failure_does_not_consume_retry_budget() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let retry_events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::OK, Bytes::from_static(b"\xff")),
            MockResponse::text(StatusCode::OK, "should-not-send"),
        ],
    );
    let sent_transport = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    configure_runtime(&mut client, None);
    client.set_retry_policy(Arc::new(RecordingRetryPolicy::new(
        retry_events.clone(),
        RetryDecision::Retry,
        1,
    )));

    let err = client
        .request(TextEndpoint {
            policy: ResolvedPolicy::default(),
            ..Default::default()
        })
        .execute_decoded()
        .await
        .expect_err("decode failure should be terminal");

    assert_eq!(err.category(), concord_core::error::ErrorCategory::Decode);
    assert_eq!(sent_transport.sent_count().await, 1);
    assert!(retry_events.lock().await.is_empty());
}

#[tokio::test]
async fn custom_retry_policy_not_invoked_for_map_failure() -> Result<(), ApiClientError> {
    let retry_events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        Arc::new(Mutex::new(Vec::new())),
        vec![
            MockResponse::text(StatusCode::OK, "mapped"),
            MockResponse::text(StatusCode::OK, "mapped-again"),
        ],
    );
    let sent_transport = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    configure_runtime(&mut client, None);
    client.set_retry_policy(Arc::new(RecordingRetryPolicy::new(
        retry_events.clone(),
        RetryDecision::Retry,
        8,
    )));

    let err = ORDER_EVENTS
        .scope(Arc::new(StdMutex::new(Vec::new())), async {
            client
                .request(OrderingEndpoint {
                    policy: ResolvedPolicy::default(),
                    map_mode: MapMode::Fail,
                })
                .execute_decoded()
                .await
        })
        .await
        .expect_err("map failure should be terminal");

    assert!(err.to_string().contains("transform error"));
    assert_eq!(sent_transport.sent_count().await, 1);
    assert!(retry_events.lock().await.is_empty());
    Ok(())
}

#[tokio::test]
async fn decoded_response_exposes_user_metadata() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![MockResponse::text(StatusCode::CREATED, "created")],
    );
    let client = client(TestAuthVars::default(), transport);

    let decoded = client
        .request(TextEndpoint::default())
        .execute_decoded()
        .await?;

    assert_eq!(decoded.status(), StatusCode::CREATED);
    assert_eq!(decoded.headers()[http::header::CONTENT_TYPE], "text/plain");
    assert_eq!(decoded.url().as_str(), "https://example.com/text");
    assert_eq!(decoded.meta().endpoint, "Text");
    assert_eq!(decoded.value(), "created");
    assert_eq!(decoded.into_value(), "created");
    Ok(())
}

#[tokio::test]
async fn direct_await_returns_decoded_value() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, "await")]);
    let client = client(TestAuthVars::default(), transport);

    let value = client.request(TextEndpoint::default()).await?;

    assert_eq!(value, "await");
    Ok(())
}

#[tokio::test]
async fn execute_returns_same_decoded_value_as_await() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, "execute")]);
    let client = client(TestAuthVars::default(), transport);

    let value = client.request(TextEndpoint::default()).execute().await?;

    assert_eq!(value, "execute");
    Ok(())
}

#[tokio::test]
async fn execute_raw_returns_classified_raw_response() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, "raw")]);
    let client = client(TestAuthVars::default(), transport);

    let response = client
        .request(TextEndpoint::default())
        .execute_raw()
        .await?;

    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.meta.endpoint, "Text");
    assert_eq!(response.url.as_str(), "https://example.com/text");
    assert_eq!(response.body, Bytes::from_static(b"raw"));
    Ok(())
}

#[tokio::test]
async fn execute_raw_uses_retry() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let retry_events = Arc::new(Mutex::new(Vec::new()));
    let limiter = Arc::new(ObservationRateLimiter::new(events.clone()));
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, "retry-me"),
            MockResponse::text(StatusCode::OK, "raw"),
        ],
    );
    let sent_transport = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    client.set_runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())));
    configure_runtime(&mut client, Some(limiter));
    client.set_retry_policy(Arc::new(RecordingRetryPolicy::new(
        retry_events.clone(),
        RetryDecision::Retry,
        8,
    )));

    let raw = client
        .request(TextEndpoint {
            policy: ResolvedPolicy::default(),
            ..Default::default()
        })
        .execute_raw()
        .await?;

    assert_eq!(raw.status, StatusCode::OK);
    assert_eq!(raw.body, Bytes::from_static(b"raw"));
    assert_eq!(sent_transport.sent_count().await, 2);
    let events = events.lock().await.clone();
    assert_eq!(positions(&events, "rate_acquire").len(), 2);
    assert_eq!(
        positions(&events, "rate_status:500 Internal Server Error").len(),
        1
    );
    assert_eq!(positions(&events, "rate_status:200 OK").len(), 1);
    assert_eq!(
        positions(&retry_events.lock().await, "retry_decision:Retry").len(),
        1
    );
    Ok(())
}

#[tokio::test]
async fn per_call_overrides_apply_to_pending_request() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport =
        MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, "override")]);
    let sent_transport = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    let debug = Arc::new(RecordingDebugSink::default());
    client.set_debug_sink(debug.clone());

    let decoded = client
        .request(TextEndpoint::default())
        .debug_level(DebugLevel::V)
        .timeout(Duration::from_millis(250))
        .attempt(7)
        .execute_decoded()
        .await?;

    assert_eq!(decoded.value(), "override");
    let requests = sent_transport.requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].timeout, Some(Duration::from_millis(250)));
    assert_eq!(requests[0].meta.attempt, 7);
    assert_eq!(
        debug.events(),
        vec!["request_start:v:Text:0", "response_status:v:200 OK:true"]
    );
    Ok(())
}

#[tokio::test]
async fn decode_error_includes_endpoint_status_and_content_type() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![MockResponse::text(
            StatusCode::OK,
            Bytes::from_static(b"\xff"),
        )],
    );
    let client = client(TestAuthVars::default(), transport);

    let err = client
        .request(TextEndpoint::default())
        .execute_decoded()
        .await
        .expect_err("invalid utf-8 should fail decode");
    let msg = err.to_string();
    assert!(msg.contains("GET Text"));
    assert!(msg.contains("status=200 OK"));
    assert!(msg.contains("content-type=text/plain"));
}

#[tokio::test]
async fn very_verbose_debug_does_not_emit_request_or_response_body_bytes()
-> Result<(), ApiClientError> {
    const REQUEST_SENTINEL: &str = "PR52_REQUEST_BODY_SENTINEL_DO_NOT_LOG";
    const RESPONSE_SENTINEL: &str = "PR52_RESPONSE_BODY_SENTINEL_DO_NOT_LOG";

    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![MockResponse::text(StatusCode::OK, RESPONSE_SENTINEL)],
    );
    let debug = Arc::new(RecordingDebugSink::default());
    let mut client = client(TestAuthVars::default(), transport);
    client.set_debug_sink(debug.clone());
    client.set_debug_level(DebugLevel::VV);

    let decoded = client
        .request(BodyDebugEndpoint {
            request_body: Bytes::from_static(REQUEST_SENTINEL.as_bytes()),
            policy: ResolvedPolicy::default(),
        })
        .execute_decoded()
        .await?;

    assert_eq!(decoded.value(), RESPONSE_SENTINEL);
    let debug_output = debug.events().join("\n");
    assert!(debug_output.contains("request_start:vv:BodyDebug:0"));
    assert!(debug_output.contains("request_headers:vv"));
    assert!(debug_output.contains("response_status:vv:200 OK:true"));
    assert!(debug_output.contains("response_headers:vv"));
    assert!(!debug_output.contains(REQUEST_SENTINEL));
    assert!(!debug_output.contains(RESPONSE_SENTINEL));
    Ok(())
}

#[tokio::test]
async fn dev_body_capture_disabled_by_default() -> Result<(), ApiClientError> {
    let dir = unique_capture_dir("disabled");
    std::fs::create_dir_all(&dir).expect("create test capture dir");
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![MockResponse::text(
            StatusCode::OK,
            "PR52_DISABLED_CAPTURE_SENTINEL",
        )],
    );
    let client = client(TestAuthVars::default(), transport);

    let decoded = client
        .request(TextEndpoint::default())
        .execute_decoded()
        .await?;

    assert_eq!(decoded.value(), "PR52_DISABLED_CAPTURE_SENTINEL");
    assert!(capture_files(&dir).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[allow(deprecated)]
#[tokio::test]
async fn dev_body_capture_writes_response_only_to_safe_file() -> Result<(), ApiClientError> {
    const REQUEST_SENTINEL: &str = "PR52_CAPTURE_REQUEST_SENTINEL_DO_NOT_WRITE";
    const RESPONSE_SENTINEL: &str = "PR52_CAPTURE_RESPONSE_SENTINEL";

    let dir = unique_capture_dir("enabled");
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![MockResponse::text(StatusCode::OK, RESPONSE_SENTINEL)],
    );
    let debug = Arc::new(RecordingDebugSink::default());
    let mut client = client(TestAuthVars::default(), transport);
    client.set_debug_sink(debug.clone());
    client.set_debug_level(DebugLevel::VV);
    client.configure(|cfg| {
        cfg.dev_body_capture(
            concord_core::advanced::DevBodyCaptureConfig::response_dir(&dir).max_bytes(1024),
        );
    });

    let decoded = client
        .request(BodyDebugEndpoint {
            request_body: Bytes::from_static(REQUEST_SENTINEL.as_bytes()),
            policy: ResolvedPolicy::default(),
        })
        .execute_decoded()
        .await?;

    assert_eq!(decoded.value(), RESPONSE_SENTINEL);
    let files = capture_files(&dir);
    assert_eq!(files.len(), 1);
    let filename = files[0]
        .file_name()
        .and_then(|name| name.to_str())
        .expect("capture filename should be utf-8");
    assert!(filename.starts_with("BodyDebug-POST-200-"));
    assert!(!filename.contains("body-debug"));
    assert!(!filename.contains('?'));
    assert!(!filename.contains(REQUEST_SENTINEL));
    assert!(!filename.contains(RESPONSE_SENTINEL));
    let captured = std::fs::read_to_string(&files[0]).expect("read captured response body");
    assert_eq!(captured, RESPONSE_SENTINEL);
    assert!(!captured.contains(REQUEST_SENTINEL));
    let debug_output = debug.events().join("\n");
    assert!(!debug_output.contains(REQUEST_SENTINEL));
    assert!(!debug_output.contains(RESPONSE_SENTINEL));
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[allow(deprecated)]
#[tokio::test]
async fn dev_body_capture_skips_oversized_response() -> Result<(), ApiClientError> {
    const RESPONSE_SENTINEL: &str = "PR52_OVERSIZE_RESPONSE_SENTINEL_DO_NOT_CAPTURE";

    let dir = unique_capture_dir("oversize");
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![MockResponse::text(StatusCode::OK, RESPONSE_SENTINEL)],
    );
    let debug = Arc::new(RecordingDebugSink::default());
    let mut client = client(TestAuthVars::default(), transport);
    client.set_debug_sink(debug.clone());
    client.set_debug_level(DebugLevel::VV);
    client.configure(|cfg| {
        cfg.dev_body_capture(
            concord_core::advanced::DevBodyCaptureConfig::response_dir(&dir).max_bytes(8),
        );
    });

    let decoded = client
        .request(TextEndpoint::default())
        .execute_decoded()
        .await?;

    assert_eq!(decoded.value(), RESPONSE_SENTINEL);
    assert!(capture_files(&dir).is_empty());
    let debug_output = debug.events().join("\n");
    assert!(!debug_output.contains(RESPONSE_SENTINEL));
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[allow(deprecated)]
#[tokio::test]
async fn dev_body_capture_skips_protected_auth_response() -> Result<(), ApiClientError> {
    const AUTH_RESPONSE_SENTINEL: &str = "PR52_AUTH_TOKEN_RESPONSE_SENTINEL_DO_NOT_CAPTURE";

    let dir = unique_capture_dir("auth-skip");
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![MockResponse::text(StatusCode::OK, AUTH_RESPONSE_SENTINEL)],
    );
    let mut client = client(
        TestAuthVars {
            token: Some("auth-token".to_string()),
            identity: "auth-user",
        },
        transport,
    );
    client.configure(|cfg| {
        cfg.dev_body_capture(concord_core::advanced::DevBodyCaptureConfig::response_dir(
            &dir,
        ));
    });
    let policy = ResolvedPolicy {
        auth: auth_policy(AuthPlacement::Bearer).auth,
        ..Default::default()
    };

    let decoded = client
        .request(TextEndpoint {
            policy,
            ..Default::default()
        })
        .execute_decoded()
        .await?;

    assert_eq!(decoded.value(), AUTH_RESPONSE_SENTINEL);
    assert!(capture_files(&dir).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[allow(deprecated)]
#[tokio::test]
async fn debug_sink_body_free_when_dev_body_capture_enabled() -> Result<(), ApiClientError> {
    const RESPONSE_SENTINEL: &str = "PR64_DEBUG_SINK_RESPONSE_SENTINEL";

    let dir = unique_capture_dir("debug-free");
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![MockResponse::text(StatusCode::OK, RESPONSE_SENTINEL)],
    );
    let debug = Arc::new(RecordingDebugSink::default());
    let mut client = client(TestAuthVars::default(), transport);
    client.set_debug_sink(debug.clone());
    client.set_debug_level(DebugLevel::VV);
    client.configure(|cfg| {
        cfg.dev_body_capture(
            concord_core::advanced::DevBodyCaptureConfig::response_dir(&dir).max_bytes(1024),
        );
    });

    let decoded = client
        .request(BodyDebugEndpoint {
            request_body: Bytes::from_static(b"PR64_DEBUG_SINK_REQUEST_SENTINEL"),
            policy: ResolvedPolicy::default(),
        })
        .execute_decoded()
        .await?;

    assert_eq!(decoded.value(), RESPONSE_SENTINEL);
    let debug_output = debug.events().join("\n");
    assert!(debug_output.contains("response_status:vv:200 OK:true"));
    assert!(debug_output.contains("response_headers:vv"));
    assert!(!debug_output.contains("PR64_DEBUG_SINK_REQUEST_SENTINEL"));
    assert!(!debug_output.contains(RESPONSE_SENTINEL));
    let files = capture_files(&dir);
    assert_eq!(files.len(), 1);
    let captured = std::fs::read_to_string(&files[0]).expect("read captured response body");
    assert_eq!(captured, RESPONSE_SENTINEL);
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[allow(deprecated)]
#[tokio::test]
async fn runtime_hooks_body_free_when_dev_body_capture_enabled() -> Result<(), ApiClientError> {
    const RESPONSE_SENTINEL: &str = "PR64_RUNTIME_HOOK_RESPONSE_SENTINEL";

    let dir = unique_capture_dir("hooks-free");
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, RESPONSE_SENTINEL)],
    );
    let mut client = client(TestAuthVars::default(), transport);
    client.set_runtime_hooks(Arc::new(RecordingRuntimeHooks::new(events.clone())));
    client.configure(|cfg| {
        cfg.dev_body_capture(
            concord_core::advanced::DevBodyCaptureConfig::response_dir(&dir).max_bytes(1024),
        );
    });

    let decoded = client
        .request(BodyDebugEndpoint {
            request_body: Bytes::from_static(b"PR64_RUNTIME_HOOK_REQUEST_SENTINEL"),
            policy: ResolvedPolicy::default(),
        })
        .execute_decoded()
        .await?;

    assert_eq!(decoded.value(), RESPONSE_SENTINEL);
    let hook_events = events.lock().await.clone();
    assert!(hook_events.iter().any(|event| event == "pre_send"));
    assert!(hook_events.iter().any(|event| event == "classify_response"));
    assert!(
        !hook_events
            .iter()
            .any(|event| event.contains("PR64_RUNTIME_HOOK_REQUEST_SENTINEL"))
    );
    assert!(
        !hook_events
            .iter()
            .any(|event| event.contains(RESPONSE_SENTINEL))
    );
    let files = capture_files(&dir);
    assert_eq!(files.len(), 1);
    let captured = std::fs::read_to_string(&files[0]).expect("read captured response body");
    assert_eq!(captured, RESPONSE_SENTINEL);
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

fn unique_capture_dir(name: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("test clock should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "concord-pr52-{name}-{}-{nanos}",
        std::process::id()
    ))
}

fn capture_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    if !dir.exists() {
        return Vec::new();
    }
    let mut files = std::fs::read_dir(dir)
        .expect("read capture dir")
        .map(|entry| entry.expect("read capture entry").path())
        .collect::<Vec<_>>();
    files.sort();
    files
}

#[derive(Default)]
struct RecordingDebugSink {
    events: StdMutex<Vec<String>>,
}

impl RecordingDebugSink {
    fn events(&self) -> Vec<String> {
        self.events.lock().expect("debug events lock").clone()
    }
}

impl DebugSink for RecordingDebugSink {
    fn request_start(
        &self,
        dbg: DebugLevel,
        _method: &Method,
        _url: &str,
        endpoint: &'static str,
        page_index: u32,
    ) {
        self.events
            .lock()
            .expect("debug events lock")
            .push(format!("request_start:{dbg}:{endpoint}:{page_index}"));
    }

    fn request_headers(&self, dbg: DebugLevel, _headers: &HeaderMap) {
        self.events
            .lock()
            .expect("debug events lock")
            .push(format!("request_headers:{dbg}"));
    }

    fn response_status(&self, dbg: DebugLevel, status: StatusCode, _url: &str, ok: bool) {
        self.events
            .lock()
            .expect("debug events lock")
            .push(format!("response_status:{dbg}:{status}:{ok}"));
    }

    fn response_headers(&self, dbg: DebugLevel, _headers: &HeaderMap) {
        self.events
            .lock()
            .expect("debug events lock")
            .push(format!("response_headers:{dbg}"));
    }
}

#[tokio::test]
async fn decode_error_does_not_trigger_transport_retry() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, Bytes::from_static(b"\xff")),
            MockResponse::text(StatusCode::OK, "should-not-send"),
        ],
    );
    let sent_transport = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let err = client
        .request(TextEndpoint {
            policy: retry_policy(2),
            ..Default::default()
        })
        .execute_decoded()
        .await
        .expect_err("decode failure is terminal");

    assert!(err.to_string().contains("decode error"));
    assert_eq!(sent_transport.sent_count().await, 1);
}

#[tokio::test]
async fn runtime_config_applies_debug_rate_limit_transport_and_pagination_loop_detection()
-> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![MockResponse::text(StatusCode::OK, "configured")],
    );
    let mut client = client(TestAuthVars::default(), transport.clone());
    client.configure(|cfg| {
        cfg.debug(concord_core::prelude::DebugLevel::VV);
        cfg.rate_limiter(Arc::new(NoopRateLimiter::new()));
        cfg.pagination_detect_loops(false);
    });

    assert_eq!(client.debug_level(), concord_core::prelude::DebugLevel::VV);
    assert!(!client.pagination_detect_loops());
    let decoded = client
        .request(TextEndpoint::default())
        .execute_decoded()
        .await?;
    assert_eq!(decoded.into_value(), "configured");
    assert_eq!(transport.sent_count().await, 1);
    Ok(())
}

#[tokio::test]
async fn response_content_length_above_limit_fails_before_decode() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![MockResponse::text(StatusCode::OK, "too-large").with_content_length(Some(9))],
    );
    let mut client = client(TestAuthVars::default(), transport);
    client.configure(|cfg| {
        cfg.max_response_body_bytes(4);
    });

    let err = client
        .request(TextEndpoint::default())
        .execute_decoded()
        .await
        .expect_err("known content length above limit should fail");

    assert!(matches!(
        err,
        ApiClientError::ResponseTooLarge {
            limit: 4,
            actual: 9,
            ..
        }
    ));
}

#[tokio::test]
async fn response_unknown_length_above_limit_fails_while_reading() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, Bytes::new())
                .with_content_length(None)
                .with_chunks(vec![Bytes::from_static(b"abcd"), Bytes::from_static(b"e")]),
        ],
    );
    let mut client = client(TestAuthVars::default(), transport);
    client.configure(|cfg| {
        cfg.max_response_body_bytes(4);
    });

    let err = client
        .request(TextEndpoint::default())
        .execute_decoded()
        .await
        .expect_err("chunked body above limit should fail");

    assert!(matches!(
        err,
        ApiClientError::ResponseBodyLimitExceeded { limit: 4, .. }
    ));
}

#[tokio::test]
async fn response_exactly_at_limit_succeeds() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, "abcd")]);
    let mut client = client(TestAuthVars::default(), transport);
    client.configure(|cfg| {
        cfg.max_response_body_bytes(4);
    });

    let decoded = client
        .request(TextEndpoint::default())
        .execute_decoded()
        .await?;

    assert_eq!(decoded.value(), "abcd");
    Ok(())
}

#[tokio::test]
async fn response_below_limit_succeeds() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, "abc")]);
    let mut client = client(TestAuthVars::default(), transport);
    client.configure(|cfg| {
        cfg.max_response_body_bytes(4);
    });

    let decoded = client
        .request(TextEndpoint::default())
        .execute_decoded()
        .await?;

    assert_eq!(decoded.value(), "abc");
    Ok(())
}

#[tokio::test]
async fn content_length_over_limit_rejects_before_body_read() {
    const RESPONSE_SENTINEL: &str = "PR74_CONTENT_LENGTH_OVER_LIMIT_SENTINEL";

    let events = Arc::new(Mutex::new(Vec::new()));
    let read_count = Arc::new(AtomicUsize::new(0));
    let limiter = Arc::new(ObservationRateLimiter::new(events.clone()));
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::OK, RESPONSE_SENTINEL)
                .with_content_length(Some(5))
                .with_read_count(read_count.clone()),
        ],
    );
    let sent_transport = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    let debug = Arc::new(RecordingDebugSink::default());
    client.set_debug_sink(debug.clone());
    client.set_debug_level(DebugLevel::VV);
    client.set_runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())));
    configure_runtime(&mut client, Some(limiter));
    client.configure(|cfg| {
        cfg.max_response_body_bytes(4);
    });

    let err = client
        .request(TextEndpoint {
            policy: ResolvedPolicy::default(),
            ..Default::default()
        })
        .execute_decoded()
        .await
        .expect_err("known content length above limit should fail before reading body");

    assert!(matches!(
        err,
        ApiClientError::ResponseTooLarge {
            limit: 4,
            actual: 5,
            ..
        }
    ));
    assert_eq!(body_reads(&read_count), 0);
    assert_eq!(sent_transport.sent_count().await, 1);
    let events = events.lock().await.clone();
    assert!(events.iter().any(|event| event == "pre_send"));
    assert!(events.iter().any(|event| event.starts_with("hook_meta:")));
    assert!(events.iter().any(|event| event.starts_with("rate_meta:")));
    let debug_output = debug.events().join("\n");
    assert!(debug_output.contains("request_start:v:Text:0"));
    assert!(!debug_output.contains(RESPONSE_SENTINEL));
    assert!(!format!("{events:?}").contains(RESPONSE_SENTINEL));
}

#[tokio::test]
async fn streaming_body_over_limit_rejects_during_bounded_read() {
    const RESPONSE_SENTINEL: &str = "PR74_STREAMING_BODY_OVER_LIMIT_SENTINEL";

    let events = Arc::new(Mutex::new(Vec::new()));
    let read_count = Arc::new(AtomicUsize::new(0));
    let limiter = Arc::new(ObservationRateLimiter::new(events.clone()));
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::OK, Bytes::new())
                .with_content_length(None)
                .with_chunks(vec![
                    Bytes::from_static(b"abcd"),
                    Bytes::from_static(RESPONSE_SENTINEL.as_bytes()),
                ])
                .with_read_count(read_count.clone()),
        ],
    );
    let sent_transport = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    let debug = Arc::new(RecordingDebugSink::default());
    client.set_debug_sink(debug.clone());
    client.set_runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())));
    configure_runtime(&mut client, Some(limiter));
    client.configure(|cfg| {
        cfg.max_response_body_bytes(4);
    });

    let err = client
        .request(TextEndpoint {
            policy: ResolvedPolicy::default(),
            ..Default::default()
        })
        .execute_decoded()
        .await
        .expect_err("chunked body above limit should fail while reading");

    assert!(matches!(
        err,
        ApiClientError::ResponseBodyLimitExceeded { limit: 4, .. }
    ));
    assert_eq!(body_reads(&read_count), 2);
    assert_eq!(sent_transport.sent_count().await, 1);
    let events = events.lock().await.clone();
    assert!(events.iter().any(|event| event == "transport"));
    assert!(events.iter().any(|event| event.starts_with("hook_meta:")));
    assert!(events.iter().any(|event| event.starts_with("rate_meta:")));
    let debug_output = debug.events().join("\n");
    assert!(debug_output.contains("request_start:v:Text:0"));
    assert!(!debug_output.contains(RESPONSE_SENTINEL));
    assert!(!format!("{events:?}").contains(RESPONSE_SENTINEL));
}

#[tokio::test]
async fn body_at_exact_limit_succeeds() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let read_count = Arc::new(AtomicUsize::new(0));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "abcd")
                .with_content_length(Some(4))
                .with_read_count(read_count.clone()),
        ],
    );
    let mut client = client(TestAuthVars::default(), transport);
    client.configure(|cfg| {
        cfg.max_response_body_bytes(4);
    });

    let decoded = client
        .request(TextEndpoint::default())
        .execute_decoded()
        .await?;

    assert_eq!(decoded.value(), "abcd");
    assert_eq!(body_reads(&read_count), 1);
    Ok(())
}

#[tokio::test]
async fn rate_limit_response_context_remains_body_free() {
    const RESPONSE_SENTINEL: &str = "PR74_RATE_LIMIT_BODY_FREE_SENTINEL";

    let events = Arc::new(Mutex::new(Vec::new()));
    let read_count = Arc::new(AtomicUsize::new(0));
    let limiter = Arc::new(ObservationRateLimiter::new(events.clone()));
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::OK, RESPONSE_SENTINEL)
                .with_content_length(Some(5))
                .with_read_count(read_count.clone()),
        ],
    );
    let mut client = client(TestAuthVars::default(), transport);
    client.set_runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())));
    configure_runtime(&mut client, Some(limiter));
    client.configure(|cfg| {
        cfg.max_response_body_bytes(4);
    });

    let err = client
        .request(TextEndpoint {
            policy: ResolvedPolicy::default(),
            ..Default::default()
        })
        .execute_decoded()
        .await
        .expect_err("body limit should fail before rate-limit metadata can expose body bytes");

    assert!(matches!(
        err,
        ApiClientError::ResponseTooLarge {
            limit: 4,
            actual: 5,
            ..
        }
    ));
    assert_eq!(body_reads(&read_count), 0);
    let events = events.lock().await.clone();
    assert!(events.iter().any(|event| event.starts_with("rate_meta:")));
    assert!(events.iter().any(|event| event.starts_with("rate_status:")));
    assert!(!format!("{events:?}").contains(RESPONSE_SENTINEL));
    assert!(!err.to_string().contains(RESPONSE_SENTINEL));
}

#[tokio::test]
async fn debug_hooks_never_receive_body_bytes_on_body_errors() {
    const RESPONSE_SENTINEL: &str = "PR74_DEBUG_HOOK_BODY_FREE_SENTINEL";

    let cases = [
        (
            MockResponse::text(StatusCode::OK, RESPONSE_SENTINEL).with_content_length(Some(5)),
            0usize,
        ),
        (
            MockResponse::text(StatusCode::OK, Bytes::new())
                .with_content_length(None)
                .with_chunks(vec![
                    Bytes::from_static(b"abcd"),
                    Bytes::from_static(RESPONSE_SENTINEL.as_bytes()),
                ]),
            2usize,
        ),
    ];

    for (response, expected_reads) in cases {
        let events = Arc::new(Mutex::new(Vec::new()));
        let read_count = Arc::new(AtomicUsize::new(0));
        let limiter = Arc::new(ObservationRateLimiter::new(events.clone()));
        let transport = MockTransport::new(
            events.clone(),
            vec![response.with_read_count(read_count.clone())],
        );
        let mut client = client(TestAuthVars::default(), transport);
        let debug = Arc::new(RecordingDebugSink::default());
        client.set_debug_sink(debug.clone());
        client.set_runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())));
        configure_runtime(&mut client, Some(limiter));
        client.configure(|cfg| {
            cfg.max_response_body_bytes(4);
        });

        let err = client
            .request(TextEndpoint {
                policy: ResolvedPolicy::default(),
                ..Default::default()
            })
            .execute_decoded()
            .await
            .expect_err("body error should remain body-free in debug and hooks");

        assert!(matches!(
            err,
            ApiClientError::ResponseTooLarge { .. }
                | ApiClientError::ResponseBodyLimitExceeded { .. }
        ));
        assert!(!err.to_string().contains(RESPONSE_SENTINEL));
        assert!(!format!("{err:?}").contains(RESPONSE_SENTINEL));
        assert!(!debug.events().join("\n").contains(RESPONSE_SENTINEL));
        assert_eq!(body_reads(&read_count), expected_reads);
        let events = events.lock().await.clone();
        assert!(events.iter().any(|event| event.starts_with("hook_meta:")));
        assert!(events.iter().any(|event| event.starts_with("rate_meta:")));
        assert!(!format!("{events:?}").contains(RESPONSE_SENTINEL));
    }
}

#[tokio::test]
async fn body_limit_plus_one_fails() {
    const RESPONSE_SENTINEL: &[u8] = b"abcde";

    let events = Arc::new(Mutex::new(Vec::new()));
    let read_count = Arc::new(AtomicUsize::new(0));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, Bytes::from_static(RESPONSE_SENTINEL))
                .with_content_length(Some(5))
                .with_read_count(read_count.clone()),
        ],
    );
    let mut client = client(TestAuthVars::default(), transport);
    configure_runtime(&mut client, None);
    client.configure(|cfg| {
        cfg.max_response_body_bytes(4);
    });

    let err = client
        .request(TextEndpoint::default())
        .execute_decoded()
        .await
        .expect_err("limit+1 body should fail");

    assert!(matches!(
        err,
        ApiClientError::ResponseTooLarge {
            limit: 4,
            actual: 5,
            ..
        }
    ));
    assert_eq!(body_reads(&read_count), 0);
    assert!(!err.to_string().contains("abcde"));
}

#[tokio::test]
async fn decode_failure_under_limit_is_not_body_limit() {
    const RESPONSE_SENTINEL: &str = "PR74_DECODE_FAILURE_UNDER_LIMIT_SENTINEL";

    let events = Arc::new(Mutex::new(Vec::new()));
    let read_count = Arc::new(AtomicUsize::new(0));
    let body = Bytes::from_static(b"PR74_DECODE_FAILURE_UNDER_LIMIT_SENTINEL\xff");
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, body.clone())
                .with_content_length(Some(body.len() as u64))
                .with_read_count(read_count.clone()),
        ],
    );
    let mut client = client(TestAuthVars::default(), transport);
    configure_runtime(&mut client, None);
    client.configure(|cfg| {
        cfg.max_response_body_bytes(128);
    });

    let err = client
        .request(TextEndpoint::default())
        .execute_decoded()
        .await
        .expect_err("invalid utf-8 below the body limit should fail as decode");

    assert!(matches!(err, ApiClientError::Decode { .. }));
    assert!(err.to_string().contains("decode error"));
    assert!(!err.to_string().contains("response body exceeded limit"));
    assert!(!err.to_string().contains(RESPONSE_SENTINEL));
    assert_eq!(body_reads(&read_count), 1);
    let err_debug = format!("{err:?}");
    assert!(!err_debug.contains(RESPONSE_SENTINEL));
    let mut source = std::error::Error::source(&err);
    while let Some(err_source) = source {
        assert!(!err_source.to_string().contains(RESPONSE_SENTINEL));
        source = std::error::Error::source(err_source);
    }
}

#[tokio::test]
async fn response_too_large_does_not_decode() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, Bytes::from_static(b"\xff"))
                .with_content_length(Some(8)),
        ],
    );
    let mut client = client(TestAuthVars::default(), transport);
    client.configure(|cfg| {
        cfg.max_response_body_bytes(1);
    });

    let err = client
        .request(TextEndpoint::default())
        .execute_decoded()
        .await
        .expect_err("body limit should fail before utf-8 decode");

    assert!(matches!(err, ApiClientError::ResponseTooLarge { .. }));
    assert!(!err.to_string().contains("utf-8"));
}

#[tokio::test]
async fn response_limit_applies() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, "large")]);
    let mut client = client(TestAuthVars::default(), transport);
    client.configure(|cfg| {
        cfg.max_response_body_bytes(4);
    });

    let err = client
        .request(TextEndpoint::default())
        .execute_decoded()
        .await
        .expect_err("response limit applies");

    assert!(matches!(err, ApiClientError::ResponseTooLarge { .. }));
}

#[tokio::test]
async fn body_limit_error_does_not_trigger_ordinary_retry() {
    const RESPONSE_SENTINEL: &str = "PR74_BODY_LIMIT_RETRY_SENTINEL";

    let events = Arc::new(Mutex::new(Vec::new()));
    let read_count = Arc::new(AtomicUsize::new(0));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, RESPONSE_SENTINEL)
                .with_content_length(Some(5))
                .with_read_count(read_count.clone()),
            MockResponse::text(StatusCode::OK, "should-not-send"),
        ],
    );
    let sent_transport = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    configure_runtime(&mut client, None);
    client.configure(|cfg| {
        cfg.max_response_body_bytes(4);
    });

    let err = client
        .request(TextEndpoint {
            policy: retry_policy(2),
            ..Default::default()
        })
        .execute_decoded()
        .await
        .expect_err("body limit should stop before ordinary retry");

    assert!(matches!(
        err,
        ApiClientError::ResponseTooLarge {
            limit: 4,
            actual: 5,
            ..
        }
    ));
    assert_eq!(sent_transport.sent_count().await, 1);
    assert_eq!(body_reads(&read_count), 0);
    assert!(!err.to_string().contains(RESPONSE_SENTINEL));
}

#[tokio::test]
async fn execute_raw_body_limit_behavior_characterized() {
    const RESPONSE_SENTINEL: &str = "PR74_EXECUTE_RAW_BODY_LIMIT_SENTINEL";

    let events = Arc::new(Mutex::new(Vec::new()));
    let read_count = Arc::new(AtomicUsize::new(0));
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::OK, RESPONSE_SENTINEL)
                .with_content_length(Some(5))
                .with_read_count(read_count.clone()),
            MockResponse::text(StatusCode::OK, "should-not-send"),
        ],
    );
    let sent_transport = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    configure_runtime(&mut client, None);
    client.configure(|cfg| {
        cfg.max_response_body_bytes(4);
    });

    let err = client
        .request(TextEndpoint::default())
        .execute_raw()
        .await
        .expect_err("execute_raw should enforce the same response body limit");

    assert!(matches!(
        err,
        ApiClientError::ResponseTooLarge {
            limit: 4,
            actual: 5,
            ..
        }
    ));
    assert_eq!(sent_transport.sent_count().await, 1);
    assert_eq!(body_reads(&read_count), 0);
    assert!(!err.to_string().contains(RESPONSE_SENTINEL));
}

#[tokio::test]
async fn request_body_bytes_remain_transport_only() -> Result<(), ApiClientError> {
    const REQUEST_SENTINEL: &str = "PR74_REQUEST_BODY_TRANSPORT_ONLY_SENTINEL";

    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "ok")],
    );
    let sent_transport = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    let debug = Arc::new(RecordingDebugSink::default());
    client.set_debug_sink(debug.clone());
    client.set_runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())));
    configure_runtime(
        &mut client,
        Some(Arc::new(ObservationRateLimiter::new(events.clone()))),
    );

    let decoded = client
        .request(BodyDebugEndpoint {
            request_body: Bytes::from_static(REQUEST_SENTINEL.as_bytes()),
            policy: ResolvedPolicy::default(),
        })
        .execute_decoded()
        .await?;

    assert_eq!(decoded.value(), "ok");
    let requests = sent_transport.requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].body.as_deref(),
        Some(REQUEST_SENTINEL.as_bytes())
    );
    let request_debug = format!("{:?}", requests[0]);
    assert!(!request_debug.contains(REQUEST_SENTINEL));
    let debug_output = debug.events().join("\n");
    assert!(!debug_output.contains(REQUEST_SENTINEL));
    let events = events.lock().await.clone();
    assert!(events.iter().any(|event| event == "pre_send"));
    assert!(events.iter().any(|event| event == "rate_acquire"));
    assert!(!format!("{events:?}").contains(REQUEST_SENTINEL));
    Ok(())
}

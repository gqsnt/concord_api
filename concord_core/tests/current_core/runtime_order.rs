use super::common::*;
use bytes::Bytes;
use concord_core::advanced::{
    AuthPlacement, Caps, DebugSink, NoopCacheStore, NoopRateLimiter, RetryContext, RetryDecision,
    RetryPolicy, TransportErrorKind,
};
use concord_core::internal::{
    BodyPlan, ClientPlanContext, EndpointMeta, EndpointPlan, RequestArgs, RequestOverrides,
    RequestPlan, ResolvedPolicy, ResolvedRoute, ResponsePlan,
};
use concord_core::prelude::{ApiClientError, DebugLevel, Endpoint};
use http::{HeaderMap, Method, StatusCode};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
use tokio::sync::Mutex;

#[tokio::test]
async fn fresh_cache_hit_bypasses_rate_limit_and_transport() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "transport")],
    );
    let sent_transport = transport.clone();
    let cache = Arc::new(RecordingCache::hit(
        events.clone(),
        built_response("Text", StatusCode::OK, "cached"),
    ));
    let limiter = Arc::new(RecordingRateLimiter::new(events.clone()));
    let mut client = client(
        TestAuthVars {
            token: Some("secret-token".to_string()),
            identity: "user-a",
        },
        transport,
    );
    client.set_runtime_hooks(Arc::new(RecordingRuntimeHooks::new(events.clone())));
    configure_runtime(&mut client, Some(cache), Some(limiter));

    let mut policy = cache_policy();
    policy.auth = auth_policy(AuthPlacement::Bearer).auth;
    let endpoint = TextEndpoint {
        policy,
        ..Default::default()
    };
    let decoded = client.request(endpoint).execute_decoded().await?;

    assert_eq!(decoded.value(), "cached");
    assert_eq!(sent_transport.sent_count().await, 0);
    let events = events.lock().await.clone();
    assert!(events.iter().any(|event| event == "cache_hit"));
    assert!(
        events
            .iter()
            .any(|event| event.starts_with("cache_before:hash:"))
    );
    assert!(!events.iter().any(|event| event.contains("secret-token")));
    assert!(!events.iter().any(|event| event == "pre_send"));
    assert!(!events.iter().any(|event| event == "rate_acquire"));
    assert!(!events.iter().any(|event| event == "transport"));
    Ok(())
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
                policy: ResolvedPolicy::default(),
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
    configure_runtime(&mut client, None, Some(limiter));

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
    configure_runtime(&mut client, None, Some(limiter));

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

#[tokio::test]
async fn retryable_status_is_not_cached_before_retry() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let cache = Arc::new(RecordingCache::miss(events.clone()));
    let after_response_count = cache.after_response_count.clone();
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, "retry-me"),
            MockResponse::text(StatusCode::OK, "fresh"),
        ],
    );
    let mut client = client(TestAuthVars::default(), transport);
    configure_runtime(&mut client, Some(cache), None);

    let endpoint = TextEndpoint {
        policy: {
            let mut policy = retry_policy(2);
            policy.cache = concord_core::internal::CacheSetting::Config(
                concord_core::advanced::CacheConfig::new(),
            );
            policy
        },
        ..Default::default()
    };
    let decoded = client.request(endpoint).execute_decoded().await?;

    assert_eq!(decoded.value(), "fresh");
    assert_eq!(*after_response_count.lock().await, 1);
    Ok(())
}

#[tokio::test]
async fn stale_cache_fallback_happens_after_retry_exhaustion() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let stale = built_response("Text", StatusCode::OK, "stale");
    let cache = Arc::new(RecordingCache::revalidate_stale_on_error(
        events.clone(),
        stale,
    ));
    let limiter = Arc::new(RecordingRateLimiter::new(events.clone()));
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, "retry-me"),
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, "still-failing"),
        ],
    );
    let sent_transport = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    client.set_runtime_hooks(Arc::new(RecordingRuntimeHooks::new(events.clone())));
    configure_runtime(&mut client, Some(cache), Some(limiter));

    let decoded = client
        .request(TextEndpoint {
            policy: {
                let mut policy = retry_policy(2);
                policy.cache = concord_core::internal::CacheSetting::Config(
                    concord_core::advanced::CacheConfig::new(),
                );
                policy
            },
            ..Default::default()
        })
        .execute_decoded()
        .await?;

    assert_eq!(decoded.value(), "stale");
    assert_eq!(sent_transport.sent_count().await, 2);
    let events = events.lock().await.clone();
    let observes = positions(&events, "rate_response");
    let second_acquire = positions(&events, "rate_acquire")[1];
    let stale_fallback = first_position(&events, "cache_after_error");
    assert!(observes[0] < second_acquire);
    assert!(observes[1] < stale_fallback);
    assert!(!events.iter().any(|event| event == "cache_after_response"));
    Ok(())
}

#[tokio::test]
async fn stale_cache_fallback_happens_after_retry_declines() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let stale = built_response("Text", StatusCode::OK, "stale");
    let cache = Arc::new(RecordingCache::revalidate_stale_on_error(
        events.clone(),
        stale,
    ));
    let limiter = Arc::new(RecordingRateLimiter::new(events.clone()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::BAD_REQUEST, "do-not-retry")],
    );
    let sent_transport = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    client.set_runtime_hooks(Arc::new(RecordingRuntimeHooks::new(events.clone())));
    configure_runtime(&mut client, Some(cache), Some(limiter));

    let decoded = client
        .request(TextEndpoint {
            policy: {
                let mut policy = retry_policy_for_statuses(2, vec![StatusCode::TOO_MANY_REQUESTS]);
                policy.cache = concord_core::internal::CacheSetting::Config(
                    concord_core::advanced::CacheConfig::new(),
                );
                policy
            },
            ..Default::default()
        })
        .execute_decoded()
        .await?;

    assert_eq!(decoded.value(), "stale");
    assert_eq!(sent_transport.sent_count().await, 1);
    let events = events.lock().await.clone();
    assert!(first_position(&events, "transport") < first_position(&events, "classify_response"));
    assert!(
        first_position(&events, "classify_response") < first_position(&events, "rate_response")
    );
    assert!(
        first_position(&events, "rate_response") < first_position(&events, "cache_after_error")
    );
    Ok(())
}

#[tokio::test]
async fn decode_failure_does_not_retry_or_use_stale_fallback() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let stale = built_response("Text", StatusCode::OK, "stale");
    let cache = Arc::new(RecordingCache::revalidate_stale_on_error(
        events.clone(),
        stale,
    ));
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::OK, Bytes::from_static(b"\xff")),
            MockResponse::text(StatusCode::OK, "should-not-send"),
        ],
    );
    let sent_transport = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    configure_runtime(&mut client, Some(cache), None);

    let err = client
        .request(TextEndpoint {
            policy: {
                let mut policy = retry_policy(2);
                policy.cache = concord_core::internal::CacheSetting::Config(
                    concord_core::advanced::CacheConfig::new(),
                );
                policy
            },
            ..Default::default()
        })
        .execute_decoded()
        .await
        .expect_err("decode failure should be terminal");

    assert!(err.to_string().contains("decode error"));
    assert_eq!(sent_transport.sent_count().await, 1);
    let events = events.lock().await.clone();
    assert!(!events.iter().any(|event| event == "cache_after_error"));
}

#[tokio::test]
async fn successful_cacheable_response_is_written_before_decode() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let cache = Arc::new(RecordingCache::miss(events.clone()));
    let after_response_count = cache.after_response_count.clone();
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(
            StatusCode::OK,
            Bytes::from_static(b"\xff"),
        )],
    );
    let mut client = client(TestAuthVars::default(), transport);
    configure_runtime(&mut client, Some(cache), None);

    let err = client
        .request(TextEndpoint {
            policy: cache_policy(),
            ..Default::default()
        })
        .execute_decoded()
        .await
        .expect_err("decode should fail after cache write");

    assert!(err.to_string().contains("decode error"));
    assert_eq!(*after_response_count.lock().await, 1);
    let events = events.lock().await.clone();
    assert!(events.iter().any(|event| event == "cache_after_response"));
    assert!(!events.iter().any(|event| event == "cache_after_error"));
}

#[tokio::test]
async fn transport_error_retry_exhaustion_then_stale_fallback() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let stale = built_response("Text", StatusCode::OK, "stale");
    let cache = Arc::new(RecordingCache::revalidate_stale_on_error(
        events.clone(),
        stale,
    ));
    let transport = MockTransport::with_outcomes(
        events.clone(),
        vec![
            MockOutcome::TransportError(TransportErrorKind::Timeout),
            MockOutcome::TransportError(TransportErrorKind::Timeout),
        ],
    );
    let sent_transport = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    client.set_runtime_hooks(Arc::new(RecordingRuntimeHooks::new(events.clone())));
    configure_runtime(&mut client, Some(cache), None);

    let decoded = client
        .request(TextEndpoint {
            policy: {
                let mut policy =
                    retry_policy_for_transport_errors(2, vec![TransportErrorKind::Timeout]);
                policy.cache = concord_core::internal::CacheSetting::Config(
                    concord_core::advanced::CacheConfig::new(),
                );
                policy
            },
            ..Default::default()
        })
        .execute_decoded()
        .await?;

    assert_eq!(decoded.value(), "stale");
    assert_eq!(sent_transport.sent_count().await, 2);
    let events = events.lock().await.clone();
    assert_eq!(positions(&events, "transport_error").len(), 2);
    assert!(
        positions(&events, "transport_error")[1] < first_position(&events, "cache_after_error")
    );
    assert!(!events.iter().any(|event| event == "rate_response"));
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
        .cache_bypass()
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
async fn dev_body_capture_is_disabled_by_default() -> Result<(), ApiClientError> {
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
async fn deprecated_dev_body_capture_writes_response_only_to_safe_file()
-> Result<(), ApiClientError> {
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
async fn deprecated_dev_body_capture_skips_oversized_response_by_default()
-> Result<(), ApiClientError> {
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
async fn deprecated_dev_body_capture_skips_authenticated_responses_by_default()
-> Result<(), ApiClientError> {
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
async fn runtime_config_applies_debug_cache_rate_limit_transport_and_pagination()
-> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![MockResponse::text(StatusCode::OK, "configured")],
    );
    let mut client = client(TestAuthVars::default(), transport.clone());
    client.configure(|cfg| {
        cfg.debug(concord_core::prelude::DebugLevel::VV);
        cfg.cache_store(Arc::new(NoopCacheStore));
        cfg.rate_limiter(Arc::new(NoopRateLimiter::new()));
        cfg.pagination(Caps::default().max_pages(3).max_items(12));
    });

    assert_eq!(client.debug_level(), concord_core::prelude::DebugLevel::VV);
    assert_eq!(client.pagination_caps().max_pages, 3);
    assert_eq!(client.pagination_caps().max_items, 12);
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
async fn response_too_large_does_not_cache() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let cache = Arc::new(RecordingCache::miss(events.clone()));
    let after_response_count = cache.after_response_count.clone();
    let transport = MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, "large")]);
    let mut client = client(TestAuthVars::default(), transport);
    configure_runtime(&mut client, Some(cache), None);
    client.configure(|cfg| {
        cfg.max_response_body_bytes(4);
    });

    let endpoint = TextEndpoint {
        policy: cache_policy(),
        ..Default::default()
    };
    let err = client
        .request(endpoint)
        .execute_decoded()
        .await
        .expect_err("body limit should fail before cache write");

    assert!(matches!(err, ApiClientError::ResponseTooLarge { .. }));
    assert_eq!(*after_response_count.lock().await, 0);
}

#[tokio::test]
async fn response_limit_applies_when_cache_is_off() {
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
        .expect_err("response limit applies independently from cache");

    assert!(matches!(err, ApiClientError::ResponseTooLarge { .. }));
}

#[tokio::test]
async fn cache_max_body_smaller_than_response_limit_skips_store_but_decode_succeeds()
-> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let cache = Arc::new(RecordingCache::miss(events.clone()));
    let after_response_count = cache.after_response_count.clone();
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "2k")],
    );
    let mut client = client(TestAuthVars::default(), transport);
    configure_runtime(&mut client, Some(cache), None);
    client.configure(|cfg| {
        cfg.max_response_body_bytes(4 * 1024);
    });

    let endpoint = TextEndpoint {
        policy: ResolvedPolicy {
            cache: concord_core::internal::CacheSetting::Config(
                concord_core::advanced::CacheConfig::new().with_max_body_bytes(1),
            ),
            ..Default::default()
        },
        ..Default::default()
    };
    let decoded = client.request(endpoint).execute_decoded().await?;

    assert_eq!(decoded.value(), "2k");
    assert_eq!(*after_response_count.lock().await, 1);
    assert!(
        events
            .lock()
            .await
            .iter()
            .any(|event| event == "cache_max_body_skip")
    );
    Ok(())
}

#[tokio::test]
async fn response_limit_smaller_than_cache_max_body_fails_before_cache() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let cache = Arc::new(RecordingCache::miss(events.clone()));
    let after_response_count = cache.after_response_count.clone();
    let transport = MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, "large")]);
    let mut client = client(TestAuthVars::default(), transport);
    configure_runtime(&mut client, Some(cache), None);
    client.configure(|cfg| {
        cfg.max_response_body_bytes(4);
    });

    let endpoint = TextEndpoint {
        policy: ResolvedPolicy {
            cache: concord_core::internal::CacheSetting::Config(
                concord_core::advanced::CacheConfig::new().with_max_body_bytes(4 * 1024),
            ),
            ..Default::default()
        },
        ..Default::default()
    };
    let err = client
        .request(endpoint)
        .execute_decoded()
        .await
        .expect_err("response limit should fail before cache max_body is considered");

    assert!(matches!(err, ApiClientError::ResponseTooLarge { .. }));
    assert_eq!(*after_response_count.lock().await, 0);
}

use super::common::*;

use bytes::Bytes;
use concord_core::advanced::{
    RateLimitContext, RateLimitFuture, RateLimitPermit, RateLimitResponseAction,
    RateLimitResponseContext, RateLimiter,
};
use concord_core::internal::{
    BodyPlan, ClientPlanContext, EndpointMeta, EndpointPlan, RequestArgs, RequestOverrides,
    RequestPlan, ResolvedPolicy, ResolvedRoute, ResponsePlan,
};
use concord_core::prelude::{ApiClientError, Endpoint, RateLimitObservation, RateLimitObserver};
use http::{HeaderValue, Method, StatusCode};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

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

#[derive(Clone)]
struct HostlessEndpoint {
    policy: ResolvedPolicy,
}

impl Endpoint<TestCx> for HostlessEndpoint {
    type Response = String;

    fn plan(&self, _ctx: &ClientPlanContext<'_, TestCx>) -> Result<RequestPlan, ApiClientError> {
        Ok(RequestPlan {
            endpoint: EndpointPlan {
                meta: EndpointMeta {
                    name: "Hostless",
                    method: Method::GET,
                    idempotent: true,
                    facade_path: &[],
                },
                route: ResolvedRoute::new(http::uri::Scheme::HTTPS, "", "/hostless"),
                policy: self.policy.clone(),
                body: BodyPlan::None,
                response: ResponsePlan {
                    accept: Some(HeaderValue::from_static("text/plain")),
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

#[derive(Clone)]
struct RecordingRateLimitContextLimiter {
    events: Arc<Mutex<Vec<String>>>,
    response_action: RateLimitResponseAction,
}

impl RecordingRateLimitContextLimiter {
    fn new(events: Arc<Mutex<Vec<String>>>, response_action: RateLimitResponseAction) -> Self {
        Self {
            events,
            response_action,
        }
    }
}

impl RateLimiter for RecordingRateLimitContextLimiter {
    fn acquire<'a>(
        &'a self,
        ctx: RateLimitContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitPermit, ApiClientError>> {
        let events = self.events.clone();
        let endpoint = ctx.endpoint;
        let method = ctx.method.clone();
        let url = ctx.url.to_string();
        let url_host = ctx.url_host.map(str::to_string);
        let attempt = ctx.attempt;
        let page_index = ctx.page_index;
        let idempotent = ctx.idempotent;
        Box::pin(async move {
            let mut events = events.lock().await;
            events.push(format!(
                "rate_acquire_meta:{endpoint}:{method}:{url}:{}:{attempt}:{page_index}:{idempotent}",
                url_host.as_deref().unwrap_or("<none>")
            ));
            Ok(RateLimitPermit)
        })
    }

    fn on_response<'a>(
        &'a self,
        ctx: RateLimitResponseContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitResponseAction, ApiClientError>> {
        let events = self.events.clone();
        let action = self.response_action.clone();
        let meta = ctx.meta;
        let url_host = meta.url_host.unwrap_or("<none>");
        let status = ctx.status;
        let headers = format!("{:?}", ctx.headers);
        Box::pin(async move {
            let mut events = events.lock().await;
            events.push(format!(
                "rate_response_meta:{}:{}:{}:{}:{}:{}:{}",
                meta.endpoint,
                meta.method,
                meta.url,
                url_host,
                meta.attempt,
                meta.page_index,
                meta.idempotent,
            ));
            events.push(format!("rate_response_status:{status}"));
            events.push(format!("rate_response_headers:{headers}"));
            Ok(action)
        })
    }
}

#[derive(Clone)]
enum ObserverMode {
    Fixed(RateLimitObservation),
    RetryAfter429,
}

#[derive(Clone)]
struct RecordingRateLimitObserver {
    events: Arc<Mutex<Vec<String>>>,
    mode: ObserverMode,
}

impl RecordingRateLimitObserver {
    fn fixed(events: Arc<Mutex<Vec<String>>>, observation: RateLimitObservation) -> Self {
        Self {
            events,
            mode: ObserverMode::Fixed(observation),
        }
    }

    fn retry_after_429(events: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            events,
            mode: ObserverMode::RetryAfter429,
        }
    }
}

impl RateLimitObserver for RecordingRateLimitObserver {
    fn observe(&self, ctx: RateLimitResponseContext<'_>) -> RateLimitObservation {
        let mut events = self.events.try_lock().expect("rate limit observer lock");
        events.push(format!(
            "rate_observe_meta:{}:{}:{}:{}:{}:{}:{}",
            ctx.meta.endpoint,
            ctx.meta.method,
            ctx.meta.url,
            ctx.meta.url_host.unwrap_or("<none>"),
            ctx.meta.attempt,
            ctx.meta.page_index,
            ctx.meta.idempotent
        ));
        events.push(format!("rate_observe_status:{}", ctx.status));
        events.push(format!("rate_observe_headers:{:?}", ctx.headers));
        match &self.mode {
            ObserverMode::Fixed(observation) => observation.clone(),
            ObserverMode::RetryAfter429 => ctx.on_429().retry_after(),
        }
    }
}

#[tokio::test]
async fn rate_limit_observation_happens_after_response_classification() -> Result<(), ApiClientError>
{
    let events = Arc::new(Mutex::new(Vec::new()));
    let limiter = Arc::new(RecordingRateLimiter::new(events.clone()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "ok")],
    );
    let mut client = client(TestAuthVars::default(), transport);
    client.set_runtime_hooks(Arc::new(RecordingRuntimeHooks::new(events.clone())));
    configure_runtime(&mut client, Some(limiter));

    client
        .request(TextEndpoint::default())
        .execute_decoded()
        .await?;

    let events = events.lock().await.clone();
    let acquire = events
        .iter()
        .position(|event| event == "rate_acquire")
        .expect("rate limiter acquired");
    let transport = events
        .iter()
        .position(|event| event == "transport")
        .expect("transport sent");
    let classify = events
        .iter()
        .position(|event| event == "classify_response")
        .expect("response classified");
    let observe = events
        .iter()
        .position(|event| event == "rate_response")
        .expect("rate limiter observed response");
    assert!(acquire < transport);
    assert!(transport < classify);
    assert!(classify < observe);
    Ok(())
}

fn first_event_with_prefix(events: &[String], prefix: &str) -> usize {
    events
        .iter()
        .position(|event| event.starts_with(prefix))
        .unwrap_or_else(|| panic!("missing event prefix `{prefix}` in {events:?}"))
}

fn first_position(events: &[String], needle: &str) -> usize {
    events
        .iter()
        .position(|event| event == needle)
        .unwrap_or_else(|| panic!("missing event `{needle}` in {events:?}"))
}

#[tokio::test]
async fn rate_limit_observes_200_before_decode_failure() {
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
    configure_runtime(
        &mut client,
        Some(Arc::new(ObservationRateLimiter::new(events.clone()))),
    );
    client.set_runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())));

    let err = client
        .request(ObservationFailureEndpoint {
            policy: Default::default(),
            request_body: Bytes::from_static(REQUEST_SENTINEL.as_bytes()),
        })
        .execute_decoded()
        .await
        .expect_err("invalid payload should fail decode");

    assert_eq!(err.category(), concord_core::error::ErrorCategory::Decode);
    let events = events.lock().await.clone();
    assert!(
        events
            .iter()
            .any(|event| event.starts_with("rate_status:200 OK"))
    );
    assert!(events.iter().any(|event| event.starts_with("rate_meta:")));
    assert!(
        events
            .iter()
            .any(|event| event.starts_with("rate_headers:"))
    );
    assert!(
        events
            .iter()
            .any(|event| event.starts_with("hook_status:200 OK"))
    );
    let hook = first_event_with_prefix(&events, "hook_status:200 OK");
    let rate = first_event_with_prefix(&events, "rate_status:200 OK");
    assert!(hook < rate);
    assert!(!events.iter().any(|event| event.contains(REQUEST_SENTINEL)));
    assert!(!events.iter().any(|event| event.contains(RESPONSE_SENTINEL)));
}

#[tokio::test]
async fn rate_limit_observes_retryable_status_before_retry() -> Result<(), ApiClientError> {
    const FIRST_SENTINEL: &str = "PR65_RATE_LIMIT_FIRST_BODY_DO_NOT_LEAK";
    const SECOND_SENTINEL: &str = "PR65_RATE_LIMIT_SECOND_BODY_DO_NOT_LEAK";

    let events = Arc::new(Mutex::new(Vec::new()));
    let limiter = Arc::new(ObservationRateLimiter::new(events.clone()));
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::TOO_MANY_REQUESTS, FIRST_SENTINEL),
            MockResponse::text(StatusCode::OK, SECOND_SENTINEL),
        ],
    );
    let sent_transport = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    client.set_runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())));
    configure_runtime(&mut client, Some(limiter));

    let decoded = client
        .request(TextEndpoint {
            policy: retry_policy_for_statuses(2, vec![StatusCode::TOO_MANY_REQUESTS]),
            ..Default::default()
        })
        .execute_decoded()
        .await?;

    assert_eq!(decoded.value(), SECOND_SENTINEL);
    assert_eq!(sent_transport.sent_count().await, 2);
    let events = events.lock().await.clone();
    let first = first_event_with_prefix(&events, "rate_status:429 Too Many Requests");
    let second = first_event_with_prefix(&events, "rate_status:200 OK");
    assert!(first < second);
    assert!(events.iter().any(|event| event.starts_with("rate_meta:")));
    assert!(
        events
            .iter()
            .any(|event| event.starts_with("rate_headers:"))
    );
    assert!(!events.iter().any(|event| event.contains(FIRST_SENTINEL)));
    assert!(!events.iter().any(|event| event.contains(SECOND_SENTINEL)));
    Ok(())
}

#[tokio::test]
async fn rate_limit_observes_auth_rejection_response() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let limiter = Arc::new(ObservationRateLimiter::new(events.clone()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::FORBIDDEN, "forbidden")],
    );
    let sent_transport = transport.clone();
    let mut client = concord_core::prelude::ApiClient::<ObservationAuthCx, _>::with_transport(
        (),
        ObservationAuthVars::bearer("PR65_QUERY_SECRET_DO_NOT_LEAK", "user-a", events.clone()),
        transport,
    );
    client.set_runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())));
    configure_runtime(&mut client, Some(limiter));

    let err = client
        .request(TextEndpoint {
            policy: auth_policy(concord_core::advanced::AuthPlacement::Query("api_key")),
            ..Default::default()
        })
        .execute_decoded()
        .await
        .expect_err("403 query-auth rejection should remain terminal");

    assert!(err.to_string().contains("auth challenge rejected"));
    assert_eq!(sent_transport.sent_count().await, 1);
    let events = events.lock().await.clone();
    assert!(
        events
            .iter()
            .any(|event| event.starts_with("rate_status:403 Forbidden"))
    );
    let rate = first_event_with_prefix(&events, "rate_status:403 Forbidden");
    let auth = first_position(&events, "auth_rejection:403 Forbidden");
    assert!(rate < auth);
    assert!(events.iter().any(|event| event.starts_with("rate_meta:")));
    assert!(
        events
            .iter()
            .any(|event| event.starts_with("rate_headers:"))
    );
    assert!(
        events
            .iter()
            .any(|event| event.starts_with("auth_rejection:403 Forbidden"))
    );
    assert!(
        !events
            .iter()
            .any(|event| event.contains("PR65_QUERY_SECRET_DO_NOT_LEAK"))
    );
    Ok(())
}

#[tokio::test]
async fn rate_limit_does_not_observe_transport_error_as_response() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let limiter = Arc::new(ObservationRateLimiter::new(events.clone()));
    let transport = MockTransport::with_outcomes(
        events.clone(),
        vec![MockOutcome::TransportError(
            concord_core::advanced::TransportErrorKind::Timeout,
        )],
    );
    let mut client = client(TestAuthVars::default(), transport);
    configure_runtime(&mut client, Some(limiter));

    let err = client
        .request(TextEndpoint::default())
        .execute_decoded()
        .await
        .expect_err("transport error should remain terminal when not retryable");

    assert!(err.to_string().contains("transport"));
    let events = events.lock().await.clone();
    assert!(events.iter().any(|event| event == "rate_acquire"));
    assert!(!events.iter().any(|event| event.starts_with("rate_status:")));
}

#[tokio::test]
async fn missing_host_fails_before_rate_limit_acquire() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let limiter = Arc::new(RecordingRateLimitContextLimiter::new(
        events.clone(),
        RateLimitResponseAction::Continue,
    ));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "should-not-send")],
    );
    let sent_transport = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    configure_runtime(&mut client, Some(limiter));

    let err = client
        .request(HostlessEndpoint {
            policy: ResolvedPolicy::default(),
        })
        .execute_decoded()
        .await
        .expect_err("hostless route should fail before rate limit acquisition");

    assert_eq!(err.category(), concord_core::error::ErrorCategory::Config);
    assert!(err.to_string().contains("build url"));
    assert_eq!(sent_transport.sent_count().await, 0);
    let events = events.lock().await.clone();
    assert!(events.is_empty());
}

#[tokio::test]
async fn rate_limit_acquire_context_does_not_expose_bearer_auth() -> Result<(), ApiClientError> {
    const BEARER_SENTINEL: &str = "PR67_BEARER_SECRET_DO_NOT_LEAK";

    let events = Arc::new(Mutex::new(Vec::new()));
    let limiter = Arc::new(RecordingRateLimitContextLimiter::new(
        events.clone(),
        RateLimitResponseAction::Continue,
    ));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "ok")],
    );
    let mut client = concord_core::prelude::ApiClient::<ObservationAuthCx, _>::with_transport(
        (),
        ObservationAuthVars::bearer(BEARER_SENTINEL, "bearer", events.clone()),
        transport,
    );
    configure_runtime(&mut client, Some(limiter));

    let decoded = client
        .request(TextEndpoint {
            policy: auth_policy(concord_core::advanced::AuthPlacement::Bearer),
            ..Default::default()
        })
        .execute_decoded()
        .await?;

    assert_eq!(decoded.value(), "ok");
    let events = events.lock().await.clone();
    assert!(
        events
            .iter()
            .any(|event| event.starts_with("rate_acquire_meta:"))
    );
    assert!(!events.iter().any(|event| event.contains(BEARER_SENTINEL)));
    Ok(())
}

#[tokio::test]
async fn rate_limit_acquire_context_does_not_expose_query_auth() -> Result<(), ApiClientError> {
    const QUERY_SENTINEL: &str = "PR67_QUERY_SECRET_DO_NOT_LEAK";

    let events = Arc::new(Mutex::new(Vec::new()));
    let limiter = Arc::new(RecordingRateLimitContextLimiter::new(
        events.clone(),
        RateLimitResponseAction::Continue,
    ));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "ok")],
    );
    let mut client = client(
        TestAuthVars {
            token: Some(QUERY_SENTINEL.to_string()),
            identity: "query",
        },
        transport,
    );
    configure_runtime(&mut client, Some(limiter));

    let decoded = client
        .request(TextEndpoint {
            policy: auth_policy(concord_core::advanced::AuthPlacement::Query("api_key")),
            ..Default::default()
        })
        .execute_decoded()
        .await?;

    assert_eq!(decoded.value(), "ok");
    let events = events.lock().await.clone();
    assert!(
        events
            .iter()
            .any(|event| event.starts_with("rate_acquire_meta:"))
    );
    assert!(!events.iter().any(|event| event.contains(QUERY_SENTINEL)));
    Ok(())
}

#[tokio::test]
async fn rate_limit_acquire_context_does_not_expose_basic_auth_material()
-> Result<(), ApiClientError> {
    const USER_SENTINEL: &str = "PR67_BASIC_USERNAME_DO_NOT_LEAK";
    const PASS_SENTINEL: &str = "PR67_BASIC_PASSWORD_DO_NOT_LEAK";

    let events = Arc::new(Mutex::new(Vec::new()));
    let limiter = Arc::new(RecordingRateLimitContextLimiter::new(
        events.clone(),
        RateLimitResponseAction::Continue,
    ));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "ok")],
    );
    let mut client = concord_core::prelude::ApiClient::<ObservationAuthCx, _>::with_transport(
        (),
        ObservationAuthVars::basic(USER_SENTINEL, PASS_SENTINEL, "basic", events.clone()),
        transport,
    );
    configure_runtime(&mut client, Some(limiter));

    let decoded = client
        .request(TextEndpoint {
            policy: auth_policy(concord_core::advanced::AuthPlacement::Basic),
            ..Default::default()
        })
        .execute_decoded()
        .await?;

    assert_eq!(decoded.value(), "ok");
    let events = events.lock().await.clone();
    assert!(
        events
            .iter()
            .any(|event| event.starts_with("rate_acquire_meta:"))
    );
    assert!(!events.iter().any(|event| event.contains(USER_SENTINEL)));
    assert!(!events.iter().any(|event| event.contains(PASS_SENTINEL)));
    Ok(())
}

#[tokio::test]
async fn rate_limit_response_context_does_not_expose_bearer_auth() -> Result<(), ApiClientError> {
    const BEARER_SENTINEL: &str = "PR67_BEARER_SECRET_DO_NOT_LEAK";

    let events = Arc::new(Mutex::new(Vec::new()));
    let limiter = Arc::new(RecordingRateLimitContextLimiter::new(
        events.clone(),
        RateLimitResponseAction::Continue,
    ));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "ok")],
    );
    let mut client = concord_core::prelude::ApiClient::<ObservationAuthCx, _>::with_transport(
        (),
        ObservationAuthVars::bearer(BEARER_SENTINEL, "bearer", events.clone()),
        transport,
    );
    configure_runtime(&mut client, Some(limiter));

    client
        .request(TextEndpoint {
            policy: auth_policy(concord_core::advanced::AuthPlacement::Bearer),
            ..Default::default()
        })
        .execute_decoded()
        .await?;

    let events = events.lock().await.clone();
    assert!(
        events
            .iter()
            .any(|event| event.starts_with("rate_response_meta:"))
    );
    assert!(!events.iter().any(|event| event.contains(BEARER_SENTINEL)));
    Ok(())
}

#[tokio::test]
async fn rate_limit_response_context_does_not_expose_query_auth() -> Result<(), ApiClientError> {
    const QUERY_SENTINEL: &str = "PR67_QUERY_SECRET_DO_NOT_LEAK";

    let events = Arc::new(Mutex::new(Vec::new()));
    let limiter = Arc::new(RecordingRateLimitContextLimiter::new(
        events.clone(),
        RateLimitResponseAction::Continue,
    ));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "ok")],
    );
    let mut client = client(
        TestAuthVars {
            token: Some(QUERY_SENTINEL.to_string()),
            identity: "query",
        },
        transport,
    );
    configure_runtime(&mut client, Some(limiter));

    client
        .request(TextEndpoint {
            policy: auth_policy(concord_core::advanced::AuthPlacement::Query("api_key")),
            ..Default::default()
        })
        .execute_decoded()
        .await?;

    let events = events.lock().await.clone();
    assert!(
        events
            .iter()
            .any(|event| event.starts_with("rate_response_meta:"))
    );
    assert!(!events.iter().any(|event| event.contains(QUERY_SENTINEL)));
    Ok(())
}

#[tokio::test]
async fn rate_limit_response_context_does_not_expose_basic_auth_material()
-> Result<(), ApiClientError> {
    const USER_SENTINEL: &str = "PR67_BASIC_USERNAME_DO_NOT_LEAK";
    const PASS_SENTINEL: &str = "PR67_BASIC_PASSWORD_DO_NOT_LEAK";

    let events = Arc::new(Mutex::new(Vec::new()));
    let limiter = Arc::new(RecordingRateLimitContextLimiter::new(
        events.clone(),
        RateLimitResponseAction::Continue,
    ));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "ok")],
    );
    let mut client = concord_core::prelude::ApiClient::<ObservationAuthCx, _>::with_transport(
        (),
        ObservationAuthVars::basic(USER_SENTINEL, PASS_SENTINEL, "basic", events.clone()),
        transport,
    );
    configure_runtime(&mut client, Some(limiter));

    client
        .request(TextEndpoint {
            policy: auth_policy(concord_core::advanced::AuthPlacement::Basic),
            ..Default::default()
        })
        .execute_decoded()
        .await?;

    let events = events.lock().await.clone();
    assert!(
        events
            .iter()
            .any(|event| event.starts_with("rate_response_meta:"))
    );
    assert!(!events.iter().any(|event| event.contains(USER_SENTINEL)));
    assert!(!events.iter().any(|event| event.contains(PASS_SENTINEL)));
    Ok(())
}

#[tokio::test]
async fn rate_limit_response_huge_delay_returns_typed_error() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let observer = Arc::new(RecordingRateLimitObserver::fixed(
        events.clone(),
        RateLimitObservation::limited().with_delay(Duration::MAX),
    ));
    let rate_limiter =
        Arc::new(concord_core::advanced::GovernorRateLimiter::new().with_response_policy(observer));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::TOO_MANY_REQUESTS, "limited")],
    );
    let sent_transport = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    configure_runtime(&mut client, Some(rate_limiter));

    let err = client
        .request(TextEndpoint {
            policy: retry_policy_for_statuses(2, vec![StatusCode::TOO_MANY_REQUESTS]),
            ..Default::default()
        })
        .execute_decoded()
        .await
        .expect_err("overflowing rate-limit delay should return a typed error");

    assert_eq!(
        err.category(),
        concord_core::error::ErrorCategory::InternalInvariant
    );
    assert!(err.to_string().contains("cooldown duration overflowed"));
    assert_eq!(sent_transport.sent_count().await, 1);
    let events = events.lock().await.clone();
    assert!(
        events
            .iter()
            .any(|event| event.starts_with("rate_observe_status:429"))
    );
}

#[tokio::test]
async fn rate_limit_response_zero_delay_is_allowed() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let observer = Arc::new(RecordingRateLimitObserver::fixed(
        events.clone(),
        RateLimitObservation::limited().with_delay(Duration::ZERO),
    ));
    let rate_limiter =
        Arc::new(concord_core::advanced::GovernorRateLimiter::new().with_response_policy(observer));
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, "retry-me"),
            MockResponse::text(StatusCode::OK, "ok"),
        ],
    );
    let sent_transport = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    configure_runtime(&mut client, Some(rate_limiter));

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
    assert!(
        events
            .iter()
            .any(|event| event.starts_with("rate_observe_status:500"))
    );
    assert!(
        events
            .iter()
            .any(|event| event.starts_with("rate_observe_status:200"))
    );
    Ok(())
}

#[tokio::test]
async fn rate_limit_response_action_cannot_bypass_auth_rejection() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let observer = Arc::new(RecordingRateLimitObserver::fixed(
        events.clone(),
        RateLimitObservation::limited().with_delay(Duration::ZERO),
    ));
    let rate_limiter =
        Arc::new(concord_core::advanced::GovernorRateLimiter::new().with_response_policy(observer));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::FORBIDDEN, "forbidden")],
    );
    let sent_transport = transport.clone();
    let mut client = concord_core::prelude::ApiClient::<ObservationAuthCx, _>::with_transport(
        (),
        ObservationAuthVars::bearer("PR67_BEARER_SECRET_DO_NOT_LEAK", "bearer", events.clone()),
        transport,
    );
    configure_runtime(&mut client, Some(rate_limiter));

    let err = client
        .request(TextEndpoint {
            policy: auth_policy(concord_core::advanced::AuthPlacement::Bearer),
            ..Default::default()
        })
        .execute_decoded()
        .await
        .expect_err("auth rejection should remain terminal");

    assert!(err.to_string().contains("auth challenge rejected"));
    assert_eq!(sent_transport.sent_count().await, 1);
    let events = events.lock().await.clone();
    let observe = first_event_with_prefix(&events, "rate_observe_status:403 Forbidden");
    let auth = first_position(&events, "auth_rejection:403 Forbidden");
    assert!(observe < auth);
    Ok(())
}

#[tokio::test]
async fn retry_after_429_does_not_double_sleep_with_rate_limit_observer()
-> Result<(), ApiClientError> {
    let started = std::time::Instant::now();
    let events = Arc::new(Mutex::new(Vec::new()));
    let observer = Arc::new(RecordingRateLimitObserver::retry_after_429(events.clone()));
    let rate_limiter =
        Arc::new(concord_core::advanced::GovernorRateLimiter::new().with_response_policy(observer));
    let mut headers = http::HeaderMap::new();
    headers.insert(http::header::RETRY_AFTER, HeaderValue::from_static("1"));
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse {
                status: StatusCode::TOO_MANY_REQUESTS,
                headers,
                body: Bytes::from_static(b"retry-me"),
                content_length: None,
                chunks: None,
                read_count: None,
            },
            MockResponse::text(StatusCode::OK, "ok"),
        ],
    );
    let sent_transport = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    configure_runtime(&mut client, Some(rate_limiter));

    let handle = tokio::spawn(async move {
        client
            .request(TextEndpoint {
                policy: retry_policy_for_statuses(2, vec![StatusCode::TOO_MANY_REQUESTS]),
                ..Default::default()
            })
            .execute_decoded()
            .await
    });
    let decoded = tokio::time::timeout(Duration::from_secs(3), handle)
        .await
        .expect("request should finish without double sleeping")
        .expect("join handle should succeed")?;

    assert_eq!(decoded.value(), "ok");
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(sent_transport.sent_count().await, 2);
    let events = events.lock().await.clone();
    let first = first_event_with_prefix(&events, "rate_observe_status:429 Too Many Requests");
    let second = first_event_with_prefix(&events, "rate_observe_status:200 OK");
    assert!(first < second);
    Ok(())
}

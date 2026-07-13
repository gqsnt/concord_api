use super::common::buffered_endpoint_response_terminal;
use super::common::*;
use crate::support::assert_error_chain_does_not_contain_any;

use bytes::Bytes;
use concord_core::advanced::{
    BufferedResponse, RateLimitContext, RateLimitFuture, RateLimitPermit, RateLimitResponseAction,
    RateLimitResponseContext, RateLimiter, ResponseEntity,
};
use concord_core::error::ErrorCategory;
use concord_core::internal::{
    ClientPlanContext, EndpointMeta, EndpointPlan, PreparedBody, RequestOverrides, RequestPlan,
    ResolvedPolicy, ResolvedRoute, ResponsePlan,
};
use concord_core::prelude::{
    ApiClientError, Endpoint, RateLimitObservation, RateLimitObserver, ReusableEndpoint,
};
use http::{HeaderValue, Method, StatusCode};
use std::future::Future;
use std::pin::Pin;
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

    fn execute<'a>(
        client: &'a concord_core::prelude::ApiClient<TestCx>,
        plan: RequestPlan,
    ) -> Pin<Box<dyn Future<Output = Result<String, ApiClientError>> + Send + 'a>> {
        Box::pin(
            async move { BufferedResponse::<InvalidJsonResponse>::execute(client, plan).await },
        )
    }
}

impl ReusableEndpoint<TestCx> for ObservationFailureEndpoint {
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
                response: ResponsePlan {
                    accept: Some(HeaderValue::from_static("application/json")),
                    no_content: false,
                    format: concord_core::internal::Format::Text,
                },
                pagination: None,
            },
            body: PreparedBody::reusable_bytes(
                self.request_body.clone(),
                Some(HeaderValue::from_static("application/json")),
            ),
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

    buffered_endpoint_execute!(TestCx, concord_core::prelude::Text<String>);
}

buffered_endpoint_response_terminal!(
    HostlessEndpoint,
    TestCx,
    concord_core::prelude::Text<String>
);

impl ReusableEndpoint<TestCx> for HostlessEndpoint {
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
                response: ResponsePlan {
                    accept: Some(HeaderValue::from_static("text/plain")),
                    no_content: false,
                    format: concord_core::internal::Format::Text,
                },
                pagination: None,
            },
            body: PreparedBody::empty(),
            overrides: RequestOverrides::default(),
        })
    }
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
        let page_index = ctx.page_index;
        let idempotent = ctx.idempotent;
        Box::pin(async move {
            let mut events = events.lock().await;
            events.push(format!(
                "rate_acquire_meta:{endpoint}:{method}:{url}:{}:{page_index}:{idempotent}",
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
                "rate_response_meta:{}:{}:{}:{}:{}:{}",
                meta.endpoint, meta.method, meta.url, url_host, meta.page_index, meta.idempotent,
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
            "rate_observe_meta:{}:{}:{}:{}:{}:{}",
            ctx.meta.endpoint,
            ctx.meta.method,
            ctx.meta.url,
            ctx.meta.url_host.unwrap_or("<none>"),
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

#[derive(Clone)]
struct SanitizedResponseHeaderRateLimiter {
    events: Arc<Mutex<Vec<String>>>,
}

impl SanitizedResponseHeaderRateLimiter {
    fn new(events: Arc<Mutex<Vec<String>>>) -> Self {
        Self { events }
    }
}

impl RateLimiter for SanitizedResponseHeaderRateLimiter {
    fn acquire<'a>(
        &'a self,
        _ctx: RateLimitContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitPermit, ApiClientError>> {
        Box::pin(async move { Ok(RateLimitPermit) })
    }

    fn on_response<'a>(
        &'a self,
        ctx: RateLimitResponseContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitResponseAction, ApiClientError>> {
        let events = self.events.clone();
        Box::pin(async move {
            let set_cookie = ctx
                .headers
                .get(http::header::SET_COOKIE)
                .map(|value| value.as_str().to_owned());
            let www_authenticate = ctx
                .headers
                .get(http::header::WWW_AUTHENTICATE)
                .map(|value| value.as_str().to_owned());
            let refresh_token = ctx
                .headers
                .get(http::HeaderName::from_static("x-refresh-token"))
                .map(|value| value.as_str().to_owned());
            let retry_after = ctx
                .headers
                .get(http::header::RETRY_AFTER)
                .map(|value| value.as_str().to_owned());
            let rate_limit_remaining = ctx
                .headers
                .get(http::HeaderName::from_static("x-rate-limit-remaining"))
                .map(|value| value.as_str().to_owned());
            let mut events = events.lock().await;
            events.push(format!(
                "sanitized_cookie:{:?}:contains:{}",
                set_cookie,
                ctx.headers.contains_key(http::header::SET_COOKIE)
            ));
            events.push(format!("sanitized_authenticate:{:?}", www_authenticate));
            events.push(format!("sanitized_refresh:{:?}", refresh_token));
            events.push(format!("sanitized_retry_after:{:?}", retry_after));
            events.push(format!(
                "sanitized_rate_limit_remaining:{:?}",
                rate_limit_remaining
            ));
            Ok(RateLimitResponseAction::Continue)
        })
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
    let _server = transport.clone();
    let mut client = client(TestAuthVars::default(), transport.clone());
    client.set_runtime_hooks(Arc::new(RecordingRuntimeHooks::new(events.clone())));
    configure_runtime(&mut client, Some(limiter));

    client.request(TextEndpoint::default()).response().await?;

    let events = events.lock().await.clone();
    let acquire = events
        .iter()
        .position(|event| event == "rate_acquire")
        .expect("rate limiter acquired");
    let classify = events
        .iter()
        .position(|event| event == "classify_response")
        .expect("response classified");
    let observe = events
        .iter()
        .position(|event| event == "rate_response")
        .expect("rate limiter observed response");
    assert_eq!(_server.sent_count().await, 1);
    assert!(acquire < classify);
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

fn response_with_retry_after(
    status: StatusCode,
    body: &'static str,
    retry_after: &'static str,
) -> MockResponse {
    let mut response = MockResponse::text(status, body);
    response.headers.insert(
        http::header::RETRY_AFTER,
        HeaderValue::from_static(retry_after),
    );
    response
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
    let _server = transport.clone();
    let mut client = client(TestAuthVars::default(), transport.clone());
    configure_runtime(
        &mut client,
        Some(Arc::new(ObservationRateLimiter::new(events.clone()))),
    );
    let _server = transport.clone();
    client.set_runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())));

    let err = client
        .request(ObservationFailureEndpoint {
            policy: Default::default(),
            request_body: Bytes::from_static(REQUEST_SENTINEL.as_bytes()),
        })
        .execute()
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
async fn rate_limit_observes_final_429_without_resending() -> Result<(), ApiClientError> {
    const FIRST_SENTINEL: &str = "PR65_RATE_LIMIT_FIRST_BODY_DO_NOT_LEAK";

    let events = Arc::new(Mutex::new(Vec::new()));
    let limiter = Arc::new(ObservationRateLimiter::new(events.clone()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(
            StatusCode::TOO_MANY_REQUESTS,
            FIRST_SENTINEL,
        )],
    );
    let _server = transport.clone();
    let sent_transport = transport.clone();
    let mut client = client(TestAuthVars::default(), transport.clone());
    client.set_runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())));
    configure_runtime(&mut client, Some(limiter));

    let err = client
        .request(TextEndpoint {
            policy: ResolvedPolicy::default(),
            ..Default::default()
        })
        .response()
        .await
        .expect_err("429 is terminal for the visible execution");

    assert!(matches!(err, ApiClientError::HttpStatus { .. }));
    assert_eq!(sent_transport.sent_count().await, 1);
    let events = events.lock().await.clone();
    first_event_with_prefix(&events, "rate_status:429 Too Many Requests");
    assert!(events.iter().any(|event| event.starts_with("rate_meta:")));
    assert!(
        events
            .iter()
            .any(|event| event.starts_with("rate_headers:"))
    );
    assert!(!events.iter().any(|event| event.contains(FIRST_SENTINEL)));
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
    let _server = transport.clone();
    let sent_transport = transport.clone();
    let mut client = observation_client(
        ObservationAuthVars::bearer("PR65_QUERY_SECRET_DO_NOT_LEAK", "user-a", events.clone()),
        &transport,
    );
    client.set_runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())));
    configure_runtime(&mut client, Some(limiter));

    let err = client
        .request(TextEndpoint {
            policy: auth_policy(concord_core::advanced::AuthPlacement::Query("api_key")),
            ..Default::default()
        })
        .response()
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
    let transport =
        MockTransport::with_outcomes(events.clone(), vec![MockOutcome::DisconnectAfterRequest]);
    let _server = transport.clone();
    let mut client = client(TestAuthVars::default(), transport.clone());
    configure_runtime(&mut client, Some(limiter));

    let err = client
        .request(TextEndpoint::default())
        .response()
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
    let transport = MockTransport::new(events.clone(), vec![]);
    let _server = transport.clone();
    let sent_transport = transport.clone();
    let mut client = client(TestAuthVars::default(), transport.clone());
    configure_runtime(&mut client, Some(limiter));

    let err = client
        .request(HostlessEndpoint {
            policy: ResolvedPolicy::default(),
        })
        .response()
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
    let _server = transport.clone();
    let mut client = observation_client(
        ObservationAuthVars::bearer(BEARER_SENTINEL, "bearer", events.clone()),
        &transport,
    );
    configure_runtime(&mut client, Some(limiter));

    let decoded = client
        .request(TextEndpoint {
            policy: auth_policy(concord_core::advanced::AuthPlacement::Bearer),
            ..Default::default()
        })
        .response()
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
    let _server = transport.clone();
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
        .response()
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
    let _server = transport.clone();
    let mut client = observation_client(
        ObservationAuthVars::basic(USER_SENTINEL, PASS_SENTINEL, "basic", events.clone()),
        &transport,
    );
    configure_runtime(&mut client, Some(limiter));

    let decoded = client
        .request(TextEndpoint {
            policy: auth_policy(concord_core::advanced::AuthPlacement::Basic),
            ..Default::default()
        })
        .response()
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
    let _server = transport.clone();
    let mut client = observation_client(
        ObservationAuthVars::bearer(BEARER_SENTINEL, "bearer", events.clone()),
        &transport,
    );
    configure_runtime(&mut client, Some(limiter));

    client
        .request(TextEndpoint {
            policy: auth_policy(concord_core::advanced::AuthPlacement::Bearer),
            ..Default::default()
        })
        .response()
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
    let _server = transport.clone();
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
        .response()
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
    let _server = transport.clone();
    let mut client = observation_client(
        ObservationAuthVars::basic(USER_SENTINEL, PASS_SENTINEL, "basic", events.clone()),
        &transport,
    );
    configure_runtime(&mut client, Some(limiter));

    client
        .request(TextEndpoint {
            policy: auth_policy(concord_core::advanced::AuthPlacement::Basic),
            ..Default::default()
        })
        .response()
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
async fn rate_limit_response_huge_delay_is_capped_before_storage() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let observer = Arc::new(RecordingRateLimitObserver::fixed(
        events.clone(),
        RateLimitObservation::limited().with_delay(Duration::MAX),
    ));
    let rate_limiter =
        Arc::new(concord_core::advanced::GovernorRateLimiter::new().with_response_policy(observer));
    static METHOD: Method = Method::GET;
    static URL: &str = "https://example.com/text";
    static ENDPOINT: &str = "Text";
    let plan = concord_core::advanced::RateLimitPlan::default();
    let headers = http::HeaderMap::new();
    let ctx = RateLimitResponseContext {
        meta: RateLimitContext {
            endpoint: ENDPOINT,
            method: &METHOD,
            url: URL,
            url_host: Some("example.com"),
            page_index: 0,
            idempotent: true,
            max_cooldown: Duration::from_secs(1),
            plan: &plan,
        },
        status: StatusCode::TOO_MANY_REQUESTS,
        headers: concord_core::advanced::SanitizedHeaders::new(&headers),
        max_cooldown: Duration::from_secs(1),
    };

    let action = rate_limiter
        .on_response(ctx)
        .await
        .expect("the configured maximum caps an unsafe delay");

    assert_eq!(action.retry_after(), Some(Duration::from_secs(1)));
    assert!(action.cooldown_stored());
    let events = events.lock().await.clone();
    assert!(
        events
            .iter()
            .any(|event| event.starts_with("rate_observe_status:429"))
    );
}

#[tokio::test]
async fn rate_limit_response_above_cap_is_terminal_and_not_resent() {
    const RESPONSE_SENTINEL: &str = "PRSEC7_RATE_LIMIT_RESPONSE_SENTINEL";

    let transport = MockTransport::new(
        Arc::new(Mutex::new(Vec::new())),
        vec![response_with_retry_after(
            StatusCode::TOO_MANY_REQUESTS,
            RESPONSE_SENTINEL,
            "2",
        )],
    );
    let _server = transport.clone();
    let sent_transport = transport.clone();
    let mut client = client(TestAuthVars::default(), transport.clone());
    client.configure(|cfg| {
        cfg.max_rate_limit_cooldown(Duration::from_secs(1));
    });

    let err = client
        .request(TextEndpoint {
            policy: ResolvedPolicy::default(),
            ..Default::default()
        })
        .response()
        .await
        .expect_err("the final 429 should remain terminal");

    assert!(matches!(err, ApiClientError::HttpStatus { .. }));
    assert_eq!(sent_transport.sent_count().await, 1);
    assert_error_chain_does_not_contain_any(&err, &[RESPONSE_SENTINEL]);
}

#[tokio::test]
async fn rate_limit_response_above_cap_delays_only_the_followup_request()
-> Result<(), ApiClientError> {
    const RESPONSE_SENTINEL: &str = "PRSEC7_RATE_LIMIT_POISON_SENTINEL";

    let transport = MockTransport::new(
        Arc::new(Mutex::new(Vec::new())),
        vec![
            response_with_retry_after(StatusCode::TOO_MANY_REQUESTS, RESPONSE_SENTINEL, "2"),
            MockResponse::text(StatusCode::OK, "ok"),
        ],
    );
    let _server = transport.clone();
    let sent_transport = transport.clone();
    let mut client = client(TestAuthVars::default(), transport.clone());
    client.configure(|cfg| {
        cfg.max_rate_limit_cooldown(Duration::from_millis(20));
    });

    let err = client
        .request(TextEndpoint {
            policy: ResolvedPolicy::default(),
            ..Default::default()
        })
        .response()
        .await
        .expect_err("the final 429 should remain terminal");
    assert!(matches!(err, ApiClientError::HttpStatus { .. }));
    assert_eq!(sent_transport.sent_count().await, 1);

    let later = tokio::time::timeout(
        Duration::from_millis(250),
        client
            .request(TextEndpoint {
                policy: ResolvedPolicy::default(),
                ..Default::default()
            })
            .response(),
    )
    .await
    .expect("the capped future-call cooldown should complete")
    .expect("follow-up request should succeed");

    assert_eq!(later.value(), "ok");
    assert_eq!(sent_transport.sent_count().await, 2);
    assert_error_chain_does_not_contain_any(&err, &[RESPONSE_SENTINEL]);
    Ok(())
}

#[tokio::test]
async fn short_rate_limit_cooldown_still_allows_followup_requests() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let observer = Arc::new(RecordingRateLimitObserver::fixed(
        events.clone(),
        RateLimitObservation::limited().with_delay(Duration::from_millis(1)),
    ));
    let rate_limiter =
        Arc::new(concord_core::advanced::GovernorRateLimiter::new().with_response_policy(observer));
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::TOO_MANY_REQUESTS, "retry-me"),
            MockResponse::text(StatusCode::OK, "ok"),
        ],
    );
    let _server = transport.clone();
    let sent_transport = transport.clone();
    let mut client = client(TestAuthVars::default(), transport.clone());
    client.configure(|cfg| {
        cfg.max_rate_limit_cooldown(Duration::from_secs(1));
    });
    configure_runtime(&mut client, Some(rate_limiter));

    let first = client
        .request(TextEndpoint {
            policy: ResolvedPolicy::default(),
            ..Default::default()
        })
        .response()
        .await
        .expect_err("429 remains terminal for the current call");
    assert!(matches!(first, ApiClientError::HttpStatus { .. }));

    let decoded = client
        .request(TextEndpoint {
            policy: ResolvedPolicy::default(),
            ..Default::default()
        })
        .response()
        .await?;

    assert_eq!(decoded.value(), "ok");
    assert_eq!(sent_transport.sent_count().await, 2);
    Ok(())
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
    let _server = transport.clone();
    let sent_transport = transport.clone();
    let mut client = client(TestAuthVars::default(), transport.clone());
    configure_runtime(&mut client, Some(rate_limiter));

    let first = client
        .request(TextEndpoint {
            policy: ResolvedPolicy::default(),
            ..Default::default()
        })
        .response()
        .await
        .expect_err("500 remains terminal for the current call");
    assert!(matches!(first, ApiClientError::HttpStatus { .. }));

    let decoded = client
        .request(TextEndpoint {
            policy: ResolvedPolicy::default(),
            ..Default::default()
        })
        .response()
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
    let _server = transport.clone();
    let sent_transport = transport.clone();
    let mut client = observation_client(
        ObservationAuthVars::bearer("PR67_BEARER_SECRET_DO_NOT_LEAK", "bearer", events.clone()),
        &transport,
    );
    configure_runtime(&mut client, Some(rate_limiter));

    let err = client
        .request(TextEndpoint {
            policy: auth_policy(concord_core::advanced::AuthPlacement::Bearer),
            ..Default::default()
        })
        .response()
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
async fn rate_limit_response_context_sanitizes_sensitive_response_headers()
-> Result<(), ApiClientError> {
    const SET_COOKIE_SENTINEL: &str = "LEAK_SENTINEL_SET_COOKIE";
    const WWW_AUTHENTICATE_SENTINEL: &str = "LEAK_SENTINEL_WWW_AUTH";
    const REFRESH_TOKEN_SENTINEL: &str = "LEAK_SENTINEL_REFRESH_TOKEN";

    let events = Arc::new(Mutex::new(Vec::new()));
    let limiter = Arc::new(SanitizedResponseHeaderRateLimiter::new(events.clone()));
    let transport = MockTransport::new(
        events.clone(),
        vec![{
            let mut response = MockResponse::text(StatusCode::TOO_MANY_REQUESTS, "retry-me");
            response.headers.insert(
                http::header::SET_COOKIE,
                HeaderValue::from_static(SET_COOKIE_SENTINEL),
            );
            response.headers.insert(
                http::header::WWW_AUTHENTICATE,
                HeaderValue::from_static(WWW_AUTHENTICATE_SENTINEL),
            );
            response.headers.insert(
                http::HeaderName::from_static("x-refresh-token"),
                HeaderValue::from_static(REFRESH_TOKEN_SENTINEL),
            );
            response
                .headers
                .insert(http::header::RETRY_AFTER, HeaderValue::from_static("3"));
            response.headers.insert(
                http::HeaderName::from_static("x-rate-limit-remaining"),
                HeaderValue::from_static("7"),
            );
            response
        }],
    );
    let _server = transport.clone();
    let sent_transport = transport.clone();
    let mut client = client(TestAuthVars::default(), transport.clone());
    configure_runtime(&mut client, Some(limiter));

    let err = client
        .request(TextEndpoint::default())
        .response()
        .await
        .expect_err("429 should surface as a typed status error");

    assert!(matches!(err, ApiClientError::HttpStatus { .. }));
    assert_eq!(err.category(), ErrorCategory::HttpStatus);
    assert_eq!(sent_transport.sent_count().await, 1);
    let events = events.lock().await.clone();
    assert!(
        events
            .iter()
            .any(|event| event == "sanitized_cookie:Some(\"<redacted>\"):contains:true")
    );
    assert!(
        events
            .iter()
            .any(|event| event == "sanitized_authenticate:Some(\"<redacted>\")")
    );
    assert!(
        events
            .iter()
            .any(|event| event == "sanitized_refresh:Some(\"<redacted>\")")
    );
    assert!(
        events
            .iter()
            .any(|event| event == "sanitized_retry_after:Some(\"3\")")
    );
    assert!(
        events
            .iter()
            .any(|event| event == "sanitized_rate_limit_remaining:Some(\"7\")")
    );
    assert_error_chain_does_not_contain_any(
        &err,
        &[
            SET_COOKIE_SENTINEL,
            WWW_AUTHENTICATE_SENTINEL,
            REFRESH_TOKEN_SENTINEL,
        ],
    );
    Ok(())
}

#[tokio::test]
async fn retry_after_429_delays_only_a_future_call() -> Result<(), ApiClientError> {
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
            },
            MockResponse::text(StatusCode::OK, "ok"),
        ],
    );
    let _server = transport.clone();
    let sent_transport = transport.clone();
    let mut client = client(TestAuthVars::default(), transport.clone());
    configure_runtime(&mut client, Some(rate_limiter));

    let first = client
        .request(TextEndpoint {
            policy: ResolvedPolicy::default(),
            ..Default::default()
        })
        .response()
        .await
        .expect_err("429 remains terminal for the current call");
    assert!(matches!(first, ApiClientError::HttpStatus { .. }));
    assert_eq!(sent_transport.sent_count().await, 1);

    let decoded = tokio::time::timeout(
        Duration::from_secs(3),
        client
            .request(TextEndpoint {
                policy: ResolvedPolicy::default(),
                ..Default::default()
            })
            .response(),
    )
    .await
    .expect("future call should finish after one cooldown")?;

    assert_eq!(decoded.value(), "ok");
    assert!(started.elapsed() >= Duration::from_millis(900));
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(sent_transport.sent_count().await, 2);
    let events = events.lock().await.clone();
    let first = first_event_with_prefix(&events, "rate_observe_status:429 Too Many Requests");
    let second = first_event_with_prefix(&events, "rate_observe_status:200 OK");
    assert!(first < second);
    Ok(())
}

#[cfg(not(feature = "rate-limit-governor"))]
#[tokio::test]
async fn no_default_rate_limit_empty_plan_succeeds() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, "ok")]);
    let _server = transport.clone();
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport.clone());

    let decoded = client.request(TextEndpoint::default()).response().await?;

    assert_eq!(decoded.value(), "ok");
    assert_eq!(sent.sent_count().await, 1);
    Ok(())
}

#[cfg(not(feature = "rate-limit-governor"))]
#[tokio::test]
async fn no_default_rate_limit_non_empty_plan_fails_closed() {
    const AUTH_SENTINEL: &str = "PRSEC8_NO_DEFAULT_AUTH_SENTINEL";

    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![MockResponse::text(StatusCode::OK, "should-not-send")],
    );
    let _server = transport.clone();
    let sent = transport.clone();
    let client = client(
        TestAuthVars {
            token: Some(AUTH_SENTINEL.to_string()),
            identity: "bearer",
        },
        transport.clone(),
    );
    let mut policy = rate_limit_policy();
    policy.auth = auth_policy(concord_core::advanced::AuthPlacement::Bearer).auth;

    let err = client
        .request(TextEndpoint {
            policy,
            ..Default::default()
        })
        .response()
        .await
        .expect_err("non-empty rate-limit plans should fail closed without governor support");

    assert_eq!(
        err.category(),
        concord_core::error::ErrorCategory::RateLimit
    );
    assert_eq!(
        err.rate_limit_error().map(|err| err.kind()),
        Some(concord_core::advanced::RateLimitErrorKind::InvalidConfiguration)
    );
    assert!(err.to_string().contains("explicit opt-out"));
    assert_eq!(sent.sent_count().await, 0);
    assert_error_chain_does_not_contain_any(&err, &[AUTH_SENTINEL]);
}

#[cfg(not(feature = "rate-limit-governor"))]
#[tokio::test]
async fn no_default_rate_limit_explicit_noop_limiter_opt_out_succeeds() -> Result<(), ApiClientError>
{
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, "ok")]);
    let _server = transport.clone();
    let sent = transport.clone();
    let mut client = client(TestAuthVars::default(), transport.clone());
    client.configure(|cfg| {
        cfg.rate_limiter(Arc::new(concord_core::advanced::NoopRateLimiter::new()));
    });

    let decoded = client
        .request(TextEndpoint {
            policy: rate_limit_policy(),
            ..Default::default()
        })
        .response()
        .await?;

    assert_eq!(decoded.value(), "ok");
    assert_eq!(sent.sent_count().await, 1);
    Ok(())
}

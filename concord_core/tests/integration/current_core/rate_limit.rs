use super::common::*;

use bytes::Bytes;
use concord_core::internal::{
    BodyPlan, ClientPlanContext, EndpointMeta, EndpointPlan, RequestArgs, RequestOverrides,
    RequestPlan, ResolvedPolicy, ResolvedRoute, ResponsePlan,
};
use concord_core::prelude::ApiClientError;
use concord_core::prelude::Endpoint;
use http::{HeaderValue, Method, StatusCode};
use std::sync::Arc;
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
    configure_runtime(&mut client, None, Some(limiter));

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
    let cache = Arc::new(RecordingCache::miss(events.clone()));
    let after_response_count = cache.after_response_count.clone();
    let mut response = MockResponse::text(StatusCode::OK, RESPONSE_SENTINEL);
    response.headers.insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    let transport = MockTransport::new(events.clone(), vec![response]);
    let mut client = client(TestAuthVars::default(), transport);
    configure_runtime(
        &mut client,
        Some(cache),
        Some(Arc::new(ObservationRateLimiter::new(events.clone()))),
    );
    client.set_runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())));

    let err = client
        .request(ObservationFailureEndpoint {
            policy: cache_policy(),
            request_body: Bytes::from_static(REQUEST_SENTINEL.as_bytes()),
        })
        .execute_decoded()
        .await
        .expect_err("invalid payload should fail decode");

    assert_eq!(err.category(), concord_core::error::ErrorCategory::Decode);
    assert_eq!(*after_response_count.lock().await, 0);
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
    configure_runtime(&mut client, None, Some(limiter));

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
    configure_runtime(&mut client, None, Some(limiter));

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
    configure_runtime(&mut client, None, Some(limiter));

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

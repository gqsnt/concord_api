use super::common::{
    MockOutcome, MockResponse, MockTransport, TestAuthVars, TestCx, buffered_endpoint_execute,
    buffered_endpoint_response_terminal, client,
};
use bytes::Bytes;
use concord_core::advanced::{RetryBackoff, RetryConfig, RetryIdempotency, StreamBody};
use concord_core::error::ErrorCategory;
use concord_core::internal::{
    BodyPlan, ClientPlanContext, EndpointMeta, EndpointPlan, Format, Replayability, RequestArgs,
    RequestOverrides, RequestPlan, ResolvedPolicy, ResolvedRoute, ResponsePlan, RetrySetting,
};
use concord_core::prelude::{ApiClientError, Endpoint, ReusableEndpoint};
use http::{HeaderValue, Method, StatusCode};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

use crate::support::{RedactionSentinels, assert_error_chain_does_not_contain_any};

#[derive(Clone)]
struct AttemptEndpoint {
    name: &'static str,
    method: Method,
    path: &'static str,
    idempotent: bool,
    policy: ResolvedPolicy,
    body: AttemptBody,
}

#[derive(Clone)]
enum AttemptBody {
    Bytes(Bytes),
    Stream(Bytes),
}

impl AttemptBody {
    fn plan_parts(&self) -> (BodyPlan, RequestArgs, Replayability) {
        match self {
            Self::Bytes(body) => (
                BodyPlan::Encoded {
                    content_type: Some(HeaderValue::from_static("application/json")),
                    format: Format::Text,
                },
                RequestArgs::with_body_bytes(body.clone()),
                Replayability::Replayable,
            ),
            Self::Stream(body) => (
                BodyPlan::RawStream {
                    content_type: HeaderValue::from_static("application/octet-stream"),
                },
                RequestArgs::with_stream_body(StreamBody::from_bytes(body.clone())),
                Replayability::NonReplayable,
            ),
        }
    }
}

impl AttemptEndpoint {
    fn new(
        name: &'static str,
        method: Method,
        path: &'static str,
        idempotent: bool,
        policy: ResolvedPolicy,
        body: AttemptBody,
    ) -> Self {
        Self {
            name,
            method,
            path,
            idempotent,
            policy,
            body,
        }
    }

    fn request_plan(&self) -> RequestPlan {
        let (body, args, replayability) = self.body.plan_parts();
        RequestPlan {
            endpoint: EndpointPlan {
                meta: EndpointMeta {
                    name: self.name,
                    method: self.method.clone(),
                    idempotent: self.idempotent,
                    facade_path: &[],
                },
                route: ResolvedRoute::new(http::uri::Scheme::HTTPS, "example.com", self.path),
                policy: self.policy.clone(),
                body,
                response: ResponsePlan {
                    accept: Some(HeaderValue::from_static("text/plain")),
                    no_content: false,
                    format: Format::Text,
                },
                pagination: None,
            },
            args,
            overrides: RequestOverrides::default(),
            replayability,
        }
    }
}

impl Endpoint<TestCx> for AttemptEndpoint {
    type Response = String;

    buffered_endpoint_execute!(TestCx, concord_core::prelude::Text<String>);
}

buffered_endpoint_response_terminal!(AttemptEndpoint, TestCx, concord_core::prelude::Text<String>);

impl ReusableEndpoint<TestCx> for AttemptEndpoint {
    fn plan(&self, _ctx: &ClientPlanContext<'_, TestCx>) -> Result<RequestPlan, ApiClientError> {
        Ok(self.request_plan())
    }
}

fn finalized_attempt_policy() -> ResolvedPolicy {
    ResolvedPolicy {
        timeout: Some(Duration::from_millis(250)),
        headers: [
            ("x-client", HeaderValue::from_static("client")),
            ("x-scope", HeaderValue::from_static("scope")),
            ("x-endpoint", HeaderValue::from_static("endpoint")),
        ]
        .into_iter()
        .map(|(name, value)| (http::header::HeaderName::from_static(name), value))
        .collect(),
        query: vec![
            ("client".to_string(), "1".to_string()),
            ("scope".to_string(), "2".to_string()),
            ("endpoint".to_string(), "3".to_string()),
        ],
        ..Default::default()
    }
}

fn replayable_policy() -> ResolvedPolicy {
    ResolvedPolicy {
        retry: RetrySetting::Config(RetryConfig {
            max_attempts: 2,
            methods: vec![Method::PUT],
            statuses: vec![StatusCode::INTERNAL_SERVER_ERROR],
            transport_errors: Vec::new(),
            backoff: RetryBackoff::None,
            respect_retry_after: true,
            idempotency: RetryIdempotency::SafeMethodsOnly,
        }),
        ..Default::default()
    }
}

fn attempt_sentinels() -> RedactionSentinels {
    RedactionSentinels::new(
        "ATTEMPT_AUTH_SENTINEL",
        "ATTEMPT_BODY_SENTINEL",
        "ATTEMPT_RESPONSE_SENTINEL",
    )
}

#[tokio::test]
async fn finalized_attempt_request_is_sent_once() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, "ok")]);
    let sent_transport = transport.clone();
    let client = client(TestAuthVars::default(), transport);
    let endpoint = AttemptEndpoint::new(
        "FinalizedAttempt",
        Method::POST,
        "/attempt/finalized",
        false,
        finalized_attempt_policy(),
        AttemptBody::Bytes(Bytes::from_static(b"{\"attempt\":true}")),
    );

    let raw = client.request(endpoint).execute_raw().await?;

    assert_eq!(raw.status, StatusCode::OK);
    assert_eq!(sent_transport.sent_count().await, 1);
    let requests = sent_transport.requests().await;
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.meta.endpoint, "FinalizedAttempt");
    assert_eq!(request.meta.method, Method::POST);
    assert_eq!(request.meta.attempt, 0);
    assert_eq!(
        request.url.as_str(),
        "https://example.com/attempt/finalized?client=1&scope=2&endpoint=3"
    );
    assert_eq!(request.timeout, Some(Duration::from_millis(250)));
    assert_eq!(
        request.headers.get("x-client").unwrap().to_str().unwrap(),
        "client"
    );
    assert_eq!(
        request.headers.get("x-scope").unwrap().to_str().unwrap(),
        "scope"
    );
    assert_eq!(
        request.headers.get("x-endpoint").unwrap().to_str().unwrap(),
        "endpoint"
    );
    assert_eq!(
        request
            .headers
            .get(http::header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap(),
        "application/json"
    );
    let expected_body = Bytes::from_static(b"{\"attempt\":true}");
    assert_eq!(request.body.as_bytes(), Some(&expected_body));
    Ok(())
}

#[tokio::test]
async fn execute_raw_and_decoded_share_the_same_attempt_path() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "shared-response"),
            MockResponse::text(StatusCode::OK, "shared-response"),
        ],
    );
    let sent_transport = transport.clone();
    let client = client(TestAuthVars::default(), transport);
    let endpoint = AttemptEndpoint::new(
        "SharedAttempt",
        Method::POST,
        "/attempt/shared",
        false,
        finalized_attempt_policy(),
        AttemptBody::Bytes(Bytes::from_static(b"shared-request")),
    );

    let raw = client.request(endpoint.clone()).execute_raw().await?;
    let decoded = client.request(endpoint).response().await?;

    assert_eq!(raw.body, Bytes::from_static(b"shared-response"));
    assert_eq!(decoded.into_value(), "shared-response");
    let requests = sent_transport.requests().await;
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].meta.endpoint, requests[1].meta.endpoint);
    assert_eq!(requests[0].meta.method, requests[1].meta.method);
    assert_eq!(requests[0].url, requests[1].url);
    assert_eq!(requests[0].headers, requests[1].headers);
    assert_eq!(requests[0].body.as_bytes(), requests[1].body.as_bytes());
    Ok(())
}

#[tokio::test]
async fn http_status_errors_remain_typed_and_redacted() {
    let sentinels = attempt_sentinels();
    let events = Arc::new(Mutex::new(Vec::new()));
    let response_reads = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, sentinels.response)
                .with_read_count(response_reads.clone()),
        ],
    );
    let client = client(TestAuthVars::default(), transport.clone());
    let mut policy = ResolvedPolicy::default();
    policy
        .headers
        .insert("x-auth", HeaderValue::from_static(sentinels.auth));
    let endpoint = AttemptEndpoint::new(
        "StatusSafety",
        Method::GET,
        "/attempt/status",
        true,
        policy,
        AttemptBody::Bytes(Bytes::from_static(sentinels.body.as_bytes())),
    );

    let err = client
        .request(endpoint)
        .response()
        .await
        .expect_err("HTTP 500 should surface as a status error");

    assert!(matches!(err, ApiClientError::HttpStatus { .. }));
    assert_eq!(err.category(), ErrorCategory::HttpStatus);
    assert_eq!(err.context().endpoint, "StatusSafety");
    assert_eq!(err.context().method, Method::GET);
    assert_eq!(err.http_status(), Some(StatusCode::INTERNAL_SERVER_ERROR));
    assert_eq!(response_reads.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert_error_chain_does_not_contain_any(&err, &sentinels.all());
    let requests = transport.requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].url.path(), "/attempt/status");
    assert_eq!(
        requests[0].body.as_bytes(),
        Some(&Bytes::from_static(sentinels.body.as_bytes()))
    );
}

#[tokio::test]
async fn transport_errors_remain_typed_and_redacted() {
    let sentinels = attempt_sentinels();
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::with_outcomes(
        events,
        vec![MockOutcome::TransportError(
            concord_core::transport::TransportErrorKind::Dns,
        )],
    );
    let client = client(TestAuthVars::default(), transport.clone());
    let mut policy = ResolvedPolicy::default();
    policy
        .headers
        .insert("x-auth", HeaderValue::from_static(sentinels.auth));
    let endpoint = AttemptEndpoint::new(
        "TransportSafety",
        Method::POST,
        "/attempt/transport",
        false,
        policy,
        AttemptBody::Bytes(Bytes::from_static(sentinels.body.as_bytes())),
    );

    let err = client
        .request(endpoint)
        .execute_raw()
        .await
        .expect_err("transport failure should surface as transport error");

    assert!(matches!(err, ApiClientError::Transport { .. }));
    assert_eq!(err.category(), ErrorCategory::Transport);
    assert_eq!(err.context().endpoint, "TransportSafety");
    assert_eq!(err.context().method, Method::POST);
    assert_error_chain_does_not_contain_any(&err, &sentinels.all());
    let requests = transport.requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].url.path(), "/attempt/transport");
    assert_eq!(
        requests[0].body.as_bytes(),
        Some(&Bytes::from_static(sentinels.body.as_bytes()))
    );
}

#[tokio::test]
async fn replayable_encoded_bodies_can_retry_with_the_same_payload() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, "retry-me"),
            MockResponse::text(StatusCode::OK, "retry-ok"),
        ],
    );
    let sent_transport = transport.clone();
    let client = client(TestAuthVars::default(), transport);
    let mut policy = replayable_policy();
    policy.timeout = Some(Duration::from_millis(100));
    let endpoint = AttemptEndpoint::new(
        "ReplayableAttempt",
        Method::PUT,
        "/attempt/replayable",
        true,
        policy,
        AttemptBody::Bytes(Bytes::from_static(b"replayable-body")),
    );

    let raw = client.request(endpoint).execute_raw().await?;

    assert_eq!(raw.status, StatusCode::OK);
    assert_eq!(sent_transport.sent_count().await, 2);
    let requests = sent_transport.requests().await;
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].meta.attempt, 0);
    assert_eq!(requests[1].meta.attempt, 1);
    assert_eq!(requests[0].body.as_bytes(), requests[1].body.as_bytes());
    assert_eq!(
        requests[0].body.as_bytes(),
        Some(&Bytes::from_static(b"replayable-body"))
    );
    assert_eq!(
        requests[1].body.as_bytes(),
        Some(&Bytes::from_static(b"replayable-body"))
    );
    Ok(())
}

#[tokio::test]
async fn non_replayable_stream_bodies_stop_after_the_first_attempt() {
    let sentinels = attempt_sentinels();
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, sentinels.response),
            MockResponse::text(StatusCode::OK, "unused"),
        ],
    );
    let sent_transport = transport.clone();
    let client = client(TestAuthVars::default(), transport);
    let endpoint = AttemptEndpoint::new(
        "NonReplayableAttempt",
        Method::PUT,
        "/attempt/non-replayable",
        true,
        replayable_policy(),
        AttemptBody::Stream(Bytes::from_static(sentinels.body.as_bytes())),
    );

    let err = client
        .request(endpoint)
        .execute_raw()
        .await
        .expect_err("stream bodies should not be silently retried");

    assert!(matches!(err, ApiClientError::HttpStatus { .. }));
    assert_eq!(err.category(), ErrorCategory::HttpStatus);
    assert_eq!(err.context().endpoint, "NonReplayableAttempt");
    assert_eq!(err.context().method, Method::PUT);
    assert_error_chain_does_not_contain_any(&err, &sentinels.all());
    assert_eq!(sent_transport.sent_count().await, 1);
    let requests = sent_transport.requests().await;
    assert_eq!(requests.len(), 1);
    assert!(requests[0].body.is_stream());
}

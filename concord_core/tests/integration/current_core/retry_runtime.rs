use super::common::{
    MockOutcome, MockResponse, MockTransport, TestAuthVars, client, request_plan, retry_policy,
    retry_policy_for_statuses, retry_policy_for_transport_errors,
};
use crate::support::assert_error_chain_does_not_contain_any;
use bytes::Bytes;
use concord_core::advanced::{
    RetryContext, RetryDecision, RetryIdempotency, RetryOutcome, RetryPolicy, StreamBody,
};
use concord_core::error::ErrorCategory;
use concord_core::internal::{PreparedBody, ResolvedPolicy, RetrySetting};
use concord_core::prelude::ApiClientError;
use http::{HeaderValue, Method, StatusCode};
use std::sync::Arc;
use tokio::sync::Mutex;

fn retry_plan(
    name: &'static str,
    method: Method,
    path: &'static str,
    policy: ResolvedPolicy,
    idempotent: bool,
) -> concord_core::internal::RequestPlan {
    let mut plan = request_plan(name, method, path, policy, None);
    plan.endpoint.meta.idempotent = idempotent;
    plan
}

fn retry_encoded_plan(
    name: &'static str,
    method: Method,
    path: &'static str,
    policy: ResolvedPolicy,
    body: &'static [u8],
    idempotent: bool,
) -> concord_core::internal::RequestPlan {
    let mut plan = retry_plan(name, method, path, policy, idempotent);
    plan.body = PreparedBody::reusable_bytes(
        Bytes::from_static(body),
        Some(HeaderValue::from_static("application/json")),
    );
    plan
}

fn retry_stream_plan(
    name: &'static str,
    method: Method,
    path: &'static str,
    policy: ResolvedPolicy,
    body: &'static [u8],
    idempotent: bool,
) -> concord_core::internal::RequestPlan {
    let mut plan = retry_plan(name, method, path, policy, idempotent);
    plan.body = PreparedBody::from_stream_body(
        StreamBody::from_bytes(Bytes::from_static(body)),
        Some(HeaderValue::from_static("application/octet-stream")),
    );
    plan
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

#[derive(Clone, Copy)]
struct InheritedStatusRetry;

impl RetryPolicy for InheritedStatusRetry {
    fn should_retry(&self, ctx: &RetryContext<'_>) -> RetryDecision {
        if matches!(
            &ctx.outcome,
            RetryOutcome::HttpStatus(StatusCode::TOO_MANY_REQUESTS)
        ) {
            RetryDecision::Retry
        } else {
            RetryDecision::Stop
        }
    }
}

#[tokio::test]
async fn retryable_status_retries_then_succeeds_and_records_attempt_indexes()
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

    let decoded = client
        .execute_plan::<concord_core::prelude::Text<String>>(retry_plan(
            "RetryStatus",
            Method::GET,
            "/retry/status",
            retry_policy(2),
            true,
        ))
        .await?;

    assert_eq!(decoded.value(), "ok");
    assert_eq!(sent.sent_count().await, 2);
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].meta.attempt, 0);
    assert_eq!(requests[1].meta.attempt, 1);
    Ok(())
}

#[tokio::test]
async fn unconfigured_status_does_not_retry() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::BAD_REQUEST, "bad-request"),
            MockResponse::text(StatusCode::OK, "should-not-send"),
        ],
    );
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let err = client
        .execute_plan::<concord_core::prelude::Text<String>>(retry_plan(
            "RetryStatusMismatch",
            Method::GET,
            "/retry/status-mismatch",
            retry_policy(2),
            true,
        ))
        .await
        .expect_err("unconfigured status should not retry");

    assert!(matches!(err, ApiClientError::HttpStatus { .. }));
    assert_eq!(err.category(), ErrorCategory::HttpStatus);
    assert_eq!(err.context().endpoint, "RetryStatusMismatch");
    assert_eq!(err.context().method, Method::GET);
    assert_eq!(err.http_status(), Some(StatusCode::BAD_REQUEST));
    assert_eq!(sent.sent_count().await, 1);
}

#[tokio::test]
async fn retry_status_exhaustion_redacts_request_and_response_sentinels() {
    const AUTH_SENTINEL: &str = "PR18_RETRY_AUTH_SENTINEL";
    const RESPONSE_SENTINEL: &str = "PR18_RETRY_RESPONSE_SENTINEL";

    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, "retry-one"),
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, RESPONSE_SENTINEL),
        ],
    );
    let sent = transport.clone();
    let mut policy = retry_policy(2);
    policy
        .headers
        .insert("x-auth", HeaderValue::from_static(AUTH_SENTINEL));
    let client = client(TestAuthVars::default(), transport);

    let err = client
        .execute_plan::<concord_core::prelude::Text<String>>(retry_plan(
            "RetryStatusRedacted",
            Method::GET,
            "/retry/status-redacted",
            policy,
            true,
        ))
        .await
        .expect_err("retry exhaustion should surface as a status error");

    assert!(matches!(err, ApiClientError::HttpStatus { .. }));
    assert_eq!(err.category(), ErrorCategory::HttpStatus);
    assert_eq!(err.context().endpoint, "RetryStatusRedacted");
    assert_eq!(err.context().method, Method::GET);
    assert_eq!(err.http_status(), Some(StatusCode::INTERNAL_SERVER_ERROR));
    assert_eq!(sent.sent_count().await, 2);
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0]
            .headers
            .get("x-auth")
            .and_then(|value| value.to_str().ok()),
        Some(AUTH_SENTINEL)
    );
    assert_eq!(
        requests[1]
            .headers
            .get("x-auth")
            .and_then(|value| value.to_str().ok()),
        Some(AUTH_SENTINEL)
    );
    assert_error_chain_does_not_contain_any(&err, &[AUTH_SENTINEL, RESPONSE_SENTINEL]);
}

#[tokio::test]
async fn retryable_transport_error_retries_then_succeeds_and_records_attempt_indexes()
-> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::with_outcomes(
        events,
        vec![
            MockOutcome::TransportError(concord_core::transport::TransportErrorKind::Timeout),
            MockResponse::text(StatusCode::OK, "ok").into(),
        ],
    );
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let decoded = client
        .execute_plan::<concord_core::prelude::Text<String>>(retry_plan(
            "RetryTransport",
            Method::GET,
            "/retry/transport",
            retry_policy_for_transport_errors(
                2,
                vec![concord_core::transport::TransportErrorKind::Timeout],
            ),
            true,
        ))
        .await?;

    assert_eq!(decoded.value(), "ok");
    assert_eq!(sent.sent_count().await, 2);
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].meta.attempt, 0);
    assert_eq!(requests[1].meta.attempt, 1);
    Ok(())
}

#[tokio::test]
async fn unconfigured_transport_error_does_not_retry() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::with_outcomes(
        events,
        vec![
            MockOutcome::TransportError(concord_core::transport::TransportErrorKind::Connect),
            MockResponse::text(StatusCode::OK, "should-not-send").into(),
        ],
    );
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let err = client
        .execute_plan::<concord_core::prelude::Text<String>>(retry_plan(
            "RetryTransportMismatch",
            Method::GET,
            "/retry/transport-mismatch",
            retry_policy_for_transport_errors(
                2,
                vec![concord_core::transport::TransportErrorKind::Timeout],
            ),
            true,
        ))
        .await
        .expect_err("unconfigured transport error should not retry");

    assert!(matches!(err, ApiClientError::Transport { .. }));
    assert_eq!(err.category(), ErrorCategory::Transport);
    assert_eq!(err.context().endpoint, "RetryTransportMismatch");
    assert_eq!(err.context().method, Method::GET);
    assert_eq!(sent.sent_count().await, 1);
}

#[tokio::test]
async fn transport_error_exhaustion_redacts_request_sentinel_and_reports_context() {
    const AUTH_SENTINEL: &str = "PR18_RETRY_AUTH_SENTINEL";

    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::with_outcomes(
        events,
        vec![
            MockOutcome::TransportError(concord_core::transport::TransportErrorKind::Timeout),
            MockOutcome::TransportError(concord_core::transport::TransportErrorKind::Timeout),
        ],
    );
    let sent = transport.clone();
    let mut policy = retry_policy_for_transport_errors(
        2,
        vec![concord_core::transport::TransportErrorKind::Timeout],
    );
    policy
        .headers
        .insert("x-auth", HeaderValue::from_static(AUTH_SENTINEL));
    let client = client(TestAuthVars::default(), transport);

    let err = client
        .execute_plan::<concord_core::prelude::Text<String>>(retry_plan(
            "RetryTransportRedacted",
            Method::GET,
            "/retry/transport-redacted",
            policy,
            true,
        ))
        .await
        .expect_err("retry exhaustion should return a transport error");

    assert!(matches!(err, ApiClientError::Transport { .. }));
    assert_eq!(err.category(), ErrorCategory::Timeout);
    assert_eq!(err.context().endpoint, "RetryTransportRedacted");
    assert_eq!(err.context().method, Method::GET);
    assert_eq!(sent.sent_count().await, 2);
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0]
            .headers
            .get("x-auth")
            .and_then(|value| value.to_str().ok()),
        Some(AUTH_SENTINEL)
    );
    assert_eq!(
        requests[1]
            .headers
            .get("x-auth")
            .and_then(|value| value.to_str().ok()),
        Some(AUTH_SENTINEL)
    );
    assert_error_chain_does_not_contain_any(&err, &[AUTH_SENTINEL]);
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
    let mut policy = retry_policy(2);
    if let RetrySetting::Config(config) = &mut policy.retry {
        config.methods = vec![Method::POST];
        config.idempotency = RetryIdempotency::SafeMethodsOnly;
    }
    let client = client(TestAuthVars::default(), transport);

    let err = client
        .execute_plan::<concord_core::prelude::Text<String>>(retry_plan(
            "RetryUnsafeNoHeader",
            Method::POST,
            "/retry/unsafe",
            policy,
            false,
        ))
        .await
        .expect_err("unsafe request without idempotency should not retry");

    assert!(matches!(err, ApiClientError::HttpStatus { .. }));
    assert_eq!(err.category(), ErrorCategory::HttpStatus);
    assert_eq!(err.context().endpoint, "RetryUnsafeNoHeader");
    assert_eq!(err.context().method, Method::POST);
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
    let header = http::HeaderName::from_static("idempotency-key");
    let mut policy = retry_policy(2);
    if let RetrySetting::Config(config) = &mut policy.retry {
        config.methods = vec![Method::POST];
        config.idempotency = RetryIdempotency::Header(header.clone());
    }
    policy
        .headers
        .insert(header.clone(), HeaderValue::from_static("stable-key"));
    let client = client(TestAuthVars::default(), transport);

    let decoded = client
        .execute_plan::<concord_core::prelude::Text<String>>(retry_plan(
            "RetryUnsafeWithHeader",
            Method::POST,
            "/retry/unsafe-with-header",
            policy,
            false,
        ))
        .await?;

    assert_eq!(decoded.value(), "ok");
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0]
            .headers
            .get(&header)
            .and_then(|value| value.to_str().ok()),
        Some("stable-key")
    );
    assert_eq!(
        requests[1]
            .headers
            .get(&header)
            .and_then(|value| value.to_str().ok()),
        Some("stable-key")
    );
    Ok(())
}

#[tokio::test]
async fn retry_after_header_zero_is_honored_without_sleeping() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            response_with_retry_after(StatusCode::TOO_MANY_REQUESTS, "retry-me", "0"),
            MockResponse::text(StatusCode::OK, "ok"),
        ],
    );
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let decoded = client
        .execute_plan::<concord_core::prelude::Text<String>>(retry_plan(
            "RetryAfterZero",
            Method::GET,
            "/retry/retry-after",
            retry_policy_for_statuses(2, vec![StatusCode::TOO_MANY_REQUESTS]),
            true,
        ))
        .await?;

    assert_eq!(decoded.value(), "ok");
    assert_eq!(sent.sent_count().await, 2);
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].meta.attempt, 0);
    assert_eq!(requests[1].meta.attempt, 1);
    Ok(())
}

#[tokio::test]
async fn inherited_classifier_uses_independent_cap_and_retry_after_setting()
-> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            response_with_retry_after(StatusCode::TOO_MANY_REQUESTS, "retry-me", "0"),
            MockResponse::text(StatusCode::OK, "ok"),
        ],
    );
    let sent = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    client.configure(|cfg| {
        cfg.max_attempts(2).respect_retry_after(true);
    });
    client.set_retry_policy(Arc::new(InheritedStatusRetry));

    let decoded = client
        .execute_plan::<concord_core::prelude::Text<String>>(retry_plan(
            "InheritedRetry",
            Method::GET,
            "/retry/inherited",
            ResolvedPolicy::default(),
            true,
        ))
        .await?;

    assert_eq!(decoded.value(), "ok");
    assert_eq!(sent.sent_count().await, 2);
    Ok(())
}

#[tokio::test]
async fn inherited_attempt_caps_outside_one_through_three_are_rejected() {
    for invalid_cap in [0, 4] {
        let events = Arc::new(Mutex::new(Vec::new()));
        let transport = MockTransport::new(
            events,
            vec![MockResponse::text(StatusCode::OK, "must-not-send")],
        );
        let sent = transport.clone();
        let mut client = client(TestAuthVars::default(), transport);
        client.configure(|cfg| {
            cfg.max_attempts(invalid_cap);
        });
        client.set_retry_policy(Arc::new(InheritedStatusRetry));

        let err = client
            .execute_plan::<concord_core::prelude::Text<String>>(retry_plan(
                "InvalidInheritedCap",
                Method::GET,
                "/retry/invalid-cap",
                ResolvedPolicy::default(),
                true,
            ))
            .await
            .expect_err("invalid inherited cap should be rejected before send");
        assert_eq!(err.category(), concord_core::error::ErrorCategory::Config);
        assert_eq!(sent.sent_count().await, 0);
    }
}

#[tokio::test]
async fn replayable_encoded_body_is_preserved_across_retry_attempts() -> Result<(), ApiClientError>
{
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, "retry-me"),
            MockResponse::text(StatusCode::OK, "ok"),
        ],
    );
    let sent = transport.clone();
    let mut policy = retry_policy(2);
    if let RetrySetting::Config(config) = &mut policy.retry {
        config.methods = vec![Method::PUT];
    }
    let client = client(TestAuthVars::default(), transport);

    let decoded = client
        .execute_plan::<concord_core::prelude::Text<String>>(retry_encoded_plan(
            "RetryReplayableBody",
            Method::PUT,
            "/retry/replayable-body",
            policy,
            b"{\"retry\":true}",
            true,
        ))
        .await?;

    assert_eq!(decoded.value(), "ok");
    assert_eq!(sent.sent_count().await, 2);
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].meta.attempt, 0);
    assert_eq!(requests[1].meta.attempt, 1);
    assert_eq!(requests[0].body.as_bytes(), requests[1].body.as_bytes());
    Ok(())
}

#[tokio::test]
async fn non_replayable_stream_body_stops_after_the_first_attempt() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, "retry-me"),
            MockResponse::text(StatusCode::OK, "should-not-send"),
        ],
    );
    let sent = transport.clone();
    let mut policy = retry_policy(2);
    if let RetrySetting::Config(config) = &mut policy.retry {
        config.methods = vec![Method::PUT];
    }
    let client = client(TestAuthVars::default(), transport);

    let err = client
        .execute_plan::<concord_core::prelude::Text<String>>(retry_stream_plan(
            "RetryNonReplayableBody",
            Method::PUT,
            "/retry/non-replayable-body",
            policy,
            b"retry-stream-body",
            true,
        ))
        .await
        .expect_err("non-replayable stream body should stop after the first attempt");

    assert!(matches!(err, ApiClientError::HttpStatus { .. }));
    assert_eq!(err.category(), ErrorCategory::HttpStatus);
    assert_eq!(err.context().endpoint, "RetryNonReplayableBody");
    assert_eq!(err.context().method, Method::PUT);
    assert_eq!(sent.sent_count().await, 1);
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 1);
    assert!(requests[0].body.as_bytes().is_some());
}

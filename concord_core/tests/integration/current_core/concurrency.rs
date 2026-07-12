#![allow(clippy::needless_update)] // Concurrent endpoint fixtures keep `..Default::default()` for resilience to added fields.

use super::common::buffered_endpoint_response_terminal;
use super::common::*;
use crate::support::assert_error_chain_does_not_contain_any;
use bytes::Bytes;
use concord_core::advanced::{
    AuthAppliedCredential, AuthError, AuthFuture, AuthPlacement, AuthRequirement, AuthStepPolicy,
    CredentialContext, CredentialId, CredentialProvider, CredentialRefreshReason, CredentialSlot,
    DynBody, OctetStream, PreparedAuthCredential, RateLimitBucketUse, RateLimitContext,
    RateLimitKeyPart, RateLimitPermit, RateLimitPlan, RateLimitResponseAction,
    RateLimitResponseContext, RateLimiter, RawStreamResponse, RequestMeta, ResponseEntity,
    StreamBody, Transport, TransportError, TransportErrorKind, apply_secret_credential,
};
use concord_core::error::ErrorContext;
use concord_core::internal::{PreparedBody, ResolvedPolicy};
use concord_core::prelude::{
    AccessToken, ApiClient, ApiClientError, ClientContext, CursorPagination, Endpoint,
    PaginationTermination, ReusableEndpoint,
};
use http::{HeaderMap, HeaderValue, StatusCode};
use std::collections::{HashMap, VecDeque};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
use std::time::Duration;
use tokio::sync::{Mutex, Notify, watch};

const RAW_AUTH_SENTINEL_PR80_A: &str = "RAW_AUTH_SENTINEL_PR80_A";
const RAW_AUTH_SENTINEL_PR80_B: &str = "RAW_AUTH_SENTINEL_PR80_B";
const RESPONSE_BODY_SENTINEL_PR80_A: &str = "RESPONSE_BODY_SENTINEL_PR80_A";
const RESPONSE_BODY_SENTINEL_PR80_B: &str = "RESPONSE_BODY_SENTINEL_PR80_B";

fn assert_bearer_auth_header_contains(request: &CapturedTransportRequest, sentinel: &str) {
    let header = request
        .headers
        .get(http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok());
    assert!(
        header.is_some_and(|value| value.contains(sentinel)),
        "authorization header did not contain the expected sentinel"
    );
}

fn assert_bearer_auth_header_not_contains(request: &CapturedTransportRequest, sentinel: &str) {
    let header = request
        .headers
        .get(http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    assert!(
        !header.contains(sentinel),
        "authorization header unexpectedly contained a sentinel"
    );
}

fn assert_bytes_body(request: &CapturedTransportRequest, expected: &[u8]) {
    match &request.body {
        CapturedBody::Bytes(bytes) => {
            assert!(
                bytes.as_ref() == expected,
                "request body bytes did not match the expected value"
            );
        }
        _ => panic!("expected bytes request body"),
    }
}

fn assert_stream_body(request: &CapturedTransportRequest) {
    assert!(
        request.body.as_bytes().is_some(),
        "expected a consumed standard request body"
    );
}

fn request_with_path<'a>(
    requests: &'a [CapturedTransportRequest],
    path: &str,
) -> &'a CapturedTransportRequest {
    requests
        .iter()
        .find(|request| request.url.path() == path)
        .expect("expected request with the requested path")
}

fn query_value(url: &url::Url, name: &str) -> Option<String> {
    url.query_pairs()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
}

#[tokio::test]
async fn identical_concurrent_get_requests_are_not_coalesced() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = GateTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "first"),
            MockResponse::text(StatusCode::OK, "second"),
        ],
    );
    let sent = transport.clone();
    let mut client = ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport);
    client.configure(|cfg| {
        cfg.retry_admission(concord_core::advanced::RetryAdmissionRegistry::new(
            4096,
            std::time::Duration::from_secs(15 * 60),
        ));
    });
    let client = Arc::new(client);

    let a = spawn_text_request(client.clone(), TextEndpoint::default());
    let b = spawn_text_request(client, TextEndpoint::default());

    sent.wait_for_sends(2).await;
    assert_eq!(sent.sent_count().await, 2);
    sent.release_all();

    let mut values = vec![
        a.await.expect("request task panicked")?,
        b.await.expect("request task panicked")?,
    ];
    values.sort();
    assert_eq!(values, vec!["first".to_string(), "second".to_string()]);
    assert_eq!(sent.sent_count().await, 2);
    Ok(())
}

#[tokio::test]
async fn identical_concurrent_post_requests_are_not_coalesced() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = GateTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "first"),
            MockResponse::text(StatusCode::OK, "second"),
        ],
    );
    let sent = transport.clone();
    let client = ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport);
    let client = Arc::new(client);
    let endpoint = TextEndpoint {
        method: http::Method::POST,
        ..Default::default()
    };

    let a = spawn_text_request(client.clone(), endpoint.clone());
    let b = spawn_text_request(client, endpoint);

    sent.wait_for_sends(2).await;
    assert_eq!(sent.sent_count().await, 2);
    sent.release_all();

    let mut values = vec![
        a.await.expect("request task panicked")?,
        b.await.expect("request task panicked")?,
    ];
    values.sort();
    assert_eq!(values, vec!["first".to_string(), "second".to_string()]);
    assert_eq!(sent.sent_count().await, 2);
    Ok(())
}

#[tokio::test]
async fn rate_limit_still_observes_each_non_coalesced_request() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let limiter = Arc::new(RecordingRateLimiter::new(events.clone()));
    let transport = GateTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::OK, "first"),
            MockResponse::text(StatusCode::OK, "second"),
        ],
    );
    let sent = transport.clone();
    let mut client = ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport);
    configure_runtime(&mut client, Some(limiter));
    let client = Arc::new(client);

    let a = spawn_text_request(client.clone(), TextEndpoint::default());
    let b = spawn_text_request(client, TextEndpoint::default());

    sent.wait_for_sends(2).await;
    assert_eq!(sent.sent_count().await, 2);
    sent.release_all();

    a.await.expect("request task panicked")?;
    b.await.expect("request task panicked")?;

    let events = events.lock().await.clone();
    assert_eq!(
        events
            .iter()
            .filter(|event| event.as_str() == "rate_acquire")
            .count(),
        2
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.as_str() == "rate_response")
            .count(),
        2
    );
    Ok(())
}

#[tokio::test]
async fn retry_still_applies_per_non_coalesced_request() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = GateTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, "retry-a"),
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, "retry-b"),
            MockResponse::text(StatusCode::OK, "first"),
            MockResponse::text(StatusCode::OK, "second"),
        ],
    );
    let sent = transport.clone();
    let mut client = ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport);
    client.configure(|cfg| {
        cfg.retry_admission(concord_core::advanced::RetryAdmissionRegistry::new(
            4096,
            std::time::Duration::from_secs(15 * 60),
        ));
    });
    let client = Arc::new(client);
    let endpoint = TextEndpoint {
        policy: retry_policy(2),
        ..Default::default()
    };

    let a = spawn_text_request(client.clone(), endpoint.clone());
    let b = spawn_text_request(client, endpoint);

    sent.wait_for_sends(2).await;
    assert_eq!(sent.sent_count().await, 2);
    sent.release_all();

    let results = [
        a.await.expect("request task panicked"),
        b.await.expect("request task panicked"),
    ];
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(results.iter().filter(|result| result.is_err()).count(), 1);
    assert_eq!(sent.sent_count().await, 3);
    Ok(())
}

#[tokio::test]
async fn concurrent_retry_attempt_indexes_remain_request_local() -> Result<(), ApiClientError> {
    let retry_events = Arc::new(Mutex::new(Vec::new()));
    let sibling_events = Arc::new(Mutex::new(Vec::new()));
    let retry_transport = GateTransport::new(
        retry_events,
        vec![
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, "retry-a"),
            MockResponse::text(StatusCode::OK, "retry-b"),
        ],
    );
    let sibling_transport = GateTransport::new(
        sibling_events,
        vec![MockResponse::text(StatusCode::OK, "sibling")],
    );
    let retry_sent = retry_transport.clone();
    let sibling_sent = sibling_transport.clone();
    let retry_client = Arc::new(ApiClient::<TestCx, _>::with_transport(
        (),
        TestAuthVars::default(),
        retry_transport,
    ));
    let sibling_client = Arc::new(ApiClient::<TestCx, _>::with_transport(
        (),
        TestAuthVars::default(),
        sibling_transport,
    ));

    let retry = TextEndpoint {
        name: "Retry",
        path: "/retry",
        policy: retry_policy(2),
        ..Default::default()
    };
    let sibling = TextEndpoint {
        name: "Sibling",
        path: "/sibling",
        ..Default::default()
    };

    let retry_task = tokio::spawn({
        let retry = retry.clone();
        async move {
            retry_client
                .request(retry)
                .response()
                .await
                .map(|response| response.into_value())
        }
    });
    let sibling_task = tokio::spawn({
        async move {
            sibling_client
                .request(sibling)
                .response()
                .await
                .map(|response| response.into_value())
        }
    });

    retry_sent.wait_for_sends(1).await;
    sibling_sent.wait_for_sends(1).await;
    retry_sent.release_all();
    sibling_sent.release_all();

    let retry_value = retry_task
        .await
        .expect("retry task should join")
        .expect("retry request should succeed");
    let sibling_value = sibling_task
        .await
        .expect("sibling task should join")
        .expect("sibling request should succeed");
    assert_eq!(retry_value, "retry-b");
    assert_eq!(sibling_value, "sibling");

    let mut retry_requests = retry_sent.requests().await;
    assert_eq!(retry_requests.len(), 2);
    retry_requests.sort_by_key(|request| request.meta.attempt);
    assert_eq!(retry_requests[0].meta.endpoint, "Retry");
    assert_eq!(retry_requests[0].meta.method, http::Method::GET);
    assert_eq!(retry_requests[0].meta.attempt, 0);
    assert_eq!(retry_requests[0].meta.page_index, 0);
    assert_eq!(retry_requests[1].meta.endpoint, "Retry");
    assert_eq!(retry_requests[1].meta.method, http::Method::GET);
    assert_eq!(retry_requests[1].meta.attempt, 1);
    assert_eq!(retry_requests[1].meta.page_index, 0);

    let sibling_requests = sibling_sent.requests().await;
    assert_eq!(sibling_requests.len(), 1);
    assert_eq!(sibling_requests[0].meta.endpoint, "Sibling");
    assert_eq!(sibling_requests[0].meta.method, http::Method::GET);
    assert_eq!(sibling_requests[0].meta.attempt, 0);
    assert_eq!(sibling_requests[0].meta.page_index, 0);
    Ok(())
}

#[tokio::test]
async fn concurrent_missing_credential_acquisition_single_flights() -> Result<(), ApiClientError> {
    const N: usize = 4;

    let events = Arc::new(Mutex::new(Vec::new()));
    let provider = ControlledTokenProvider::new("shared-token");
    let transport = GateTransport::new(
        events,
        (0..N)
            .map(|index| MockResponse::text(StatusCode::OK, format!("ok-{index}")))
            .collect(),
    );
    let sent = transport.clone();
    let client = Arc::new(ApiClient::<SingleFlightCx, _>::with_transport(
        (),
        SingleFlightAuthVars {
            provider: provider.clone(),
        },
        transport,
    ));
    let endpoint = TextEndpoint {
        policy: auth_policy(AuthPlacement::Bearer),
        ..Default::default()
    };

    let tasks = (0..N)
        .map(|_| spawn_single_flight_request(client.clone(), endpoint.clone()))
        .collect::<Vec<_>>();

    provider.wait_for_acquires(1).await;
    assert_eq!(provider.acquire_count().await, 1);
    assert_eq!(sent.sent_count().await, 0);

    provider.release_all();
    sent.wait_for_sends(N).await;
    assert_eq!(provider.acquire_count().await, 1);
    assert_eq!(sent.sent_count().await, N);

    let requests = sent.requests().await;
    assert_eq!(requests.len(), N);
    for request in &requests {
        assert_eq!(
            request.headers.get(http::header::AUTHORIZATION),
            Some(&HeaderValue::from_static("Bearer shared-token"))
        );
    }

    sent.release_all();

    let mut values = Vec::new();
    for task in tasks {
        values.push(task.await.expect("request task panicked")?);
    }
    values.sort();
    assert_eq!(
        values,
        (0..N)
            .map(|index| format!("ok-{index}"))
            .collect::<Vec<_>>()
    );
    Ok(())
}

#[tokio::test]
async fn concurrent_pending_overrides_are_request_local() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = GateTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::OK, "a").with_content_length(Some(1)),
            MockResponse::text(StatusCode::OK, "b").with_content_length(Some(1)),
        ],
    );
    let debug = Arc::new(SafeRecordingDebugSink::new(events.clone()));
    let mut client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
    client.configure(|cfg| {
        cfg.debug_level(concord_core::prelude::DebugLevel::None);
        cfg.debug_sink(debug.clone());
    });

    let a = TextEndpoint {
        name: "A",
        path: "/a",
        ..Default::default()
    };
    let b = TextEndpoint {
        name: "B",
        path: "/b",
        ..Default::default()
    };

    transport.wait_for_sends(0).await;
    let task_a = tokio::spawn({
        let client = client.clone();
        let a = a.clone();
        async move {
            client
                .request(a)
                .timeout(Duration::from_secs(2))
                .debug_level(concord_core::prelude::DebugLevel::VV)
                .response()
                .await
        }
    });
    let task_b = tokio::spawn({
        let client = client.clone();
        async move { client.request(b).response().await }
    });

    transport.wait_for_sends(2).await;
    let requests = transport.requests().await;
    assert_eq!(requests.len(), 2);
    let req_a = requests
        .iter()
        .find(|request| request.url.path() == "/a")
        .expect("request A should be recorded");
    let req_b = requests
        .iter()
        .find(|request| request.url.path() == "/b")
        .expect("request B should be recorded");
    assert_eq!(req_a.timeout, Some(Duration::from_secs(2)));
    assert_eq!(req_b.timeout, None);

    transport.release_all();
    let a = task_a
        .await
        .expect("task A should join")
        .expect("task A should succeed");
    let b = task_b
        .await
        .expect("task B should join")
        .expect("task B should succeed");
    assert_eq!(a.value(), "a");
    assert_eq!(b.value(), "b");

    let debug_events = debug.events.lock().await.clone();
    assert!(debug_events.iter().any(|event| event.contains("/a")));
    assert!(!debug_events.iter().any(|event| event.contains("/b")));
    Ok(())
}

#[tokio::test]
async fn concurrent_clone_reconfigure_does_not_affect_in_flight_request() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = GateTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::OK, Bytes::from_static(b"abcde"))
                .with_content_length(Some(5)),
            MockResponse::text(StatusCode::OK, Bytes::from_static(b"abcde"))
                .with_content_length(Some(5)),
        ],
    );
    let mut base_client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
    base_client.configure(|cfg| {
        cfg.max_response_body_bytes(4);
    });
    let request_client = base_client.clone();
    let mut reconfigured_client = base_client.clone();

    let in_flight = tokio::spawn(async move {
        request_client
            .request(TextEndpoint::default())
            .response()
            .await
    });

    transport.wait_for_sends(1).await;
    reconfigured_client.configure(|cfg| {
        cfg.no_response_body_limit();
    });
    transport.release_all();

    let err = in_flight
        .await
        .expect("request task should complete")
        .expect_err("in-flight request should keep the original body limit");
    assert!(matches!(
        err,
        ApiClientError::ResponseBodyLimitExceeded { .. }
    ));

    let later = reconfigured_client
        .request(TextEndpoint::default())
        .response()
        .await
        .expect("later request should use the updated no-limit config");
    assert_eq!(later.value(), "abcde");
}

#[tokio::test]
async fn concurrent_success_and_decode_failure_are_isolated() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let gate = PhaseGate::new();
    let limiter = Arc::new(RecordingRateLimiter::new(events.clone()));
    let transport = RoutedTransport::new(gate.clone(), events.clone());
    transport
        .insert(
            "/ok",
            MockOutcome::Response(MockResponse::text(StatusCode::OK, "one")),
        )
        .await;
    transport
        .insert(
            "/bad",
            MockOutcome::Response(MockResponse::text(StatusCode::OK, vec![0xff])),
        )
        .await;
    let mut client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
    client.configure(|cfg| {
        cfg.rate_limiter(limiter.clone());
    });
    let ok = TextEndpoint {
        name: "Ok",
        path: "/ok",
        policy: rate_limit_policy(),
        ..Default::default()
    };
    let bad = TextEndpoint {
        name: "Bad",
        path: "/bad",
        policy: rate_limit_policy(),
        ..Default::default()
    };

    gate.block("transport_send").await;
    let ok_task = tokio::spawn({
        let client = client.clone();
        let ok = ok.clone();
        async move { client.request(ok).response().await }
    });
    let bad_task = tokio::spawn({
        let client = client.clone();
        async move { client.request(bad).response().await }
    });

    gate.wait_for("transport_send", 2).await;
    gate.release_all("transport_send").await;
    let ok_value = ok_task
        .await
        .expect("ok task should join")
        .expect("ok request should succeed");
    assert_eq!(ok_value.value(), "one");
    let bad_err = bad_task
        .await
        .expect("bad task should join")
        .expect_err("bad request should fail to decode");
    assert!(matches!(bad_err, ApiClientError::Decode { .. }));
    assert_eq!(transport.sent_count().await, 2);
    Ok(())
}

#[tokio::test]
async fn concurrent_rate_limit_keys_are_isolated() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let gate = PhaseGate::new();
    let limiter_policy_a = rate_limit_policy_with_bucket("bucket-a", "route", "a");
    let limiter_policy_b = rate_limit_policy_with_bucket("bucket-b", "route", "b");
    let blocked_label = rate_limit_plan_label(&limiter_policy_a.rate_limit);
    let limiter = Arc::new(KeyBlockingRateLimiter::new(
        events.clone(),
        gate.clone(),
        blocked_label,
        "rate_acquire_a",
    ));
    let transport = RoutedTransport::new(gate.clone(), events.clone());
    transport
        .insert(
            "/a",
            MockOutcome::Response(MockResponse::text(StatusCode::OK, "a")),
        )
        .await;
    transport
        .insert(
            "/b",
            MockOutcome::Response(MockResponse::text(StatusCode::OK, "b")),
        )
        .await;
    let mut client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
    client.configure(|cfg| {
        cfg.rate_limiter(limiter.clone());
    });
    let a = TextEndpoint {
        name: "A",
        path: "/a",
        policy: limiter_policy_a,
        ..Default::default()
    };
    let b = TextEndpoint {
        name: "B",
        path: "/b",
        policy: limiter_policy_b,
        ..Default::default()
    };

    gate.block("rate_acquire_a").await;
    let task_a = tokio::spawn({
        let client = client.clone();
        let a = a.clone();
        async move {
            client
                .request(a)
                .response()
                .await
                .map(|response| response.into_value())
        }
    });
    let task_b = tokio::spawn({
        let client = client.clone();
        async move {
            client
                .request(b)
                .response()
                .await
                .map(|response| response.into_value())
        }
    });

    gate.wait_for("rate_acquire_a", 1).await;
    assert!(!task_a.is_finished());
    let b_value = task_b
        .await
        .expect("task B should join")
        .expect("task B should complete independently");
    assert_eq!(b_value, "b");
    assert_eq!(limiter.acquire_started.load(AtomicOrdering::SeqCst), 2);
    let labels = limiter.events.lock().await.clone();
    assert!(
        labels
            .iter()
            .any(|event| event.contains("route=Static(\"a\")"))
    );
    assert!(
        labels
            .iter()
            .any(|event| event.contains("route=Static(\"b\")"))
    );
    assert!(!task_a.is_finished());

    gate.release_all("rate_acquire_a").await;
    let a_value = task_a
        .await
        .expect("task A should join")
        .expect("task A should complete after release");
    assert_eq!(a_value, "a");
    assert_eq!(transport.sent_count().await, 2);
    assert_eq!(limiter.response_observed.load(AtomicOrdering::SeqCst), 2);
    assert_eq!(limiter.acquire_started.load(AtomicOrdering::SeqCst), 2);
    Ok(())
}

#[tokio::test]
async fn concurrent_rate_limit_acquisitions_remain_request_local() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let gate = PhaseGate::new();
    let limiter_policy_a = rate_limit_policy_with_bucket("bucket-a", "route", "a");
    let limiter_policy_b = rate_limit_policy_with_bucket("bucket-b", "route", "b");
    let blocked_label = rate_limit_plan_label(&limiter_policy_a.rate_limit);
    let limiter = Arc::new(KeyBlockingRateLimiter::new(
        events.clone(),
        gate.clone(),
        blocked_label,
        "rate_acquire_a",
    ));
    let transport = RoutedTransport::new(gate.clone(), events.clone());
    transport
        .insert(
            "/a",
            MockOutcome::Response(MockResponse::text(StatusCode::OK, "a")),
        )
        .await;
    transport
        .insert(
            "/b",
            MockOutcome::Response(MockResponse::text(StatusCode::OK, "b")),
        )
        .await;
    let mut client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
    client.configure(|cfg| {
        cfg.rate_limiter(limiter.clone());
    });
    let a = TextEndpoint {
        name: "RateA",
        path: "/a",
        policy: limiter_policy_a,
        ..Default::default()
    };
    let b = TextEndpoint {
        name: "RateB",
        path: "/b",
        policy: limiter_policy_b,
        ..Default::default()
    };

    gate.block("rate_acquire_a").await;
    let task_a = tokio::spawn({
        let client = client.clone();
        let a = a.clone();
        async move {
            client
                .request(a)
                .response()
                .await
                .map(|response| response.into_value())
        }
    });
    let task_b = tokio::spawn({
        let client = client.clone();
        async move {
            client
                .request(b)
                .response()
                .await
                .map(|response| response.into_value())
        }
    });

    gate.wait_for("rate_acquire_a", 1).await;
    assert!(!task_a.is_finished());
    let b_value = task_b
        .await
        .expect("task B should join")
        .expect("task B should complete independently");
    assert_eq!(b_value, "b");
    assert_eq!(limiter.acquire_started.load(AtomicOrdering::SeqCst), 2);
    let events = limiter.events.lock().await.clone();
    assert!(
        events
            .iter()
            .any(|event| event.contains("rate_acquire:RateA:GET:https://example.com/a:"))
    );
    assert!(
        events
            .iter()
            .any(|event| event.contains("rate_acquire:RateB:GET:https://example.com/b:"))
    );
    assert!(
        events
            .iter()
            .any(|event| event.contains("route=Static(\"a\")"))
    );
    assert!(
        events
            .iter()
            .any(|event| event.contains("route=Static(\"b\")"))
    );
    assert!(!task_a.is_finished());

    gate.release_all("rate_acquire_a").await;
    let a_value = task_a
        .await
        .expect("task A should join")
        .expect("task A should complete after release");
    assert_eq!(a_value, "a");
    assert_eq!(transport.sent_count().await, 2);
    assert_eq!(limiter.response_observed.load(AtomicOrdering::SeqCst), 2);
    assert_eq!(limiter.acquire_started.load(AtomicOrdering::SeqCst), 2);

    let requests = transport.requests().await;
    assert_eq!(requests.len(), 2);
    let a_request = request_with_path(&requests, "/a");
    assert_eq!(a_request.meta.endpoint, "RateA");
    assert_eq!(a_request.meta.method, http::Method::GET);
    assert_eq!(a_request.meta.attempt, 0);
    assert_eq!(a_request.meta.page_index, 0);
    let b_request = request_with_path(&requests, "/b");
    assert_eq!(b_request.meta.endpoint, "RateB");
    assert_eq!(b_request.meta.method, http::Method::GET);
    assert_eq!(b_request.meta.attempt, 0);
    assert_eq!(b_request.meta.page_index, 0);
    Ok(())
}

#[tokio::test]
async fn concurrent_transport_error_and_success_are_isolated() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let gate = PhaseGate::new();
    let limiter = Arc::new(RecordingRateLimiter::new(events.clone()));
    let transport = RoutedTransport::new(gate.clone(), events.clone());
    transport
        .insert(
            "/ok",
            MockOutcome::Response(MockResponse::text(StatusCode::OK, "one")),
        )
        .await;
    transport
        .insert(
            "/err",
            MockOutcome::TransportError(TransportErrorKind::Timeout),
        )
        .await;
    let mut client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
    client.configure(|cfg| {
        cfg.rate_limiter(limiter.clone());
    });
    let ok = TextEndpoint {
        name: "Ok",
        path: "/ok",
        policy: rate_limit_policy(),
        ..Default::default()
    };
    let err = TextEndpoint {
        name: "Err",
        path: "/err",
        policy: rate_limit_policy(),
        ..Default::default()
    };

    gate.block("transport_send").await;
    let ok_task = tokio::spawn({
        let client = client.clone();
        let ok = ok.clone();
        async move { client.request(ok).response().await }
    });
    let err_task = tokio::spawn({
        let client = client.clone();
        async move { client.request(err).response().await }
    });
    gate.wait_for("transport_send", 2).await;
    gate.release_all("transport_send").await;

    let ok_value = ok_task
        .await
        .expect("ok task should join")
        .expect("ok request should succeed");
    assert_eq!(ok_value.value(), "one");
    let err_value = err_task
        .await
        .expect("err task should join")
        .expect_err("err request should fail");
    assert!(matches!(err_value, ApiClientError::Transport { .. }));
    assert_eq!(
        limiter
            .events
            .lock()
            .await
            .iter()
            .filter(|event| *event == "rate_response")
            .count(),
        1
    );
    Ok(())
}

#[tokio::test]
async fn concurrent_pagination_runs_keep_independent_state() -> Result<(), ApiClientError> {
    let events_a = Arc::new(Mutex::new(Vec::new()));
    let events_b = Arc::new(Mutex::new(Vec::new()));
    let transport_a = GateTransport::new(
        events_a.clone(),
        vec![
            MockResponse::text(StatusCode::OK, "a1,a2"),
            MockResponse::text(StatusCode::OK, "a3"),
        ],
    );
    let sent_a = transport_a.clone();
    let transport_b = GateTransport::new(
        events_b.clone(),
        vec![
            MockResponse::text(StatusCode::OK, "b1,b2|next=next-b"),
            MockResponse::text(StatusCode::OK, "b3|"),
        ],
    );
    let sent_b = transport_b.clone();
    let client_a = Arc::new(ApiClient::<TestCx, _>::with_transport(
        (),
        TestAuthVars::default(),
        transport_a,
    ));
    let client_b = Arc::new(ApiClient::<TestCx, _>::with_transport(
        (),
        TestAuthVars::default(),
        transport_b,
    ));

    let page_task = tokio::spawn({
        let client = client_a.clone();
        async move {
            client
                .request(PageOnlyItemsEndpoint {
                    policy: Default::default(),
                    page: 1,
                    count: 2,
                    pagination: PaginationVariant::Paged {
                        page: 1,
                        per_page: 2,
                    },
                    ..Default::default()
                })
                .paginate(PaginationTermination::hard_page_cap(10))
                .collect()
                .await
        }
    });
    let cursor_task = tokio::spawn({
        let client = client_b.clone();
        async move {
            client
                .request(CursorItemsEndpoint {
                    policy: Default::default(),
                    pagination: PaginationVariant::cursor::<CursorItems>(CursorPagination {
                        cursor: Some("start".to_string()),
                        per_page: 2,
                        send_cursor_on_first: true,
                        stop_when_cursor_missing: true,
                    }),
                    ..Default::default()
                })
                .paginate(PaginationTermination::hard_page_cap(10))
                .collect()
                .await
        }
    });

    sent_a.wait_for_sends(1).await;
    sent_b.wait_for_sends(1).await;
    sent_a.release_all();
    sent_b.release_all();

    let page_items = wait_bounded("page pagination task", page_task)
        .await
        .expect("page pagination task should join")
        .expect("page pagination should succeed");
    let cursor_items = wait_bounded("cursor pagination task", cursor_task)
        .await
        .expect("cursor pagination task should join")
        .expect("cursor pagination should succeed");

    assert_eq!(
        page_items,
        vec!["a1".to_string(), "a2".to_string(), "a3".to_string()]
    );
    assert_eq!(
        cursor_items,
        vec!["b1".to_string(), "b2".to_string(), "b3".to_string()]
    );

    let page_requests = sent_a.requests().await;
    assert_eq!(page_requests.len(), 2);
    assert_eq!(page_requests[0].meta.page_index, 0);
    assert_eq!(page_requests[1].meta.page_index, 1);

    let cursor_requests = sent_b.requests().await;
    assert_eq!(cursor_requests.len(), 2);
    assert_eq!(cursor_requests[0].meta.page_index, 0);
    assert_eq!(cursor_requests[1].meta.page_index, 1);

    let page_requests: Vec<_> = page_requests
        .iter()
        .filter(|request| request.url.path() == "/page-only-items")
        .collect();
    assert_eq!(page_requests.len(), 2);
    assert_eq!(
        query_value(&page_requests[0].url, "page"),
        Some("1".to_string())
    );
    assert_eq!(
        query_value(&page_requests[0].url, "per_page"),
        Some("2".to_string())
    );
    assert_eq!(
        query_value(&page_requests[1].url, "page"),
        Some("2".to_string())
    );
    assert_eq!(
        query_value(&page_requests[1].url, "per_page"),
        Some("2".to_string())
    );

    let cursor_requests: Vec<_> = cursor_requests
        .iter()
        .filter(|request| request.url.path() == "/cursor-items")
        .collect();
    assert_eq!(cursor_requests.len(), 2);
    assert_eq!(
        query_value(&cursor_requests[0].url, "cursor"),
        Some("start".to_string())
    );
    assert_eq!(
        query_value(&cursor_requests[0].url, "per_page"),
        Some("2".to_string())
    );
    assert_eq!(
        query_value(&cursor_requests[1].url, "cursor"),
        Some("next-b".to_string())
    );
    assert_eq!(
        query_value(&cursor_requests[1].url, "per_page"),
        Some("2".to_string())
    );
    Ok(())
}

#[tokio::test]
async fn concurrent_observer_surfaces_are_body_auth_free() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let debug_a_events = Arc::new(Mutex::new(Vec::new()));
    let debug_b_events = Arc::new(Mutex::new(Vec::new()));
    let transport = GateTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::OK, RESPONSE_BODY_SENTINEL_PR80_A)
                .with_content_length(Some(RESPONSE_BODY_SENTINEL_PR80_A.len() as u64)),
            MockResponse::text(StatusCode::OK, RESPONSE_BODY_SENTINEL_PR80_B)
                .with_content_length(Some(RESPONSE_BODY_SENTINEL_PR80_B.len() as u64)),
        ],
    );
    let limiter = Arc::new(ObservationRateLimiter::new(events.clone()));
    let hooks = Arc::new(ObservationRuntimeHooks::new(events.clone()));

    let mut policy = auth_policy(AuthPlacement::Bearer);
    policy.rate_limit = rate_limit_policy().rate_limit;

    let mut client_a = ApiClient::<ObservationAuthCx, _>::with_transport(
        (),
        ObservationAuthVars::bearer(RAW_AUTH_SENTINEL_PR80_A, "observer-a", events.clone()),
        transport.clone(),
    );
    client_a.configure(|cfg| {
        cfg.rate_limiter(limiter.clone());
        cfg.debug_level(concord_core::prelude::DebugLevel::VV);
    });
    client_a.set_runtime_hooks(hooks.clone());
    client_a.set_debug_sink(Arc::new(SafeRecordingDebugSink::new(
        debug_a_events.clone(),
    )));

    let mut client_b = ApiClient::<ObservationAuthCx, _>::with_transport(
        (),
        ObservationAuthVars::bearer(RAW_AUTH_SENTINEL_PR80_B, "observer-b", events.clone()),
        transport.clone(),
    );
    client_b.configure(|cfg| {
        cfg.rate_limiter(limiter.clone());
        cfg.debug_level(concord_core::prelude::DebugLevel::VV);
    });
    client_b.set_runtime_hooks(hooks.clone());
    client_b.set_debug_sink(Arc::new(SafeRecordingDebugSink::new(
        debug_b_events.clone(),
    )));

    let endpoint = TextEndpoint {
        policy,
        ..Default::default()
    };

    let a = tokio::spawn({
        let client = Arc::new(client_a);
        let endpoint = endpoint.clone();
        async move { client.request(endpoint).response().await }
    });
    let b = tokio::spawn({
        let client = Arc::new(client_b);
        let endpoint = endpoint.clone();
        async move { client.request(endpoint).response().await }
    });

    transport.wait_for_sends(2).await;
    transport.release_all();

    a.await
        .expect("observer task A should join")
        .expect("observer request A should succeed");
    b.await
        .expect("observer task B should join")
        .expect("observer request B should succeed");

    let shared_events = events.lock().await.clone();
    assert!(
        !shared_events
            .iter()
            .any(|event| event.contains(RAW_AUTH_SENTINEL_PR80_A))
            && !shared_events
                .iter()
                .any(|event| event.contains(RAW_AUTH_SENTINEL_PR80_B))
    );
    assert!(
        !shared_events
            .iter()
            .any(|event| event.contains(RESPONSE_BODY_SENTINEL_PR80_A))
            && !shared_events
                .iter()
                .any(|event| event.contains(RESPONSE_BODY_SENTINEL_PR80_B))
    );

    let debug_a_events = debug_a_events.lock().await.clone();
    let debug_b_events = debug_b_events.lock().await.clone();
    assert!(
        !debug_a_events
            .iter()
            .any(|event| event.contains(RAW_AUTH_SENTINEL_PR80_A))
            && !debug_a_events
                .iter()
                .any(|event| event.contains(RAW_AUTH_SENTINEL_PR80_B))
            && !debug_a_events
                .iter()
                .any(|event| event.contains(RESPONSE_BODY_SENTINEL_PR80_A))
            && !debug_a_events
                .iter()
                .any(|event| event.contains(RESPONSE_BODY_SENTINEL_PR80_B))
    );
    assert!(
        !debug_b_events
            .iter()
            .any(|event| event.contains(RAW_AUTH_SENTINEL_PR80_A))
            && !debug_b_events
                .iter()
                .any(|event| event.contains(RAW_AUTH_SENTINEL_PR80_B))
            && !debug_b_events
                .iter()
                .any(|event| event.contains(RESPONSE_BODY_SENTINEL_PR80_A))
            && !debug_b_events
                .iter()
                .any(|event| event.contains(RESPONSE_BODY_SENTINEL_PR80_B))
    );
    Ok(())
}

#[tokio::test]
async fn concurrent_auth_material_does_not_cross_contaminate() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = GateTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "left"),
            MockResponse::text(StatusCode::OK, "right"),
        ],
    );
    let sent = transport.clone();
    let client_a = Arc::new(ApiClient::<TestCx, _>::with_transport(
        (),
        TestAuthVars {
            token: Some(RAW_AUTH_SENTINEL_PR80_A.to_string()),
            identity: "left",
        },
        transport.clone(),
    ));
    let client_b = Arc::new(ApiClient::<TestCx, _>::with_transport(
        (),
        TestAuthVars {
            token: Some(RAW_AUTH_SENTINEL_PR80_B.to_string()),
            identity: "right",
        },
        transport,
    ));
    let left = TextEndpoint {
        name: "LeftAuth",
        path: "/left-auth",
        policy: auth_policy(AuthPlacement::Bearer),
        ..Default::default()
    };
    let right = TextEndpoint {
        name: "RightAuth",
        path: "/right-auth",
        policy: auth_policy(AuthPlacement::Bearer),
        ..Default::default()
    };

    let left_task = tokio::spawn({
        let client = client_a.clone();
        let left = left.clone();
        async move { client.request(left).response().await }
    });
    let right_task = tokio::spawn({
        let client = client_b.clone();
        let right = right.clone();
        async move { client.request(right).response().await }
    });

    sent.wait_for_sends(2).await;
    sent.release_all();

    let left_value = left_task
        .await
        .expect("left task should join")
        .expect("left request should succeed");
    let right_value = right_task
        .await
        .expect("right task should join")
        .expect("right request should succeed");
    let mut values = vec![left_value.value().clone(), right_value.value().clone()];
    values.sort();
    assert_eq!(values, vec!["left".to_string(), "right".to_string()]);

    let requests = sent.requests().await;
    assert_eq!(requests.len(), 2);
    let left_request = request_with_path(&requests, "/left-auth");
    assert_eq!(left_request.meta.endpoint, "LeftAuth");
    assert_eq!(left_request.meta.method, http::Method::GET);
    assert_bearer_auth_header_contains(left_request, RAW_AUTH_SENTINEL_PR80_A);
    assert_bearer_auth_header_not_contains(left_request, RAW_AUTH_SENTINEL_PR80_B);

    let right_request = request_with_path(&requests, "/right-auth");
    assert_eq!(right_request.meta.endpoint, "RightAuth");
    assert_eq!(right_request.meta.method, http::Method::GET);
    assert_bearer_auth_header_contains(right_request, RAW_AUTH_SENTINEL_PR80_B);
    assert_bearer_auth_header_not_contains(right_request, RAW_AUTH_SENTINEL_PR80_A);

    Ok(())
}

#[tokio::test]
async fn concurrent_request_bodies_remain_isolated() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "bytes-ok"),
            MockResponse::text(StatusCode::OK, "stream-ok"),
        ],
    );
    let sent = transport.clone();
    let client = Arc::new(ApiClient::<TestCx, _>::with_transport(
        (),
        TestAuthVars::default(),
        transport,
    ));

    let bytes_endpoint = BodyEndpoint {
        name: "BytesBody",
        path: "/bytes-body",
        body: BodyKind::Bytes(Bytes::from_static(b"bytes-sentinel")),
    };
    let stream_endpoint = BodyEndpoint {
        name: "StreamBody",
        path: "/stream-body",
        body: BodyKind::Stream(Bytes::from_static(b"stream-sentinel")),
    };

    let bytes_task = tokio::spawn({
        let client = client.clone();
        let endpoint = bytes_endpoint.clone();
        async move { client.request(endpoint).response().await }
    });
    let stream_task = tokio::spawn({
        let client = client.clone();
        let endpoint = stream_endpoint.clone();
        async move { client.request(endpoint).response().await }
    });

    let bytes_value = bytes_task
        .await
        .expect("bytes task should join")
        .expect("bytes request should succeed");
    let stream_value = stream_task
        .await
        .expect("stream task should join")
        .expect("stream request should succeed");
    let mut values = vec![bytes_value.value().clone(), stream_value.value().clone()];
    values.sort();
    assert_eq!(
        values,
        vec!["bytes-ok".to_string(), "stream-ok".to_string()]
    );

    let requests = sent.requests().await;
    assert_eq!(requests.len(), 2);
    let bytes_request = request_with_path(&requests, "/bytes-body");
    assert_eq!(bytes_request.meta.endpoint, "BytesBody");
    assert_eq!(bytes_request.meta.method, http::Method::POST);
    assert_bytes_body(bytes_request, b"bytes-sentinel");

    let stream_request = request_with_path(&requests, "/stream-body");
    assert_eq!(stream_request.meta.endpoint, "StreamBody");
    assert_eq!(stream_request.meta.method, http::Method::POST);
    assert_stream_body(stream_request);

    Ok(())
}

#[tokio::test]
async fn concurrent_cancellation_does_not_affect_sibling_request() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = GateTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "ok"),
            MockResponse::text(StatusCode::OK, "ok"),
        ],
    );
    let sent = transport.clone();
    let client_a = Arc::new(ApiClient::<TestCx, _>::with_transport(
        (),
        TestAuthVars {
            token: Some(RAW_AUTH_SENTINEL_PR80_A.to_string()),
            identity: "cancelled",
        },
        transport.clone(),
    ));
    let client_b = Arc::new(ApiClient::<TestCx, _>::with_transport(
        (),
        TestAuthVars {
            token: Some(RAW_AUTH_SENTINEL_PR80_B.to_string()),
            identity: "sibling",
        },
        transport,
    ));
    let cancelled = TextEndpoint {
        name: "Cancelled",
        path: "/cancelled",
        policy: auth_policy(AuthPlacement::Bearer),
        ..Default::default()
    };
    let sibling = TextEndpoint {
        name: "Sibling",
        path: "/sibling",
        policy: auth_policy(AuthPlacement::Bearer),
        ..Default::default()
    };

    let cancelled_task = tokio::spawn({
        let client = client_a.clone();
        let cancelled = cancelled.clone();
        async move { client.request(cancelled).response().await }
    });
    let sibling_task = tokio::spawn({
        let client = client_b.clone();
        let sibling = sibling.clone();
        async move { client.request(sibling).response().await }
    });

    sent.wait_for_sends(2).await;
    cancelled_task.abort();
    sent.release_all();

    let cancelled_join = cancelled_task
        .await
        .expect_err("cancelled task should be aborted");
    assert!(cancelled_join.is_cancelled());
    assert_error_chain_does_not_contain_any(
        &cancelled_join,
        &[RAW_AUTH_SENTINEL_PR80_A, RAW_AUTH_SENTINEL_PR80_B],
    );

    let sibling_value = sibling_task
        .await
        .expect("sibling task should join")
        .expect("sibling request should succeed");
    assert_eq!(sibling_value.value(), "ok");

    let requests = sent.requests().await;
    assert_eq!(requests.len(), 2);
    let cancelled_request = request_with_path(&requests, "/cancelled");
    assert_eq!(cancelled_request.meta.endpoint, "Cancelled");
    assert_eq!(cancelled_request.meta.method, http::Method::GET);
    assert_bearer_auth_header_contains(cancelled_request, RAW_AUTH_SENTINEL_PR80_A);
    assert_bearer_auth_header_not_contains(cancelled_request, RAW_AUTH_SENTINEL_PR80_B);

    let sibling_request = request_with_path(&requests, "/sibling");
    assert_eq!(sibling_request.meta.endpoint, "Sibling");
    assert_eq!(sibling_request.meta.method, http::Method::GET);
    assert_bearer_auth_header_contains(sibling_request, RAW_AUTH_SENTINEL_PR80_B);
    assert_bearer_auth_header_not_contains(sibling_request, RAW_AUTH_SENTINEL_PR80_A);

    Ok(())
}

#[tokio::test]
async fn concurrent_stream_response_bodies_do_not_share_chunks_or_counters()
-> Result<(), ApiClientError> {
    let left_reads = Arc::new(AtomicUsize::new(0));
    let right_reads = Arc::new(AtomicUsize::new(0));
    let left_transport = GateTransport::new(
        Arc::new(Mutex::new(Vec::new())),
        vec![octet_stream_response(
            vec![Bytes::from_static(b"left-"), Bytes::from_static(b"one")],
            left_reads.clone(),
        )],
    );
    let right_transport = GateTransport::new(
        Arc::new(Mutex::new(Vec::new())),
        vec![octet_stream_response(
            vec![Bytes::from_static(b"right-"), Bytes::from_static(b"two")],
            right_reads.clone(),
        )],
    );
    let left_sent = left_transport.clone();
    let right_sent = right_transport.clone();
    let left_client = Arc::new(ApiClient::<TestCx, _>::with_transport(
        (),
        TestAuthVars::default(),
        left_transport,
    ));
    let right_client = Arc::new(ApiClient::<TestCx, _>::with_transport(
        (),
        TestAuthVars::default(),
        right_transport,
    ));

    let left_plan = stream_response_plan("LeftStream", "/left-stream")?;
    let right_plan = stream_response_plan("RightStream", "/right-stream")?;

    let left_task = tokio::spawn({
        let client = left_client.clone();
        async move {
            let mut response =
                RawStreamResponse::<OctetStream>::execute(&client, left_plan).await?;
            collect_stream_response(&mut response).await
        }
    });
    let right_task = tokio::spawn({
        let client = right_client.clone();
        async move {
            let mut response =
                RawStreamResponse::<OctetStream>::execute(&client, right_plan).await?;
            collect_stream_response(&mut response).await
        }
    });

    left_sent.wait_for_sends(1).await;
    right_sent.wait_for_sends(1).await;
    left_sent.release_all();
    right_sent.release_all();

    let left_value = left_task
        .await
        .expect("left stream task should join")
        .expect("left stream request should succeed");
    let right_value = right_task
        .await
        .expect("right stream task should join")
        .expect("right stream request should succeed");
    assert_eq!(left_value, "left-one");
    assert_eq!(right_value, "right-two");
    assert_eq!(left_reads.load(AtomicOrdering::SeqCst), 2);
    assert_eq!(right_reads.load(AtomicOrdering::SeqCst), 2);
    assert_eq!(left_sent.sent_count().await, 1);
    assert_eq!(right_sent.sent_count().await, 1);

    Ok(())
}

#[derive(Clone)]
struct BodyEndpoint {
    name: &'static str,
    path: &'static str,
    body: BodyKind,
}

#[derive(Clone)]
enum BodyKind {
    Bytes(Bytes),
    Stream(Bytes),
}

impl Endpoint<TestCx> for BodyEndpoint {
    type Response = String;

    buffered_endpoint_execute!(TestCx, concord_core::prelude::Text<String>);
}

buffered_endpoint_response_terminal!(BodyEndpoint, TestCx, concord_core::prelude::Text<String>);

impl ReusableEndpoint<TestCx> for BodyEndpoint {
    fn plan(
        &self,
        _ctx: &concord_core::internal::ClientPlanContext<'_, TestCx>,
    ) -> Result<concord_core::internal::RequestPlan, ApiClientError> {
        let mut plan = request_plan(
            self.name,
            http::Method::POST,
            self.path,
            ResolvedPolicy::default(),
            None,
        );
        plan.body = match &self.body {
            BodyKind::Bytes(bytes) => PreparedBody::reusable_bytes(
                bytes.clone(),
                Some(HeaderValue::from_static("application/json")),
            ),
            BodyKind::Stream(bytes) => PreparedBody::from_stream_body(
                StreamBody::from_bytes(bytes.clone()),
                Some(HeaderValue::from_static("application/octet-stream")),
            ),
        };
        Ok(plan)
    }
}

fn stream_response_plan(
    name: &'static str,
    path: &'static str,
) -> Result<concord_core::internal::RequestPlan, ApiClientError> {
    let mut plan = request_plan(
        name,
        http::Method::GET,
        path,
        ResolvedPolicy::default(),
        None,
    );
    plan.endpoint.response =
        <RawStreamResponse<OctetStream> as ResponseEntity>::plan(ErrorContext {
            endpoint: name,
            method: http::Method::GET,
        })?
        .response_plan;
    Ok(plan)
}

async fn collect_stream_response(
    response: &mut concord_core::advanced::StreamResponse<OctetStream>,
) -> Result<String, ApiClientError> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response.next_chunk().await? {
        bytes.extend_from_slice(&chunk);
    }
    Ok(String::from_utf8(bytes).expect("stream chunks should be valid utf-8"))
}

fn octet_stream_response(chunks: Vec<Bytes>, read_count: Arc<AtomicUsize>) -> MockResponse {
    let content_length = chunks
        .iter()
        .try_fold(0u64, |len, chunk| len.checked_add(chunk.len() as u64))
        .expect("stream response content length should fit in u64");
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    MockResponse {
        status: StatusCode::OK,
        headers,
        body: Bytes::new(),
        content_length: Some(content_length),
        chunks: Some(chunks),
        read_count: Some(read_count),
    }
}

fn spawn_text_request<T>(
    client: Arc<ApiClient<TestCx, T>>,
    endpoint: TextEndpoint,
) -> tokio::task::JoinHandle<Result<String, ApiClientError>>
where
    T: concord_core::advanced::Transport + Clone + Send + Sync + 'static,
{
    tokio::spawn(async move {
        client
            .request(endpoint)
            .response()
            .await
            .map(|response| response.into_value())
    })
}

fn spawn_single_flight_request<T>(
    client: Arc<ApiClient<SingleFlightCx, T>>,
    endpoint: TextEndpoint,
) -> tokio::task::JoinHandle<Result<String, ApiClientError>>
where
    T: concord_core::advanced::Transport + Clone + Send + Sync + 'static,
{
    tokio::spawn(async move {
        client
            .request(endpoint)
            .response()
            .await
            .map(|response| response.into_value())
    })
}

#[derive(Clone)]
struct SingleFlightCx;

#[derive(Clone)]
struct SingleFlightAuthVars {
    provider: ControlledTokenProvider,
}

#[derive(Clone)]
struct SingleFlightAuthState {
    token: Arc<CredentialSlot<SingleFlightCx, ControlledTokenProvider>>,
}

impl ClientContext for SingleFlightCx {
    type Vars = ();
    type AuthVars = SingleFlightAuthVars;
    type AuthState = SingleFlightAuthState;
    const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
    const DOMAIN: &'static str = "example.com";

    fn init_auth_state(_vars: &Self::Vars, auth: &Self::AuthVars) -> Self::AuthState {
        SingleFlightAuthState {
            token: Arc::new(CredentialSlot::new(auth.provider.clone())),
        }
    }

    fn prepare_auth_requirement<'a>(
        requirement: &'a AuthRequirement,
        request: &'a mut concord_core::advanced::AuthApplicationRequest<'_>,
        vars: &'a Self::Vars,
        auth: &'a Self::AuthVars,
        auth_state: &'a Self::AuthState,
        executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
        _meta: &'a RequestMeta,
    ) -> AuthFuture<'a, Result<PreparedAuthCredential, AuthError>> {
        Box::pin(async move {
            let ctx = CredentialContext {
                vars,
                auth,
                auth_state,
                executor,
                credential_id: requirement.credential.id.clone(),
                reason: CredentialRefreshReason::Missing,
            };
            let lease = auth_state
                .token
                .get_or_refresh(ctx, AuthStepPolicy::default())
                .await?;
            let application = apply_secret_credential(request, requirement, &lease.value)?;
            let applied = AuthAppliedCredential {
                credential_id: requirement.credential.id.clone(),
                usage_id: requirement.usage_id.clone(),
                step_id: requirement.step_id,
                generation: Some(lease.generation),
                provenance: requirement.provenance.clone(),
            };
            Ok(PreparedAuthCredential::new(applied, application))
        })
    }
}

impl Endpoint<SingleFlightCx> for TextEndpoint {
    type Response = String;

    buffered_endpoint_execute!(SingleFlightCx, concord_core::prelude::Text<String>);
}

buffered_endpoint_response_terminal!(
    TextEndpoint,
    SingleFlightCx,
    concord_core::prelude::Text<String>
);

impl ReusableEndpoint<SingleFlightCx> for TextEndpoint {
    fn plan(
        &self,
        _ctx: &concord_core::internal::ClientPlanContext<'_, SingleFlightCx>,
    ) -> Result<concord_core::internal::RequestPlan, ApiClientError> {
        Ok(request_plan(
            self.name,
            self.method.clone(),
            self.path,
            self.policy.clone(),
            self.pagination
                .as_ref()
                .map(|_| concord_core::internal::PaginationMarker),
        ))
    }
}

#[derive(Clone)]
struct ControlledTokenProvider {
    id: CredentialId,
    token: &'static str,
    acquire_count: Arc<Mutex<usize>>,
    acquired: Arc<Notify>,
    release: watch::Sender<bool>,
}

impl ControlledTokenProvider {
    fn new(token: &'static str) -> Self {
        let (release, _) = watch::channel(false);
        Self {
            id: CredentialId::new("test", "token"),
            token,
            acquire_count: Arc::new(Mutex::new(0)),
            acquired: Arc::new(Notify::new()),
            release,
        }
    }

    async fn acquire_count(&self) -> usize {
        *self.acquire_count.lock().await
    }

    async fn wait_for_acquires(&self, expected: usize) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let notified = self.acquired.notified();
                if self.acquire_count().await >= expected {
                    break;
                }
                notified.await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("timed out waiting for {expected} credential acquisitions"));
    }

    fn release_all(&self) {
        let _ = self.release.send(true);
    }
}

impl CredentialProvider<SingleFlightCx> for ControlledTokenProvider {
    type Credential = AccessToken;

    fn id(&self) -> CredentialId {
        self.id.clone()
    }

    fn acquire<'a>(
        &'a self,
        _ctx: CredentialContext<'a, SingleFlightCx>,
    ) -> AuthFuture<'a, Result<Self::Credential, AuthError>> {
        let mut release = self.release.subscribe();
        Box::pin(async move {
            *self.acquire_count.lock().await += 1;
            self.acquired.notify_waiters();

            while !*release.borrow() {
                if release.changed().await.is_err() {
                    break;
                }
            }

            Ok(AccessToken::new(self.token.to_string()))
        })
    }
}

#[derive(Clone)]
struct RoutedTransport {
    gate: PhaseGate,
    events: Arc<Mutex<Vec<String>>>,
    routes: Arc<Mutex<HashMap<String, VecDeque<MockOutcome>>>>,
    requests: Arc<Mutex<Vec<CapturedTransportRequest>>>,
}

impl RoutedTransport {
    fn new(gate: PhaseGate, events: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            gate,
            events,
            routes: Arc::new(Mutex::new(HashMap::new())),
            requests: Arc::new(Mutex::new(Vec::new())),
        }
    }

    async fn insert(&self, path: impl Into<String>, outcome: MockOutcome) {
        self.routes
            .lock()
            .await
            .entry(path.into())
            .or_insert_with(VecDeque::new)
            .push_back(outcome);
    }

    async fn sent_count(&self) -> usize {
        self.requests.lock().await.len()
    }

    async fn requests(&self) -> Vec<CapturedTransportRequest> {
        let mut requests = self.requests.lock().await;
        std::mem::take(&mut *requests)
    }
}

impl Transport for RoutedTransport {
    fn send(
        &self,
        req: http::Request<DynBody>,
    ) -> Pin<Box<dyn Future<Output = Result<http::Response<DynBody>, TransportError>> + Send>> {
        let gate = self.gate.clone();
        let events = self.events.clone();
        let routes = self.routes.clone();
        let requests = self.requests.clone();
        Box::pin(async move {
            let captured = capture_request(req).await?;
            let path = captured.url.path().to_string();
            events.lock().await.push(format!("transport_send:{path}"));
            requests.lock().await.push(captured);
            gate.enter("transport_send").await;
            let outcome = routes
                .lock()
                .await
                .get_mut(&path)
                .and_then(VecDeque::pop_front)
                .unwrap_or_else(|| MockResponse::text(StatusCode::OK, "ok").into());
            let response = match outcome {
                MockOutcome::Response(response) => response,
                MockOutcome::TransportError(kind) => {
                    return Err(TransportError::with_kind(
                        kind,
                        std::io::Error::other("routed transport error"),
                    ));
                }
            };
            Ok(standard_response(response))
        })
    }
}

fn rate_limit_policy_with_bucket(
    bucket_name: &'static str,
    key_name: &'static str,
    key_value: &'static str,
) -> ResolvedPolicy {
    let mut policy = ResolvedPolicy::default();
    let mut plan = RateLimitPlan::new();
    plan.push_bucket(
        RateLimitBucketUse::new(
            "async-harness",
            bucket_name,
            concord_core::advanced::RateLimitKey::new(vec![RateLimitKeyPart::static_value(
                key_name, key_value,
            )]),
        )
        .with_window(concord_core::advanced::RateLimitWindow::new(
            std::num::NonZeroU32::new(10).expect("non-zero"),
            Duration::from_secs(1),
        )),
    );
    policy.rate_limit = plan;
    policy
}

fn rate_limit_plan_label(plan: &RateLimitPlan) -> String {
    let bucket = plan
        .buckets()
        .first()
        .expect("rate-limit plan should have a single bucket");
    let parts = bucket
        .key
        .parts()
        .iter()
        .map(|part| format!("{}={:?}", part.name, part.value))
        .collect::<Vec<_>>()
        .join(",");
    format!("{}:{}:{parts}", bucket.id.kind, bucket.id.name)
}

#[derive(Clone)]
struct KeyBlockingRateLimiter {
    events: Arc<Mutex<Vec<String>>>,
    gate: PhaseGate,
    blocked_label: String,
    blocked_phase: &'static str,
    acquire_started: Arc<AtomicUsize>,
    response_observed: Arc<AtomicUsize>,
}

impl KeyBlockingRateLimiter {
    fn new(
        events: Arc<Mutex<Vec<String>>>,
        gate: PhaseGate,
        blocked_label: impl Into<String>,
        blocked_phase: &'static str,
    ) -> Self {
        Self {
            events,
            gate,
            blocked_label: blocked_label.into(),
            blocked_phase,
            acquire_started: Arc::new(AtomicUsize::new(0)),
            response_observed: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn label_for(plan: &RateLimitPlan) -> String {
        rate_limit_plan_label(plan)
    }
}

impl RateLimiter for KeyBlockingRateLimiter {
    fn acquire<'a>(
        &'a self,
        ctx: RateLimitContext<'a>,
    ) -> concord_core::advanced::RateLimitFuture<'a, Result<RateLimitPermit, ApiClientError>> {
        Box::pin(async move {
            let label = Self::label_for(ctx.plan);
            self.acquire_started.fetch_add(1, AtomicOrdering::SeqCst);
            self.events.lock().await.push(format!(
                "rate_acquire:{}:{}:{}:{label}",
                ctx.endpoint, ctx.method, ctx.url
            ));
            if label == self.blocked_label {
                self.gate.enter(self.blocked_phase).await;
            }
            Ok(RateLimitPermit)
        })
    }

    fn on_response<'a>(
        &'a self,
        ctx: RateLimitResponseContext<'a>,
    ) -> concord_core::advanced::RateLimitFuture<'a, Result<RateLimitResponseAction, ApiClientError>>
    {
        Box::pin(async move {
            let label = Self::label_for(ctx.meta.plan);
            self.response_observed.fetch_add(1, AtomicOrdering::SeqCst);
            self.events.lock().await.push(format!(
                "rate_response:{}:{}:{}:{label}:{}",
                ctx.meta.endpoint, ctx.meta.method, ctx.meta.url, ctx.status
            ));
            Ok(RateLimitResponseAction::Continue)
        })
    }
}

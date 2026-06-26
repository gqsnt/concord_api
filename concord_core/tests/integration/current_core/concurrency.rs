use super::common::*;
use bytes::Bytes;
use concord_core::advanced::{
    AuthAppliedCredential, AuthDecision, AuthError, AuthFuture, AuthPlacement, AuthRequirement,
    AuthStepPolicy, BuiltRequest, BuiltResponse, CacheAfter, CacheBefore, CacheFuture,
    CacheRevalidation, CacheStore, CredentialContext, CredentialId, CredentialProvider,
    CredentialRefreshReason, CredentialSlot, PreparedAuthCredential, RateLimitBucketUse,
    RateLimitContext, RateLimitKeyPart, RateLimitPermit, RateLimitPlan, RateLimitResponseAction,
    RateLimitResponseContext, RateLimiter, RequestMeta, Transport, TransportBody, TransportError,
    TransportErrorKind, TransportRequest, TransportResponse, apply_secret_credential,
};
use concord_core::internal::{PaginationPlan, ResolvedPolicy};
use concord_core::prelude::{
    AccessToken, ApiClient, ApiClientError, ClientContext, CursorPagination, Endpoint,
    PaginationTermination,
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
    let client = Arc::new(ApiClient::<TestCx, _>::with_transport(
        (),
        TestAuthVars::default(),
        transport,
    ));

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
    let client = Arc::new(ApiClient::<TestCx, _>::with_transport(
        (),
        TestAuthVars::default(),
        transport,
    ));
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
async fn cache_hit_after_completed_response_still_avoids_transport() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let cache = Arc::new(StoringCache::default());
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "stored"),
            MockResponse::text(StatusCode::OK, "unexpected"),
        ],
    );
    let sent = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    configure_runtime(&mut client, Some(cache), None);
    let endpoint = TextEndpoint {
        policy: cache_policy(),
        ..Default::default()
    };

    let first = client.request(endpoint.clone()).execute_decoded().await?;
    let second = client.request(endpoint).execute_decoded().await?;

    assert_eq!(first.value(), "stored");
    assert_eq!(second.value(), "stored");
    assert_eq!(sent.sent_count().await, 1);
    Ok(())
}

#[tokio::test]
async fn concurrent_cache_miss_requests_both_send_transport() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let cache = Arc::new(StoringCache::default());
    let transport = GateTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "first"),
            MockResponse::text(StatusCode::OK, "second"),
        ],
    );
    let sent = transport.clone();
    let mut client = ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport);
    configure_runtime(&mut client, Some(cache), None);
    let client = Arc::new(client);
    let endpoint = TextEndpoint {
        policy: cache_policy(),
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
async fn concurrent_fresh_cache_hits_bypass_transport() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let cache = Arc::new(RecordingCache::hit(
        events.clone(),
        built_response("Text", StatusCode::OK, "cached"),
    ));
    let limiter = Arc::new(RecordingRateLimiter::new(events.clone()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "unexpected")],
    );
    let sent = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    configure_runtime(&mut client, Some(cache), Some(limiter));
    let client = Arc::new(client);
    let endpoint = TextEndpoint {
        policy: cache_policy(),
        ..Default::default()
    };

    let a = spawn_text_request(client.clone(), endpoint.clone());
    let b = spawn_text_request(client, endpoint);

    let values = vec![
        a.await.expect("request task panicked")?,
        b.await.expect("request task panicked")?,
    ];
    assert_eq!(values, vec!["cached".to_string(), "cached".to_string()]);
    assert_eq!(sent.sent_count().await, 0);

    let events = events.lock().await.clone();
    assert_eq!(
        events
            .iter()
            .filter(|event| event.as_str() == "rate_acquire")
            .count(),
        0
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.as_str() == "transport")
            .count(),
        0
    );
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
    configure_runtime(&mut client, None, Some(limiter));
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
    let client = Arc::new(ApiClient::<TestCx, _>::with_transport(
        (),
        TestAuthVars::default(),
        transport,
    ));
    let endpoint = TextEndpoint {
        policy: retry_policy(2),
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
    assert_eq!(sent.sent_count().await, 4);
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
                .execute_decoded()
                .await
        }
    });
    let task_b = tokio::spawn({
        let client = client.clone();
        async move { client.request(b).execute_decoded().await }
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
            .execute_decoded()
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
    assert!(matches!(err, ApiClientError::ResponseTooLarge { .. }));

    let later = reconfigured_client
        .request(TextEndpoint::default())
        .execute_decoded()
        .await
        .expect("later request should use the updated no-limit config");
    assert_eq!(later.value(), "abcde");
}

#[tokio::test]
async fn concurrent_cache_hit_and_miss_order_is_deterministic() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let gate = PhaseGate::new();
    let cache = Arc::new(RoutedCache::new(CacheKeyMode::Path, events.clone()));
    cache
        .insert("/hit", built_response("Text", StatusCode::OK, "cached"))
        .await;
    let limiter = Arc::new(RecordingRateLimiter::new(events.clone()));
    let transport = RoutedTransport::new(gate.clone(), events.clone());
    transport
        .insert(
            "/miss",
            MockOutcome::Response(
                MockResponse::text(StatusCode::OK, "live").with_content_length(Some(4)),
            ),
        )
        .await;
    let mut client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
    client.configure(|cfg| {
        cfg.cache_store(cache.clone());
        cfg.rate_limiter(limiter.clone());
    });
    let hit = TextEndpoint {
        name: "Hit",
        path: "/hit",
        policy: cache_and_rate_limit_policy(),
        ..Default::default()
    };
    let miss = TextEndpoint {
        name: "Miss",
        path: "/miss",
        policy: cache_and_rate_limit_policy(),
        ..Default::default()
    };

    gate.block("transport_send").await;
    let hit_task = tokio::spawn({
        let client = client.clone();
        let hit = hit.clone();
        async move {
            client
                .request(hit)
                .execute_decoded()
                .await
                .map(|response| response.into_value())
        }
    });
    let miss_task = tokio::spawn({
        let client = client.clone();
        async move {
            client
                .request(miss)
                .execute_decoded()
                .await
                .map(|response| response.into_value())
        }
    });

    let hit_value = hit_task
        .await
        .expect("hit task should join")
        .expect("hit request should complete from cache");
    assert_eq!(hit_value, "cached");
    assert_eq!(transport.sent_count().await, 1);
    assert_eq!(cache.after_response_count(), 0);

    gate.release_all("transport_send").await;
    let miss_value = miss_task
        .await
        .expect("miss task should join")
        .expect("miss request should complete");
    assert_eq!(miss_value, "live");
    assert_eq!(transport.sent_count().await, 1);
    assert_eq!(cache.after_response_count(), 1);
    assert_eq!(
        limiter
            .events
            .lock()
            .await
            .iter()
            .filter(|event| *event == "rate_acquire")
            .count(),
        1
    );
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
async fn concurrent_success_and_decode_failure_cache_admission_isolated()
-> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let gate = PhaseGate::new();
    let cache = Arc::new(RoutedCache::new(CacheKeyMode::Path, events.clone()));
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
        cfg.cache_store(cache.clone());
        cfg.rate_limiter(limiter.clone());
    });
    let ok = TextEndpoint {
        name: "Ok",
        path: "/ok",
        policy: cache_and_rate_limit_policy(),
        ..Default::default()
    };
    let bad = TextEndpoint {
        name: "Bad",
        path: "/bad",
        policy: cache_and_rate_limit_policy(),
        ..Default::default()
    };

    gate.block("transport_send").await;
    let ok_task = tokio::spawn({
        let client = client.clone();
        let ok = ok.clone();
        async move { client.request(ok).execute_decoded().await }
    });
    let bad_task = tokio::spawn({
        let client = client.clone();
        async move { client.request(bad).execute_decoded().await }
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
    assert_eq!(cache.after_response_count(), 1);
    assert_eq!(cache.after_error_count(), 0);
    assert_eq!(cache.entry_count().await, 1);
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
                .execute_decoded()
                .await
                .map(|response| response.into_value())
        }
    });
    let task_b = tokio::spawn({
        let client = client.clone();
        async move {
            client
                .request(b)
                .execute_decoded()
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
async fn concurrent_cache_hit_bypasses_rate_limit_while_other_request_waits()
-> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let gate = PhaseGate::new();
    let cache = Arc::new(RoutedCache::new(CacheKeyMode::Path, events.clone()));
    cache
        .insert("/hit", built_response("Text", StatusCode::OK, "cached"))
        .await;
    let limiter_policy_miss = rate_limit_policy_with_bucket("bucket-miss", "route", "miss");
    let blocked_label = rate_limit_plan_label(&limiter_policy_miss.rate_limit);
    let limiter = Arc::new(KeyBlockingRateLimiter::new(
        events.clone(),
        gate.clone(),
        blocked_label,
        "rate_acquire_miss",
    ));
    let transport = RoutedTransport::new(gate.clone(), events.clone());
    transport
        .insert(
            "/miss",
            MockOutcome::Response(MockResponse::text(StatusCode::OK, "live")),
        )
        .await;
    let mut client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
    client.configure(|cfg| {
        cfg.cache_store(cache.clone());
        cfg.rate_limiter(limiter.clone());
    });
    let hit = TextEndpoint {
        name: "Hit",
        path: "/hit",
        policy: cache_policy(),
        ..Default::default()
    };
    let miss = TextEndpoint {
        name: "Miss",
        path: "/miss",
        policy: limiter_policy_miss,
        ..Default::default()
    };

    gate.block("rate_acquire_miss").await;
    let hit_task = tokio::spawn({
        let client = client.clone();
        let hit = hit.clone();
        async move { client.request(hit).execute_decoded().await }
    });
    let miss_task = tokio::spawn({
        let client = client.clone();
        async move { client.request(miss).execute_decoded().await }
    });

    gate.wait_for("rate_acquire_miss", 1).await;

    let hit_value = hit_task
        .await
        .expect("hit task should join")
        .expect("hit request should complete from cache");
    assert_eq!(hit_value.value(), "cached");
    assert!(!miss_task.is_finished());
    assert_eq!(transport.sent_count().await, 0);
    assert_eq!(limiter.response_observed.load(AtomicOrdering::SeqCst), 0);
    assert_eq!(limiter.acquire_started.load(AtomicOrdering::SeqCst), 1);

    gate.release_all("rate_acquire_miss").await;
    let miss_value = miss_task
        .await
        .expect("miss task should join")
        .expect("miss request should complete");
    assert_eq!(miss_value.value(), "live");
    assert_eq!(transport.sent_count().await, 1);
    assert_eq!(limiter.response_observed.load(AtomicOrdering::SeqCst), 1);
    Ok(())
}

#[tokio::test]
async fn concurrent_transport_error_and_success_are_isolated() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let gate = PhaseGate::new();
    let cache = Arc::new(RoutedCache::new(CacheKeyMode::Path, events.clone()));
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
        cfg.cache_store(cache.clone());
        cfg.rate_limiter(limiter.clone());
    });
    let ok = TextEndpoint {
        name: "Ok",
        path: "/ok",
        policy: cache_and_rate_limit_policy(),
        ..Default::default()
    };
    let err = TextEndpoint {
        name: "Err",
        path: "/err",
        policy: cache_and_rate_limit_policy(),
        ..Default::default()
    };

    gate.block("transport_send").await;
    let ok_task = tokio::spawn({
        let client = client.clone();
        let ok = ok.clone();
        async move { client.request(ok).execute_decoded().await }
    });
    let err_task = tokio::spawn({
        let client = client.clone();
        async move { client.request(err).execute_decoded().await }
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
    assert_eq!(cache.after_response_count(), 1);
    assert_eq!(cache.entry_count().await, 1);
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
async fn concurrent_auth_identities_do_not_share_cache_identity() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let cache = Arc::new(RoutedCache::new(CacheKeyMode::Default, events.clone()));
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::OK, "shared"),
            MockResponse::text(StatusCode::OK, "shared"),
        ],
    );
    let mut client_a = client(
        TestAuthVars {
            token: Some("token-a".to_string()),
            identity: "alpha",
        },
        transport.clone(),
    );
    let mut client_b = client(
        TestAuthVars {
            token: Some("token-b".to_string()),
            identity: "beta",
        },
        transport.clone(),
    );
    client_a.configure(|cfg| {
        cfg.cache_store(cache.clone());
    });
    client_b.configure(|cfg| {
        cfg.cache_store(cache.clone());
    });
    let endpoint = TextEndpoint {
        policy: {
            let mut policy = cache_policy();
            policy.auth = auth_policy(AuthPlacement::Bearer).auth;
            policy
        },
        ..Default::default()
    };

    let first_a = client_a.request(endpoint.clone()).execute_decoded();
    let first_b = client_b.request(endpoint.clone()).execute_decoded();
    let (first_a, first_b) = tokio::join!(first_a, first_b);
    assert_eq!(first_a?.value(), "shared");
    assert_eq!(first_b?.value(), "shared");
    assert_eq!(cache.entry_count().await, 2);
    assert_eq!(transport.sent_count().await, 2);

    let second_a = client_a.request(endpoint.clone()).execute_decoded();
    let second_b = client_b.request(endpoint).execute_decoded();
    let (second_a, second_b) = tokio::join!(second_a, second_b);
    assert_eq!(second_a?.value(), "shared");
    assert_eq!(second_b?.value(), "shared");
    assert_eq!(transport.sent_count().await, 2);
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
                    pagination: PaginationPlan::Paged {
                        page_key: "page".to_string(),
                        per_page_key: "per_page".to_string(),
                        page: 1,
                        per_page: 2,
                    },
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
                    pagination: PaginationPlan::cursor::<CursorItems>(CursorPagination {
                        cursor_key: "cursor".into(),
                        per_page_key: "limit".into(),
                        cursor: Some("start".to_string()),
                        per_page: 2,
                        send_cursor_on_first: true,
                        stop_when_cursor_missing: true,
                    }),
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

    let cursor_requests: Vec<_> = cursor_requests
        .iter()
        .filter(|request| request.url.path() == "/cursor-items")
        .collect();
    assert_eq!(cursor_requests.len(), 2);
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
    let cache = Arc::new(RecordingCache::miss(events.clone()));
    let limiter = Arc::new(ObservationRateLimiter::new(events.clone()));
    let hooks = Arc::new(ObservationRuntimeHooks::new(events.clone()));

    let mut policy = auth_policy(AuthPlacement::Bearer);
    policy.cache = cache_policy().cache;
    policy.rate_limit = rate_limit_policy().rate_limit;

    let mut client_a = ApiClient::<ObservationAuthCx, _>::with_transport(
        (),
        ObservationAuthVars::bearer(RAW_AUTH_SENTINEL_PR80_A, "observer-a", events.clone()),
        transport.clone(),
    );
    client_a.configure(|cfg| {
        cfg.cache_store(cache.clone());
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
        cfg.cache_store(cache.clone());
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
        async move { client.request(endpoint).execute_decoded().await }
    });
    let b = tokio::spawn({
        let client = Arc::new(client_b);
        let endpoint = endpoint.clone();
        async move { client.request(endpoint).execute_decoded().await }
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
async fn concurrent_execute_raw_and_decoded_request_do_not_share_endpoint_cache()
-> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let gate = PhaseGate::new();
    let cache = Arc::new(RoutedCache::new(CacheKeyMode::Path, events.clone()));
    cache
        .insert("/shared", built_response("Text", StatusCode::OK, "cached"))
        .await;
    let transport = RoutedTransport::new(gate.clone(), events.clone());
    transport
        .insert(
            "/shared",
            MockOutcome::Response(MockResponse::text(StatusCode::OK, "live")),
        )
        .await;
    let mut client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
    client.configure(|cfg| {
        cfg.cache_store(cache.clone());
    });
    let endpoint = TextEndpoint {
        name: "Shared",
        path: "/shared",
        policy: cache_policy(),
        ..Default::default()
    };

    gate.block("transport_send").await;
    let raw_task = tokio::spawn({
        let client = client.clone();
        let endpoint = endpoint.clone();
        async move { client.request(endpoint).execute_raw().await }
    });
    let decoded_task = tokio::spawn({
        let client = client.clone();
        async move { client.request(endpoint).execute_decoded().await }
    });

    let decoded = decoded_task
        .await
        .expect("decoded task should join")
        .expect("decoded request should use cache");
    assert_eq!(decoded.value(), "cached");
    assert_eq!(cache.after_response_count(), 0);
    assert_eq!(cache.after_error_count(), 0);
    assert_eq!(transport.sent_count().await, 1);
    gate.release_one("transport_send").await;
    let raw = raw_task
        .await
        .expect("raw task should join")
        .expect("raw request should complete");
    assert_eq!(raw.status, StatusCode::OK);
    assert_eq!(raw.body, Bytes::from_static(b"live"));
    assert_eq!(transport.sent_count().await, 1);
    assert_eq!(cache.after_response_count(), 0);
    assert_eq!(cache.after_error_count(), 0);
    assert_eq!(
        events
            .lock()
            .await
            .iter()
            .filter(|event| event.starts_with("cache_before:"))
            .count(),
        1
    );
    Ok(())
}

#[tokio::test]
async fn concurrent_cancelled_request_does_not_admit_or_poison_successful_request()
-> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let gate = PhaseGate::new();
    let transport_probe = DropProbe::new("transport", events.clone());
    let cache = Arc::new(RoutedCache::new(CacheKeyMode::Path, events.clone()));
    let transport = GateableTransport::new(
        gate.clone(),
        events.clone(),
        vec![
            MockResponse::text(StatusCode::OK, "one").with_content_length(Some(3)),
            MockResponse::text(StatusCode::OK, "two").with_content_length(Some(3)),
        ],
    )
    .with_drop_probe(transport_probe.clone());
    let mut client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
    client.configure(|cfg| {
        cfg.cache_store(cache.clone());
    });
    let endpoint = TextEndpoint {
        policy: cache_policy(),
        ..Default::default()
    };

    gate.block("transport_send").await;
    let cancelled = tokio::spawn({
        let client = client.clone();
        let endpoint = endpoint.clone();
        async move { client.request(endpoint).execute_decoded().await }
    });
    let survivor = tokio::spawn({
        let client = client.clone();
        async move { client.request(endpoint).execute_decoded().await }
    });

    gate.wait_for("transport_send", 2).await;
    cancelled.abort();
    let cancelled_join = cancelled.await;
    assert!(cancelled_join.is_err());
    transport_probe.wait_for(1).await;
    gate.release_all("transport_send").await;
    let survivor = survivor
        .await
        .expect("survivor task should join")
        .expect("survivor request should complete");
    assert_eq!(survivor.value(), "one");
    transport_probe.wait_for(2).await;
    assert_eq!(cache.after_response_count(), 1);
    assert_eq!(transport.sent_count().await, 2);
    assert_eq!(cache.entry_count().await, 1);
    Ok(())
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
            .execute_decoded()
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
            .execute_decoded()
            .await
            .map(|response| response.into_value())
    })
}

#[derive(Default)]
struct StoringCache {
    response: Mutex<Option<BuiltResponse>>,
}

impl CacheStore for StoringCache {
    fn before_request<'a>(&'a self, _request: &'a BuiltRequest) -> CacheFuture<'a, CacheBefore> {
        Box::pin(async move {
            match self.response.lock().await.clone() {
                Some(response) => CacheBefore::Hit(response),
                None => CacheBefore::Miss,
            }
        })
    }

    fn after_response<'a>(
        &'a self,
        _request: &'a BuiltRequest,
        response: &'a BuiltResponse,
        _revalidation: Option<CacheRevalidation>,
    ) -> CacheFuture<'a, CacheAfter> {
        Box::pin(async move {
            *self.response.lock().await = Some(response.clone());
            CacheAfter::Stored
        })
    }
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
                identity: application.identity().clone(),
                provenance: requirement.provenance.clone(),
            };
            Ok(PreparedAuthCredential::new(applied, application))
        })
    }

    fn handle_auth_response<'a>(
        _requirement: &'a AuthRequirement,
        _applied: &'a AuthAppliedCredential,
        _vars: &'a Self::Vars,
        _auth: &'a Self::AuthVars,
        _auth_state: &'a Self::AuthState,
        _executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
        _meta: &'a RequestMeta,
        _status: StatusCode,
        _headers: &'a HeaderMap,
    ) -> AuthFuture<'a, Result<AuthDecision, AuthError>> {
        Box::pin(async { Ok(AuthDecision::Continue) })
    }
}

impl Endpoint<SingleFlightCx> for TextEndpoint {
    type Response = String;

    fn plan(
        &self,
        _ctx: &concord_core::internal::ClientPlanContext<'_, SingleFlightCx>,
    ) -> Result<concord_core::internal::RequestPlan, ApiClientError> {
        Ok(request_plan(
            self.name,
            self.method.clone(),
            self.path,
            self.policy.clone(),
            self.pagination.clone(),
            decode_string,
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

#[derive(Clone, Copy)]
enum CacheKeyMode {
    Path,
    Default,
}

#[derive(Clone)]
struct RoutedCache {
    mode: CacheKeyMode,
    entries: Arc<Mutex<HashMap<String, BuiltResponse>>>,
    events: Arc<Mutex<Vec<String>>>,
    after_response_count: Arc<AtomicUsize>,
    after_error_count: Arc<AtomicUsize>,
}

impl RoutedCache {
    fn new(mode: CacheKeyMode, events: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            mode,
            entries: Arc::new(Mutex::new(HashMap::new())),
            events,
            after_response_count: Arc::new(AtomicUsize::new(0)),
            after_error_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn key_for_request(&self, request: &BuiltRequest) -> String {
        match self.mode {
            CacheKeyMode::Path => request.url.path().to_string(),
            CacheKeyMode::Default => concord_core::advanced::default_cache_key(request)
                .as_str()
                .to_string(),
        }
    }

    async fn insert(&self, key: impl Into<String>, response: BuiltResponse) {
        self.entries.lock().await.insert(key.into(), response);
    }

    async fn entry_count(&self) -> usize {
        self.entries.lock().await.len()
    }

    fn after_response_count(&self) -> usize {
        self.after_response_count.load(AtomicOrdering::SeqCst)
    }

    fn after_error_count(&self) -> usize {
        self.after_error_count.load(AtomicOrdering::SeqCst)
    }
}

impl CacheStore for RoutedCache {
    fn before_request<'a>(&'a self, request: &'a BuiltRequest) -> CacheFuture<'a, CacheBefore> {
        Box::pin(async move {
            let key = self.key_for_request(request);
            self.events.lock().await.push(format!("cache_before:{key}"));
            match self.entries.lock().await.get(&key).cloned() {
                Some(response) => CacheBefore::Hit(response),
                None => CacheBefore::Miss,
            }
        })
    }

    fn after_response<'a>(
        &'a self,
        request: &'a BuiltRequest,
        response: &'a BuiltResponse,
        _revalidation: Option<CacheRevalidation>,
    ) -> CacheFuture<'a, CacheAfter> {
        Box::pin(async move {
            let key = self.key_for_request(request);
            self.after_response_count
                .fetch_add(1, AtomicOrdering::SeqCst);
            self.events
                .lock()
                .await
                .push(format!("cache_after_response:{key}"));
            self.entries.lock().await.insert(key, response.clone());
            CacheAfter::Stored
        })
    }

    fn after_error<'a>(
        &'a self,
        request: &'a BuiltRequest,
        _error: &'a ApiClientError,
        _revalidation: Option<CacheRevalidation>,
    ) -> CacheFuture<'a, Option<BuiltResponse>> {
        Box::pin(async move {
            let key = self.key_for_request(request);
            self.after_error_count.fetch_add(1, AtomicOrdering::SeqCst);
            self.events
                .lock()
                .await
                .push(format!("cache_after_error:{key}"));
            None
        })
    }
}

struct RoutedStaticBody {
    body: Option<Bytes>,
}

impl TransportBody for RoutedStaticBody {
    fn next_chunk<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Bytes>, TransportError>> + Send + 'a>> {
        Box::pin(async move { Ok(self.body.take()) })
    }
}

#[derive(Clone)]
struct RoutedTransport {
    gate: PhaseGate,
    events: Arc<Mutex<Vec<String>>>,
    routes: Arc<Mutex<HashMap<String, VecDeque<MockOutcome>>>>,
    requests: Arc<Mutex<Vec<TransportRequest>>>,
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
}

impl Transport for RoutedTransport {
    fn send(
        &self,
        req: TransportRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send>> {
        let gate = self.gate.clone();
        let events = self.events.clone();
        let routes = self.routes.clone();
        let requests = self.requests.clone();
        Box::pin(async move {
            let path = req.url.path().to_string();
            events.lock().await.push(format!("transport_send:{path}"));
            requests.lock().await.push(req.clone());
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
            Ok(TransportResponse {
                meta: req.meta,
                url: req.url,
                status: response.status,
                headers: response.headers,
                content_length: response.content_length.or_else(|| {
                    response
                        .chunks
                        .is_none()
                        .then_some(response.body.len() as u64)
                }),
                rate_limit: req.rate_limit,
                body: Box::new(RoutedStaticBody {
                    body: Some(response.body),
                }),
            })
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
            self.events
                .lock()
                .await
                .push(format!("rate_acquire:{label}"));
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
            self.events
                .lock()
                .await
                .push(format!("rate_response:{label}:{}", ctx.status));
            Ok(RateLimitResponseAction::Continue)
        })
    }
}

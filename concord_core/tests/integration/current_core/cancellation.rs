use super::common::{
    GateableBodyTransport, GateableCache, GateableHooks, GateableTransport, MockOutcome,
    MockResponse, ObservationRuntimeHooks, PhaseGate, RecordingCache, SafeRecordingDebugSink,
    TestAuthVars, TestCx, TextEndpoint, assert_events_do_not_contain, assert_still_pending,
    auth_policy, cache_and_rate_limit_policy, cache_policy, client, rate_limit_policy,
};
use bytes::Bytes;
use concord_core::advanced::AuthPlacement;
use concord_core::advanced::{Transport, TransportErrorKind};
use concord_core::advanced::{TransportBody, TransportError, TransportRequest, TransportResponse};
use concord_core::internal::PaginationPlan;
use concord_core::prelude::{ApiClient, ApiClientError, PaginationTermination};
use http::{HeaderMap, StatusCode};
use std::collections::VecDeque;
use std::error::Error;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
use tokio::sync::Mutex;

const RAW_AUTH_SENTINEL_PR79: &str = "RAW_AUTH_SENTINEL_PR79";
const RESPONSE_BODY_SENTINEL_PR79: &str = "RESPONSE_BODY_SENTINEL_PR79";

fn body_sentinels() -> [&'static str; 2] {
    [RAW_AUTH_SENTINEL_PR79, RESPONSE_BODY_SENTINEL_PR79]
}

fn transport_client<T: Transport + Clone>(transport: T) -> ApiClient<TestCx, T> {
    ApiClient::with_transport((), TestAuthVars::default(), transport)
}

fn transport_client_with_auth<T: Transport + Clone>(
    auth: TestAuthVars,
    transport: T,
) -> ApiClient<TestCx, T> {
    ApiClient::with_transport((), auth, transport)
}

fn assert_error_diagnostics_safe(err: &ApiClientError, sentinels: &[&str]) {
    let display = err.to_string();
    let debug = format!("{err:?}");
    let pretty_debug = format!("{err:#?}");
    for sentinel in sentinels {
        assert!(!display.contains(sentinel), "display leaked {sentinel}");
        assert!(!debug.contains(sentinel), "debug leaked {sentinel}");
        assert!(
            !pretty_debug.contains(sentinel),
            "pretty debug leaked {sentinel}"
        );
    }

    let mut current: Option<&(dyn Error + 'static)> = Some(err);
    while let Some(source) = current {
        let source_display = source.to_string();
        let source_debug = format!("{source:?}");
        let source_pretty = format!("{source:#?}");
        for sentinel in sentinels {
            assert!(
                !source_display.contains(sentinel),
                "source display leaked {sentinel}"
            );
            assert!(
                !source_debug.contains(sentinel),
                "source debug leaked {sentinel}"
            );
            assert!(
                !source_pretty.contains(sentinel),
                "source pretty debug leaked {sentinel}"
            );
        }
        current = source.source();
    }
}

#[derive(Clone)]
struct TimeoutAfterFirstChunkTransport {
    events: Arc<Mutex<Vec<String>>>,
    read_count: Arc<AtomicUsize>,
}

impl TimeoutAfterFirstChunkTransport {
    fn new(events: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            events,
            read_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn read_count(&self) -> usize {
        self.read_count.load(AtomicOrdering::SeqCst)
    }
}

struct TimeoutAfterFirstChunkBody {
    chunks: VecDeque<Bytes>,
    read_count: Arc<AtomicUsize>,
}

impl TransportBody for TimeoutAfterFirstChunkBody {
    fn next_chunk<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Bytes>, TransportError>> + Send + 'a>> {
        Box::pin(async move {
            let call = self.read_count.fetch_add(1, AtomicOrdering::SeqCst);
            if call == 0 {
                Ok(self.chunks.pop_front())
            } else {
                Err(TransportError::with_kind(
                    TransportErrorKind::Timeout,
                    std::io::Error::other("timeout while reading response body"),
                ))
            }
        })
    }
}

impl Transport for TimeoutAfterFirstChunkTransport {
    fn send(
        &self,
        req: TransportRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send>> {
        let events = self.events.clone();
        let read_count = self.read_count.clone();
        Box::pin(async move {
            events.lock().await.push("timeout_body_send".to_string());
            Ok(TransportResponse {
                meta: req.meta,
                url: req.url,
                status: StatusCode::OK,
                headers: HeaderMap::new(),
                content_length: None,
                rate_limit: req.rate_limit,
                body: Box::new(TimeoutAfterFirstChunkBody {
                    chunks: vec![
                        Bytes::from_static(RESPONSE_BODY_SENTINEL_PR79.as_bytes()),
                        Bytes::from_static(b"second"),
                    ]
                    .into(),
                    read_count,
                }),
            })
        })
    }
}

#[tokio::test]
async fn cancel_during_cache_lookup_does_not_reach_rate_limit_or_transport() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let gate = PhaseGate::new();
    let cache_probe = super::common::DropProbe::new("cache_lookup", events.clone());
    let rate_probe = super::common::DropProbe::new("rate_acquire", events.clone());
    let transport_probe = super::common::DropProbe::new("transport_send", events.clone());
    let cache = Arc::new(
        GateableCache::miss(gate.clone(), events.clone()).with_drop_probe(cache_probe.clone()),
    );
    let rate_limiter = Arc::new(
        super::common::CountingRateLimiter::new(events.clone())
            .with_gate(gate.clone())
            .with_drop_probe(rate_probe.clone()),
    );
    let transport = GateableTransport::new(
        gate.clone(),
        events.clone(),
        vec![
            MockResponse::text(StatusCode::OK, "one"),
            MockResponse::text(StatusCode::OK, "two"),
        ],
    )
    .with_drop_probe(transport_probe.clone());
    let mut client = transport_client(transport.clone());
    client.configure(|cfg| {
        cfg.cache_store(cache.clone());
        cfg.rate_limiter(rate_limiter.clone());
    });

    let endpoint = TextEndpoint {
        policy: cache_and_rate_limit_policy(),
        ..Default::default()
    };
    gate.block("cache_before").await;
    let task = tokio::spawn({
        let client = client.clone();
        async move { client.request(endpoint).execute_decoded().await }
    });

    gate.wait_for("cache_before", 1).await;
    task.abort();
    let join = task.await;
    assert!(join.is_err());
    cache_probe.wait_for(1).await;
    assert_eq!(cache_probe.count(), 1);
    assert_eq!(rate_probe.count(), 0);
    assert_eq!(transport_probe.count(), 0);
    gate.release_one("cache_before").await;

    let mut second = tokio::spawn({
        let client = client.clone();
        async move {
            client
                .request(TextEndpoint {
                    policy: cache_and_rate_limit_policy(),
                    ..Default::default()
                })
                .execute_decoded()
                .await
        }
    });
    gate.wait_for("cache_before", 2).await;
    assert_still_pending(
        "cancelled cache lookup must not leave a reusable permit behind",
        async {
            (&mut second)
                .await
                .expect("task should join")
                .expect("second request should succeed");
        },
    )
    .await;
    gate.release_one("cache_before").await;
    let decoded = second
        .await
        .expect("second task should join")
        .expect("second request should succeed");
    assert_eq!(decoded.value(), "one");

    assert_eq!(
        rate_limiter
            .acquire_started
            .load(std::sync::atomic::Ordering::SeqCst),
        1
    );
    assert_eq!(transport.sent_count().await, 1);
    assert_events_do_not_contain(&events, &body_sentinels()).await;
}

#[tokio::test]
async fn cancel_during_rate_limit_acquire_does_not_send_transport() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let gate = PhaseGate::new();
    let rate_probe = super::common::DropProbe::new("rate_acquire", events.clone());
    let rate_limiter = Arc::new(
        super::common::CountingRateLimiter::new(events.clone())
            .with_gate(gate.clone())
            .with_drop_probe(rate_probe.clone()),
    );
    let transport = GateableTransport::new(
        gate.clone(),
        events.clone(),
        vec![
            MockResponse::text(StatusCode::OK, "ok-1"),
            MockResponse::text(StatusCode::OK, "ok-2"),
        ],
    );
    let mut client = transport_client(transport.clone());
    client.configure(|cfg| {
        cfg.rate_limiter(rate_limiter.clone());
    });

    let endpoint = TextEndpoint {
        policy: rate_limit_policy(),
        ..Default::default()
    };
    gate.block("rate_acquire").await;
    let task = tokio::spawn({
        let client = client.clone();
        async move { client.request(endpoint).execute_decoded().await }
    });

    gate.wait_for("rate_acquire", 1).await;
    task.abort();
    let join = task.await;
    assert!(join.is_err());
    assert_eq!(rate_limiter.acquire_started.load(AtomicOrdering::SeqCst), 1);
    rate_probe.wait_for(1).await;
    assert_eq!(rate_probe.count(), 1);
    gate.release_one("rate_acquire").await;
    let second = tokio::spawn({
        let client = client.clone();
        async move {
            client
                .request(TextEndpoint {
                    policy: rate_limit_policy(),
                    ..Default::default()
                })
                .execute_decoded()
                .await
        }
    });
    gate.wait_for("rate_acquire", 2).await;
    gate.release_one("rate_acquire").await;

    let second = second
        .await
        .expect("second task should join")
        .expect("second request should complete");
    assert_eq!(second.value(), "ok-1");
    assert_eq!(rate_limiter.acquire_started.load(AtomicOrdering::SeqCst), 2);
    assert_eq!(transport.sent_count().await, 1);
    assert_events_do_not_contain(&events, &body_sentinels()).await;
}

#[tokio::test]
async fn cancel_during_pre_send_hook_does_not_send_transport() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let gate = PhaseGate::new();
    let hook_probe = super::common::DropProbe::new("hook_pre_send", events.clone());
    let hooks = Arc::new(
        GateableHooks::new(gate.clone(), events.clone()).with_drop_probe(hook_probe.clone()),
    );
    let transport = GateableTransport::new(
        gate.clone(),
        events.clone(),
        vec![
            MockResponse::text(StatusCode::OK, "ok-1"),
            MockResponse::text(StatusCode::OK, "ok-2"),
        ],
    );
    let mut client = transport_client(transport.clone());
    client.set_runtime_hooks(hooks.clone());
    let endpoint = TextEndpoint {
        policy: cache_and_rate_limit_policy(),
        ..Default::default()
    };

    gate.block("hook_pre_send").await;
    let task = tokio::spawn({
        let client = client.clone();
        async move { client.request(endpoint).execute_decoded().await }
    });

    gate.wait_for("hook_pre_send", 1).await;
    task.abort();
    let join = task.await;
    assert!(join.is_err());
    hook_probe.wait_for(1).await;
    assert_eq!(hook_probe.count(), 1);
    gate.release_one("hook_pre_send").await;
    let second = tokio::spawn({
        let client = client.clone();
        async move {
            client
                .request(TextEndpoint {
                    policy: cache_and_rate_limit_policy(),
                    ..Default::default()
                })
                .execute_decoded()
                .await
        }
    });
    gate.wait_for("hook_pre_send", 2).await;
    gate.release_one("hook_pre_send").await;
    let second = second
        .await
        .expect("second task should join")
        .expect("second request should complete");
    assert_eq!(second.value(), "ok-1");
    assert_eq!(transport.sent_count().await, 1);
    assert_events_do_not_contain(&events, &body_sentinels()).await;
}

#[tokio::test]
async fn cancel_during_transport_send_does_not_classify_response_or_cache() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let gate = PhaseGate::new();
    let transport_probe = super::common::DropProbe::new("transport_send", events.clone());
    let cache_probe = super::common::DropProbe::new("cache_before", events.clone());
    let cache = Arc::new(
        GateableCache::miss(gate.clone(), events.clone()).with_drop_probe(cache_probe.clone()),
    );
    let rate_limiter = Arc::new(super::common::CountingRateLimiter::new(events.clone()));
    let hooks = Arc::new(ObservationRuntimeHooks::new(events.clone()));
    let first_body_reads = Arc::new(AtomicUsize::new(0));
    let second_body_reads = Arc::new(AtomicUsize::new(0));
    let transport = GateableTransport::new(
        gate.clone(),
        events.clone(),
        vec![
            MockResponse::text(StatusCode::OK, "ok-1").with_read_count(first_body_reads.clone()),
            MockResponse::text(StatusCode::OK, "ok-2").with_read_count(second_body_reads.clone()),
        ],
    )
    .with_drop_probe(transport_probe.clone());
    let mut client = transport_client(transport.clone());
    client.configure(|cfg| {
        cfg.cache_store(cache.clone());
        cfg.rate_limiter(rate_limiter.clone());
    });
    client.set_runtime_hooks(hooks.clone());
    let endpoint = TextEndpoint {
        policy: cache_policy(),
        ..Default::default()
    };

    gate.block("transport_send").await;
    let task = tokio::spawn({
        let client = client.clone();
        async move { client.request(endpoint).execute_decoded().await }
    });

    gate.wait_for("transport_send", 1).await;
    task.abort();
    let join = task.await;
    assert!(join.is_err());
    transport_probe.wait_for(1).await;
    assert_eq!(transport_probe.count(), 1);
    cache_probe.wait_for(1).await;
    assert!(cache_probe.count() >= 1);
    assert_eq!(cache.after_response_count(), 0);
    assert_eq!(
        rate_limiter.response_observed.load(AtomicOrdering::SeqCst),
        0
    );
    assert_eq!(first_body_reads.load(AtomicOrdering::SeqCst), 0);
    assert!(
        !events
            .lock()
            .await
            .iter()
            .any(|event| event.starts_with("hook_post_response"))
    );
    let followup_gate = PhaseGate::new();
    let followup_first_body_reads = Arc::new(AtomicUsize::new(0));
    let followup_second_body_reads = Arc::new(AtomicUsize::new(0));
    let followup_transport = GateableTransport::new(
        followup_gate.clone(),
        events.clone(),
        vec![
            MockResponse::text(StatusCode::OK, "ok-1")
                .with_read_count(followup_first_body_reads.clone()),
            MockResponse::text(StatusCode::OK, "ok-2")
                .with_read_count(followup_second_body_reads.clone()),
        ],
    );
    let mut followup_client = transport_client(followup_transport.clone());
    followup_client.configure(|cfg| {
        cfg.cache_store(cache.clone());
        cfg.rate_limiter(rate_limiter.clone());
    });
    followup_client.set_runtime_hooks(hooks.clone());
    let second = tokio::spawn({
        let client = followup_client.clone();
        async move {
            client
                .request(TextEndpoint {
                    policy: cache_policy(),
                    ..Default::default()
                })
                .execute_decoded()
                .await
        }
    });
    followup_gate.wait_for("transport_send", 1).await;
    followup_gate.release_one("transport_send").await;
    let second = second
        .await
        .expect("second task should join")
        .expect("second request should complete");
    assert_eq!(second.value(), "ok-1");
    assert_eq!(transport.sent_count().await, 1);
    assert_eq!(followup_transport.sent_count().await, 1);
    assert_eq!(
        rate_limiter.response_observed.load(AtomicOrdering::SeqCst),
        1
    );
    assert_eq!(first_body_reads.load(AtomicOrdering::SeqCst), 0);
    assert_eq!(second_body_reads.load(AtomicOrdering::SeqCst), 0);
    assert_eq!(followup_first_body_reads.load(AtomicOrdering::SeqCst), 1);
    assert_eq!(followup_second_body_reads.load(AtomicOrdering::SeqCst), 0);
    assert_events_do_not_contain(&events, &body_sentinels()).await;
}

#[tokio::test]
async fn cancel_during_body_read_does_not_decode_map_or_cache_admit() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let gate = PhaseGate::new();
    let body_probe = super::common::DropProbe::new("body", events.clone());
    let cache = Arc::new(GateableCache::miss(gate.clone(), events.clone()));
    let transport = GateableBodyTransport::new(
        gate.clone(),
        events.clone(),
        vec![
            Bytes::from_static(b"first"),
            Bytes::from_static(RESPONSE_BODY_SENTINEL_PR79.as_bytes()),
        ],
    )
    .with_drop_probe(body_probe.clone());
    transport
        .push_body(vec![Bytes::from_static(b"second")])
        .await;
    let mut client = transport_client(transport.clone());
    client.configure(|cfg| {
        cfg.cache_store(cache.clone());
        cfg.rate_limiter(Arc::new(super::common::CountingRateLimiter::new(
            events.clone(),
        )));
    });

    let endpoint = TextEndpoint {
        policy: cache_and_rate_limit_policy(),
        ..Default::default()
    };
    gate.block("body_chunk").await;
    let task = tokio::spawn({
        let client = client.clone();
        async move { client.request(endpoint).execute_decoded().await }
    });

    gate.wait_for("body_chunk", 1).await;
    gate.release_one("body_chunk").await;
    gate.wait_for("body_chunk", 2).await;
    assert_eq!(transport.read_count(), 1);
    task.abort();
    let join = task.await;
    assert!(join.is_err());
    body_probe.wait_for(1).await;
    assert_eq!(body_probe.count(), 1);
    assert_eq!(transport.read_count(), 1);
    assert_eq!(cache.after_response_count(), 0);
    let second = tokio::spawn({
        let client = client.clone();
        async move {
            client
                .request(TextEndpoint {
                    policy: cache_and_rate_limit_policy(),
                    ..Default::default()
                })
                .execute_decoded()
                .await
        }
    });
    gate.wait_for("body_chunk", 3).await;
    gate.release_one("body_chunk").await;
    let second = second
        .await
        .expect("second task should join")
        .expect("second request should succeed");
    assert!(second.value().starts_with("second"));
    assert_eq!(transport.read_count(), 2);
}

#[tokio::test]
async fn cancel_during_cache_admission_does_not_return_success_twice_or_poison_cache() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let gate = PhaseGate::new();
    let cache_probe = super::common::DropProbe::new("cache_after_response", events.clone());
    let cache = Arc::new(
        GateableCache::miss(gate.clone(), events.clone()).with_drop_probe(cache_probe.clone()),
    );
    let transport = GateableTransport::new(
        gate.clone(),
        events.clone(),
        vec![
            MockResponse::text(StatusCode::OK, "ok-1"),
            MockResponse::text(StatusCode::OK, "ok-2"),
        ],
    );
    let mut client = transport_client(transport.clone());
    client.configure(|cfg| {
        cfg.cache_store(cache.clone());
        cfg.rate_limiter(Arc::new(super::common::CountingRateLimiter::new(
            events.clone(),
        )));
    });
    let endpoint = TextEndpoint {
        policy: cache_and_rate_limit_policy(),
        ..Default::default()
    };

    gate.block("cache_after_response").await;
    let task = tokio::spawn({
        let client = client.clone();
        async move { client.request(endpoint).execute_decoded().await }
    });

    gate.wait_for("cache_after_response", 1).await;
    task.abort();
    let join = task.await;
    assert!(join.is_err());
    cache_probe.wait_for(1).await;
    assert!(cache_probe.count() >= 1);
    assert!(cache.after_response_count() >= 1);
    let followup_gate = PhaseGate::new();
    let followup_cache = Arc::new(GateableCache::miss(followup_gate.clone(), events.clone()));
    let followup_transport = GateableTransport::new(
        followup_gate.clone(),
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "ok-2")],
    );
    let mut followup_client = transport_client(followup_transport.clone());
    followup_client.configure(|cfg| {
        cfg.cache_store(followup_cache.clone());
        cfg.rate_limiter(Arc::new(super::common::CountingRateLimiter::new(
            events.clone(),
        )));
    });
    let second = tokio::spawn({
        let client = followup_client.clone();
        async move {
            client
                .request(TextEndpoint {
                    policy: cache_and_rate_limit_policy(),
                    ..Default::default()
                })
                .execute_decoded()
                .await
        }
    });
    followup_gate.wait_for("cache_after_response", 1).await;
    followup_gate.release_one("cache_after_response").await;
    let second = second
        .await
        .expect("second task should join")
        .expect("second request should complete");
    assert_eq!(second.value(), "ok-2");
    assert_eq!(cache.after_response_count(), 1);
    assert_events_do_not_contain(&events, &body_sentinels()).await;
}

#[tokio::test]
async fn cancel_during_stale_fallback_does_not_decode_or_advance() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let gate = PhaseGate::new();
    let cache_probe = super::common::DropProbe::new("cache_after_error", events.clone());
    let cached = super::common::built_response("Text", StatusCode::OK, "stale");
    let cache = Arc::new(
        GateableCache::stale_fallback(gate.clone(), events.clone(), cached.clone())
            .with_drop_probe(cache_probe.clone()),
    );
    let transport = GateableTransport::new(
        gate.clone(),
        events.clone(),
        vec![
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, "fail-1"),
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, "fail-2"),
        ],
    );
    let mut client = transport_client(transport.clone());
    client.configure(|cfg| {
        cfg.cache_store(cache.clone());
    });
    let endpoint = TextEndpoint {
        policy: cache_policy(),
        ..Default::default()
    };

    gate.block("cache_after_error").await;
    let task = tokio::spawn({
        let client = client.clone();
        async move { client.request(endpoint).execute_decoded().await }
    });

    gate.wait_for("cache_after_error", 1).await;
    task.abort();
    let join = task.await;
    assert!(join.is_err());
    cache_probe.wait_for(1).await;
    assert!(cache_probe.count() >= 1);
    assert!(cache.after_error_count() >= 1);
    let followup_gate = PhaseGate::new();
    let followup_cache = Arc::new(GateableCache::stale_fallback(
        followup_gate.clone(),
        events.clone(),
        cached.clone(),
    ));
    let followup_transport = GateableTransport::new(
        followup_gate.clone(),
        events.clone(),
        vec![MockResponse::text(
            StatusCode::INTERNAL_SERVER_ERROR,
            "fail-2",
        )],
    );
    let mut followup_client = transport_client(followup_transport.clone());
    followup_client.configure(|cfg| {
        cfg.cache_store(followup_cache.clone());
    });
    let second = tokio::spawn({
        let client = followup_client.clone();
        async move {
            client
                .request(TextEndpoint {
                    policy: cache_policy(),
                    ..Default::default()
                })
                .execute_decoded()
                .await
        }
    });
    followup_gate.wait_for("cache_after_error", 1).await;
    followup_gate.release_one("cache_after_error").await;
    let second = second
        .await
        .expect("second task should join")
        .expect("stale fallback should still be available");
    assert_eq!(second.value(), "stale");
    assert_eq!(transport.sent_count().await, 1);
    assert_eq!(followup_transport.sent_count().await, 1);
    assert_events_do_not_contain(&events, &body_sentinels()).await;
}

#[tokio::test]
async fn cancel_pagination_between_pages_does_not_request_next_page() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let gate = PhaseGate::new();
    let transport = GateableTransport::new(
        PhaseGate::new(),
        events.clone(),
        vec![
            MockResponse::text(StatusCode::OK, "a,b|next=page-1"),
            MockResponse::text(StatusCode::OK, "c,d|next=page-2"),
            MockResponse::text(StatusCode::OK, "e,f|next=page-3"),
            MockResponse::text(StatusCode::OK, "g,h"),
        ],
    );
    let client = transport_client(transport.clone());
    let endpoint = super::common::ItemsEndpoint {
        policy: Default::default(),
        pagination: PaginationPlan::OffsetLimit {
            offset_key: "offset".to_string(),
            limit_key: "limit".to_string(),
            offset: 0,
            limit: 2,
        },
    };

    gate.block("page_callback").await;
    let first_endpoint = endpoint.clone();
    let task = tokio::spawn({
        let client = client.clone();
        let gate = gate.clone();
        async move {
            client
                .request(first_endpoint)
                .paginate(PaginationTermination::hard_page_cap(10))
                .for_each_page(move |page| {
                    let gate = gate.clone();
                    async move {
                        gate.enter("page_callback").await;
                        assert_eq!(page.value, vec!["a".to_string(), "b".to_string()]);
                        Ok(())
                    }
                })
                .await
        }
    });

    gate.wait_for("page_callback", 1).await;
    task.abort();
    let _ = task.await;
    gate.release_one("page_callback").await;
    assert_eq!(transport.sent_count().await, 1);

    client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(10))
        .for_each_page(|_page| async { Ok(()) })
        .await
        .expect("later pagination run should succeed");
    assert_events_do_not_contain(&events, &body_sentinels()).await;
}

#[tokio::test]
async fn timeout_during_body_read_does_not_cache_or_decode() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let cache = Arc::new(GateableCache::miss(PhaseGate::new(), events.clone()));
    let raw_auth = TestAuthVars {
        token: Some(RAW_AUTH_SENTINEL_PR79.to_string()),
        identity: "timeout-body",
    };
    let transport = TimeoutAfterFirstChunkTransport::new(events.clone());
    let mut client = transport_client_with_auth(raw_auth, transport.clone());
    client.configure(|cfg| {
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

    let err = client
        .request(endpoint)
        .execute_decoded()
        .await
        .expect_err("timeout during body read should surface as a transport error");

    assert!(matches!(err, ApiClientError::Transport { .. }));
    assert!(matches!(
        err.source().and_then(|source| source.downcast_ref::<concord_core::transport::TransportError>()),
        Some(source) if source.kind() == TransportErrorKind::Timeout
    ));
    assert_eq!(transport.read_count(), 2);
    assert_eq!(cache.after_response_count(), 0);
    assert_error_diagnostics_safe(&err, &[RAW_AUTH_SENTINEL_PR79, RESPONSE_BODY_SENTINEL_PR79]);
}

#[tokio::test]
async fn transport_timeout_error_is_typed_and_safe() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let cache = Arc::new(RecordingCache::miss(events.clone()));
    let rate_limiter = Arc::new(super::common::CountingRateLimiter::new(events.clone()));
    let transport = super::common::MockTransport::with_outcomes(
        events.clone(),
        vec![MockOutcome::TransportError(TransportErrorKind::Timeout)],
    );
    let raw_auth = TestAuthVars {
        token: Some(RAW_AUTH_SENTINEL_PR79.to_string()),
        identity: "transport-timeout",
    };
    let mut client = transport_client_with_auth(raw_auth, transport.clone());
    client.configure(|cfg| {
        cfg.cache_store(cache.clone());
        cfg.rate_limiter(rate_limiter.clone());
    });
    client.set_runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())));
    client.set_debug_sink(Arc::new(SafeRecordingDebugSink::new(events.clone())));

    let endpoint = TextEndpoint {
        policy: {
            let mut policy = cache_and_rate_limit_policy();
            policy.auth = auth_policy(AuthPlacement::Bearer).auth;
            policy
        },
        ..Default::default()
    };

    let err = client
        .request(endpoint)
        .execute_decoded()
        .await
        .expect_err("transport timeout should surface as a transport error");

    assert!(matches!(err, ApiClientError::Transport { .. }));
    assert!(matches!(
        err.source().and_then(|source| source.downcast_ref::<concord_core::transport::TransportError>()),
        Some(source) if source.kind() == TransportErrorKind::Timeout
    ));
    assert_eq!(transport.sent_count().await, 1);
    assert_eq!(
        rate_limiter.response_observed.load(AtomicOrdering::SeqCst),
        0
    );
    assert_eq!(*cache.after_response_count.lock().await, 0);
    assert_error_diagnostics_safe(&err, &[RAW_AUTH_SENTINEL_PR79]);
    assert_events_do_not_contain(&events, &body_sentinels()).await;
}

#[tokio::test]
async fn execute_raw_cancellation_matches_raw_contract() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let gate = PhaseGate::new();
    let transport_probe = super::common::DropProbe::new("transport_send", events.clone());
    let cache_probe = super::common::DropProbe::new("cache_before", events.clone());
    let cache = Arc::new(
        GateableCache::miss(gate.clone(), events.clone()).with_drop_probe(cache_probe.clone()),
    );
    let transport = GateableTransport::new(
        gate.clone(),
        events.clone(),
        vec![
            MockResponse::text(StatusCode::OK, "raw-1"),
            MockResponse::text(StatusCode::OK, "raw-2"),
        ],
    )
    .with_drop_probe(transport_probe.clone());
    let mut client = transport_client(transport.clone());
    client.configure(|cfg| {
        cfg.cache_store(cache.clone());
        cfg.rate_limiter(Arc::new(super::common::CountingRateLimiter::new(
            events.clone(),
        )));
    });

    let endpoint = TextEndpoint {
        policy: cache_and_rate_limit_policy(),
        ..Default::default()
    };
    gate.block("transport_send").await;
    let task = tokio::spawn({
        let client = client.clone();
        async move { client.request(endpoint).execute_raw().await }
    });

    gate.wait_for("transport_send", 1).await;
    task.abort();
    let join = task.await;
    assert!(join.is_err());
    transport_probe.wait_for(1).await;
    assert_eq!(transport_probe.count(), 1);
    assert_eq!(cache_probe.count(), 0);
    assert_eq!(cache.after_response_count(), 0);
    gate.release_one("transport_send").await;
    let raw = tokio::spawn({
        let client = client.clone();
        async move {
            client
                .request(TextEndpoint {
                    policy: cache_and_rate_limit_policy(),
                    ..Default::default()
                })
                .execute_raw()
                .await
        }
    });
    gate.wait_for("transport_send", 2).await;
    gate.release_one("transport_send").await;
    assert_eq!(cache_probe.count(), 0);
    assert_eq!(cache.after_response_count(), 0);
    let raw = raw
        .await
        .expect("later raw task should join")
        .expect("later raw request should complete");
    assert_eq!(raw.status, StatusCode::OK);
    assert_eq!(transport.sent_count().await, 2);
    assert!(
        !events
            .lock()
            .await
            .iter()
            .any(|event| event == "cache_before_started")
    );
    assert!(
        !events
            .lock()
            .await
            .iter()
            .any(|event| event == "cache_after_response")
    );
    assert_events_do_not_contain(&events, &body_sentinels()).await;
}

#[tokio::test]
async fn execute_raw_cancel_during_body_read_does_not_decode_map_or_cache() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let gate = PhaseGate::new();
    let body_probe = super::common::DropProbe::new("raw_body", events.clone());
    let cache = Arc::new(RecordingCache::miss(events.clone()));
    let rate_limiter = Arc::new(super::common::CountingRateLimiter::new(events.clone()));
    let transport = GateableBodyTransport::new(
        gate.clone(),
        events.clone(),
        vec![
            Bytes::from_static(b"raw-first"),
            Bytes::from_static(RESPONSE_BODY_SENTINEL_PR79.as_bytes()),
        ],
    )
    .with_drop_probe(body_probe.clone());
    transport
        .push_body(vec![Bytes::from_static(b"raw-second")])
        .await;
    let mut client = transport_client(transport.clone());
    client.configure(|cfg| {
        cfg.cache_store(cache.clone());
        cfg.rate_limiter(rate_limiter.clone());
    });
    client.set_runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())));
    client.set_debug_sink(Arc::new(SafeRecordingDebugSink::new(events.clone())));

    let endpoint = TextEndpoint {
        policy: cache_and_rate_limit_policy(),
        ..Default::default()
    };

    gate.block("body_chunk").await;
    let task = tokio::spawn({
        let client = client.clone();
        async move { client.request(endpoint).execute_raw().await }
    });

    gate.wait_for("body_chunk", 1).await;
    gate.release_one("body_chunk").await;
    gate.wait_for("body_chunk", 2).await;
    assert_eq!(transport.read_count(), 1);
    task.abort();
    let join = task.await;
    assert!(join.is_err());
    body_probe.wait_for(1).await;
    assert_eq!(body_probe.count(), 1);
    assert_eq!(transport.read_count(), 1);
    assert_eq!(*cache.after_response_count.lock().await, 0);
    assert_eq!(*cache.after_error_count.lock().await, 0);
    assert_eq!(
        rate_limiter.response_observed.load(AtomicOrdering::SeqCst),
        1
    );

    let raw = tokio::spawn({
        let client = client.clone();
        async move {
            client
                .request(TextEndpoint {
                    policy: cache_and_rate_limit_policy(),
                    ..Default::default()
                })
                .execute_raw()
                .await
        }
    });
    gate.wait_for("body_chunk", 3).await;
    gate.release_one("body_chunk").await;
    let raw = raw
        .await
        .expect("later raw task should join")
        .expect("later raw request should complete");
    assert_eq!(raw.status, StatusCode::OK);
    assert_eq!(raw.body, Bytes::from_static(b"raw-second"));
    assert_eq!(transport.read_count(), 2);
    assert_eq!(*cache.after_response_count.lock().await, 0);
    assert_eq!(*cache.after_error_count.lock().await, 0);
    assert!(
        !events
            .lock()
            .await
            .iter()
            .any(|event| event.starts_with("cache_before:"))
    );
    assert_events_do_not_contain(&events, &body_sentinels()).await;
}

#[tokio::test]
async fn execute_raw_cancellation_during_rate_limit_acquire_does_not_send_transport() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let gate = PhaseGate::new();
    let rate_probe = super::common::DropProbe::new("rate_acquire", events.clone());
    let cache_probe = super::common::DropProbe::new("cache_before", events.clone());
    let cache = Arc::new(
        GateableCache::miss(gate.clone(), events.clone()).with_drop_probe(cache_probe.clone()),
    );
    let rate_limiter = Arc::new(
        super::common::CountingRateLimiter::new(events.clone())
            .with_gate(gate.clone())
            .with_drop_probe(rate_probe.clone()),
    );
    let transport = GateableTransport::new(
        gate.clone(),
        events.clone(),
        vec![
            MockResponse::text(StatusCode::OK, "raw-1"),
            MockResponse::text(StatusCode::OK, "raw-2"),
        ],
    );
    let mut client = transport_client(transport.clone());
    client.configure(|cfg| {
        cfg.cache_store(cache.clone());
        cfg.rate_limiter(rate_limiter.clone());
    });

    let endpoint = TextEndpoint {
        policy: cache_and_rate_limit_policy(),
        ..Default::default()
    };
    gate.block("rate_acquire").await;
    let task = tokio::spawn({
        let client = client.clone();
        async move { client.request(endpoint).execute_raw().await }
    });

    gate.wait_for("rate_acquire", 1).await;
    task.abort();
    let join = task.await;
    assert!(join.is_err());
    rate_probe.wait_for(1).await;
    assert_eq!(rate_probe.count(), 1);
    assert_eq!(rate_limiter.acquire_started.load(AtomicOrdering::SeqCst), 1);
    assert_eq!(cache_probe.count(), 0);
    assert_eq!(transport.sent_count().await, 0);
    gate.release_one("rate_acquire").await;

    let raw = tokio::spawn({
        let client = client.clone();
        async move {
            client
                .request(TextEndpoint {
                    policy: cache_and_rate_limit_policy(),
                    ..Default::default()
                })
                .execute_raw()
                .await
        }
    });
    gate.wait_for("rate_acquire", 2).await;
    gate.release_one("rate_acquire").await;
    let raw = raw
        .await
        .expect("later raw task should join")
        .expect("later raw request should complete");
    assert_eq!(raw.status, StatusCode::OK);
    assert_eq!(rate_limiter.acquire_started.load(AtomicOrdering::SeqCst), 2);
    assert_eq!(transport.sent_count().await, 1);
    assert_eq!(cache_probe.count(), 0);
    assert_eq!(cache.after_response_count(), 0);
}

#[tokio::test]
async fn cancellation_observer_surfaces_are_body_auth_free() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let gate = PhaseGate::new();
    let raw_auth = TestAuthVars {
        token: Some(RAW_AUTH_SENTINEL_PR79.to_string()),
        identity: "observer",
    };
    let cache = Arc::new(
        GateableCache::miss(gate.clone(), events.clone()).with_drop_probe(
            super::common::DropProbe::new("cache_before", events.clone()),
        ),
    );
    let rate_limiter = Arc::new(
        super::common::CountingRateLimiter::new(events.clone())
            .with_gate(gate.clone())
            .with_drop_probe(super::common::DropProbe::new("rate", events.clone())),
    );
    let transport = GateableTransport::new(
        gate.clone(),
        events.clone(),
        vec![
            MockResponse::text(StatusCode::OK, RESPONSE_BODY_SENTINEL_PR79),
            MockResponse::text(StatusCode::OK, "ok-2"),
        ],
    )
    .with_drop_probe(super::common::DropProbe::new("transport", events.clone()));
    let hooks = Arc::new(
        GateableHooks::new(gate.clone(), events.clone())
            .with_drop_probe(super::common::DropProbe::new("hook", events.clone())),
    );
    let mut client = transport_client_with_auth(raw_auth, transport.clone());
    client.configure(|cfg| {
        cfg.cache_store(cache.clone());
        cfg.rate_limiter(rate_limiter.clone());
    });
    client.set_runtime_hooks(hooks.clone());

    let mut policy = cache_and_rate_limit_policy();
    policy.auth = auth_policy(AuthPlacement::Bearer).auth;
    gate.block("cache_after_response").await;
    let task = tokio::spawn({
        let client = client.clone();
        async move {
            client
                .request(TextEndpoint {
                    policy,
                    ..Default::default()
                })
                .execute_decoded()
                .await
        }
    });
    gate.wait_for("cache_after_response", 1).await;
    task.abort();
    let join = task.await;
    assert!(join.is_err());
    gate.release_one("cache_after_response").await;
    assert_events_do_not_contain(&events, &body_sentinels()).await;
}

#[tokio::test]
async fn transport_timeout_metadata_reaches_transport_and_is_request_scoped()
-> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = super::common::MockTransport::with_outcomes(
        events.clone(),
        vec![
            MockOutcome::Response(MockResponse::text(StatusCode::OK, "one")),
            MockOutcome::Response(MockResponse::text(StatusCode::OK, "two")),
        ],
    );
    let client = client(TestAuthVars::default(), transport.clone());
    let endpoint = TextEndpoint {
        policy: {
            let mut policy = cache_and_rate_limit_policy();
            policy.timeout = Some(std::time::Duration::from_secs(5));
            policy
        },
        ..Default::default()
    };
    client
        .request(endpoint.clone())
        .timeout(std::time::Duration::from_secs(2))
        .execute_decoded()
        .await?;
    client.request(endpoint).execute_decoded().await?;
    let requests = transport.requests().await;
    assert_eq!(requests[0].timeout, Some(std::time::Duration::from_secs(2)));
    assert_eq!(requests[1].timeout, Some(std::time::Duration::from_secs(5)));
    Ok(())
}

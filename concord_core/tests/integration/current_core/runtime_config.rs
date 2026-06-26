use super::common::{
    GateTransport, ItemsEndpoint, MockResponse, MockTransport, ObservationRateLimiter,
    RecordingCache, RecordingRateLimiter, TestAuthVars, TestCx, TextEndpoint, auth_policy,
    cache_policy, client,
};
use bytes::Bytes;
use concord_core::advanced::{
    BuiltRequest, BuiltResponse, CacheAfter, CacheBefore, CacheFuture, CacheRevalidation,
    CacheStore, DebugSink, RateLimitBucketUse, RateLimitContext, RateLimitFuture, RateLimitKey,
    RateLimitKeyPart, RateLimitPermit, RateLimitPlan, RateLimitResponseAction,
    RateLimitResponseContext, RateLimitWindow, RateLimiter, RuntimeHooks,
};
use concord_core::internal::{CacheSetting, PaginationPlan, ResolvedPolicy};
use concord_core::prelude::{ApiClient, ApiClientError, DebugLevel, PaginationTermination};
use http::{HeaderMap, Method, StatusCode};
use std::num::NonZeroU32;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::sync::Mutex;

const REQUEST_BODY_SENTINEL: &str = "RAW_REQUEST_BODY_SENTINEL_PR76";
const RESPONSE_BODY_SENTINEL: &str = "RAW_RESPONSE_BODY_SENTINEL_PR76";
const RAW_AUTH_SENTINEL: &str = "RAW_AUTH_SENTINEL_PR76";

#[tokio::test]
async fn client_config_applies_to_requests() {
    let read_count = Arc::new(AtomicUsize::new(0));
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::OK, Bytes::from_static(b"abcde"))
                .with_content_length(Some(5))
                .with_read_count(read_count.clone()),
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
        .expect_err("client body limit should reject 5-byte response");

    assert_response_too_large(&err);
    assert_eq!(body_reads(&read_count), 0);
    assert_eq!(transport_events(&events).await, vec!["transport"]);
}

#[tokio::test]
async fn per_request_debug_override_wins_and_does_not_leak() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::OK, "ok"),
            MockResponse::text(StatusCode::OK, "ok"),
        ],
    );
    let mut client = client(TestAuthVars::default(), transport);
    let debug = Arc::new(RecordingDebugSink::default());
    client.configure(|cfg| {
        cfg.debug_level(DebugLevel::None).debug_sink(debug.clone());
    });

    let first = client
        .request(TextEndpoint::default())
        .debug_level(DebugLevel::VV)
        .execute_decoded()
        .await?;
    assert_eq!(first.value, "ok");
    let after_first = debug.events().await;
    assert!(
        after_first
            .iter()
            .any(|event| event.starts_with("request:"))
    );
    assert!(
        after_first
            .iter()
            .any(|event| event.starts_with("request_headers:"))
    );
    assert!(
        after_first
            .iter()
            .any(|event| event.starts_with("response:"))
    );
    assert!(
        after_first
            .iter()
            .any(|event| event.starts_with("response_headers:"))
    );

    let second = client
        .request(TextEndpoint::default())
        .execute_decoded()
        .await?;
    assert_eq!(second.value, "ok");
    assert_eq!(
        debug.events().await,
        after_first,
        "request debug override must not mutate the client default"
    );
    Ok(())
}

#[tokio::test]
async fn clone_config_isolated_after_execute_starts() {
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
    let mut base_client: ApiClient<TestCx, GateTransport> =
        ApiClient::with_transport((), TestAuthVars::default(), transport.clone());
    base_client.configure(|cfg| {
        cfg.max_response_body_bytes(4);
    });
    let request_client = base_client.clone();

    let in_flight = tokio::spawn(async move {
        request_client
            .request(TextEndpoint::default())
            .execute_decoded()
            .await
    });
    transport.wait_for_sends(1).await;
    base_client.configure(|cfg| {
        cfg.no_response_body_limit();
    });
    transport.release_all();

    let err = in_flight
        .await
        .expect("request task should complete")
        .expect_err("in-flight request should keep the original 4-byte limit");
    assert_response_too_large(&err);

    let later = base_client
        .request(TextEndpoint::default())
        .execute_decoded()
        .await
        .expect("later request should use updated no-limit config");
    assert_eq!(later.value, "abcde");
}

#[tokio::test]
async fn per_request_timeout_override_wins_and_does_not_leak() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::OK, "one"),
            MockResponse::text(StatusCode::OK, "two"),
        ],
    );
    let mut policy = ResolvedPolicy {
        timeout: Some(Duration::from_secs(5)),
        ..ResolvedPolicy::default()
    };
    policy.rate_limit = cache_policy_with_rate_limit(None).rate_limit;
    let endpoint = TextEndpoint {
        policy,
        ..TextEndpoint::default()
    };
    let client = client(TestAuthVars::default(), transport.clone());

    client
        .request(endpoint.clone())
        .timeout(Duration::from_secs(2))
        .execute_decoded()
        .await?;
    client.request(endpoint).execute_decoded().await?;

    let requests = transport.requests().await;
    assert_eq!(requests[0].timeout, Some(Duration::from_secs(2)));
    assert_eq!(requests[1].timeout, Some(Duration::from_secs(5)));
    Ok(())
}

#[tokio::test]
async fn per_request_attempt_override_wins_and_does_not_leak() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "one"),
            MockResponse::text(StatusCode::OK, "two"),
        ],
    );
    let client = client(TestAuthVars::default(), transport.clone());

    client
        .request(TextEndpoint::default())
        .attempt(7)
        .execute_decoded()
        .await?;
    client
        .request(TextEndpoint::default())
        .execute_decoded()
        .await?;

    let requests = transport.requests().await;
    assert_eq!(requests[0].meta.attempt, 7);
    assert_eq!(requests[1].meta.attempt, 0);
    Ok(())
}

#[tokio::test]
async fn per_request_cache_mode_override_wins_and_does_not_leak() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::OK, "one"),
            MockResponse::text(StatusCode::OK, "two"),
        ],
    );
    let cache = Arc::new(RecordingCache::miss(events.clone()));
    let mut client = client(TestAuthVars::default(), transport);
    client.configure(|cfg| {
        cfg.cache_store(cache);
    });
    let endpoint = TextEndpoint {
        policy: cache_policy(),
        ..TextEndpoint::default()
    };

    client
        .request(endpoint.clone())
        .cache_bypass()
        .execute_decoded()
        .await?;
    client.request(endpoint).execute_decoded().await?;

    let events = transport_events(&events).await;
    assert_eq!(events.first().map(String::as_str), Some("transport"));
    assert_eq!(
        events
            .iter()
            .filter(|event| event.starts_with("cache_before"))
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.as_str() == "cache_after_response")
            .count(),
        1
    );
    Ok(())
}

#[tokio::test]
async fn pagination_config_snapshot_stable_across_pages() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::OK, Bytes::from_static(b"a,b"))
                .with_content_length(Some(3)),
            MockResponse::text(StatusCode::OK, Bytes::from_static(b"cc,dd"))
                .with_content_length(Some(5)),
        ],
    );
    let mut base_client = client(TestAuthVars::default(), transport);
    base_client.configure(|cfg| {
        cfg.max_response_body_bytes(4);
    });
    let request_client = base_client.clone();
    let endpoint = ItemsEndpoint {
        policy: ResolvedPolicy::default(),
        pagination: PaginationPlan::OffsetLimit {
            offset_key: "offset".to_string(),
            limit_key: "limit".to_string(),
            offset: 0,
            limit: 2,
        },
    };

    let err = request_client
        .request(endpoint)
        .paginate(PaginationTermination::hard_page_cap(2))
        .for_each_page(|page| {
            assert_eq!(page.value, vec!["a".to_string(), "b".to_string()]);
            base_client.configure(|cfg| {
                cfg.no_response_body_limit();
            });
            async { Ok(()) }
        })
        .await
        .expect_err("second page should keep the pagination-run config snapshot");

    assert_response_too_large(&err);
    assert_eq!(
        transport_events(&events).await,
        vec!["transport", "transport"]
    );
}

#[tokio::test]
async fn execute_raw_uses_same_runtime_safety_config() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let read_count = Arc::new(AtomicUsize::new(0));
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::OK, RESPONSE_BODY_SENTINEL)
                .with_content_length(Some(5))
                .with_read_count(read_count.clone()),
        ],
    );
    let cache = Arc::new(RecordingCache::miss(events.clone()));
    let limiter = Arc::new(ObservationRateLimiter::new(events.clone()));
    let hooks = Arc::new(NamedHooks::new("raw", events.clone()));
    let debug = Arc::new(RecordingDebugSink::default());
    let mut client = client(
        TestAuthVars {
            token: Some(RAW_AUTH_SENTINEL.to_string()),
            identity: "raw",
        },
        transport,
    );
    client.configure(|cfg| {
        cfg.max_response_body_bytes(4)
            .debug_level(DebugLevel::VV)
            .debug_sink(debug.clone())
            .cache_store(cache)
            .rate_limiter(limiter)
            .runtime_hooks(hooks);
    });
    let mut policy = auth_policy(concord_core::advanced::AuthPlacement::Bearer);
    policy.cache = cache_policy_with_rate_limit(None).cache;
    policy.rate_limit = cache_policy_with_rate_limit(None).rate_limit;

    let err = client
        .request(TextEndpoint {
            policy,
            ..TextEndpoint::default()
        })
        .execute_raw()
        .await
        .expect_err("execute_raw should enforce the runtime body limit");

    assert_response_too_large(&err);
    assert_eq!(body_reads(&read_count), 0);
    let event_snapshot = transport_events(&events).await;
    assert!(
        !event_snapshot
            .iter()
            .any(|event| event.starts_with("cache_before"))
    );
    assert!(event_snapshot.contains(&"rate_acquire".to_string()));
    assert!(event_snapshot.contains(&"hook_pre_send:raw".to_string()));
    assert!(event_snapshot.contains(&"hook_post_response:raw:200 OK".to_string()));
    let observed = all_observed_text(&events, &debug).await;
    assert_no_body_or_auth(&observed);
    assert!(!format!("{err:?}").contains(RESPONSE_BODY_SENTINEL));
    assert!(!format!("{err:?}").contains(RAW_AUTH_SENTINEL));
}

#[tokio::test]
async fn disabled_body_limit_behavior_characterized() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, RESPONSE_BODY_SENTINEL)],
    );
    let cache = Arc::new(RecordingCache::miss(events.clone()));
    let limiter = Arc::new(RecordingRateLimiter::new(events.clone()));
    let debug = Arc::new(RecordingDebugSink::default());
    let hooks = Arc::new(NamedHooks::new("disabled", events.clone()));
    let mut client = client(TestAuthVars::default(), transport);
    client.configure(|cfg| {
        cfg.no_response_body_limit()
            .debug_level(DebugLevel::VV)
            .debug_sink(debug.clone())
            .cache_store(cache)
            .rate_limiter(limiter)
            .runtime_hooks(hooks);
    });

    let decoded = client
        .request(TextEndpoint {
            policy: cache_policy_with_rate_limit(Some(4)),
            ..TextEndpoint::default()
        })
        .execute_decoded()
        .await?;

    assert_eq!(decoded.value, RESPONSE_BODY_SENTINEL);
    let joined = all_observed_text(&events, &debug).await;
    assert!(!joined.contains(RESPONSE_BODY_SENTINEL));
    assert!(joined.contains("cache_max_body_skip"));
    Ok(())
}

#[tokio::test]
async fn cache_max_body_does_not_raise_runtime_body_limit() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::OK, Bytes::from_static(b"abcde"))
                .with_content_length(Some(5)),
        ],
    );
    let cache = Arc::new(RecordingCache::miss(events.clone()));
    let mut client = client(TestAuthVars::default(), transport);
    client.configure(|cfg| {
        cfg.max_response_body_bytes(4).cache_store(cache.clone());
    });

    let err = client
        .request(TextEndpoint {
            policy: cache_policy_with_rate_limit(Some(1024)),
            ..TextEndpoint::default()
        })
        .execute_decoded()
        .await
        .expect_err("runtime body limit should win over larger cache max_body");

    assert_response_too_large(&err);
    assert_eq!(*cache.after_response_count.lock().await, 0);
}

#[tokio::test]
async fn runtime_body_limit_does_not_change_cache_max_body() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(
            StatusCode::OK,
            Bytes::from_static(b"abcde"),
        )],
    );
    let cache = Arc::new(RecordingCache::miss(events.clone()));
    let mut client = client(TestAuthVars::default(), transport);
    client.configure(|cfg| {
        cfg.max_response_body_bytes(1024).cache_store(cache.clone());
    });

    let decoded = client
        .request(TextEndpoint {
            policy: cache_policy_with_rate_limit(Some(4)),
            ..TextEndpoint::default()
        })
        .execute_decoded()
        .await?;

    assert_eq!(decoded.value, "abcde");
    assert_eq!(*cache.after_response_count.lock().await, 1);
    assert!(
        transport_events(&events)
            .await
            .contains(&"cache_max_body_skip".to_string())
    );
    Ok(())
}

#[tokio::test]
async fn debug_level_changes_metadata_volume_not_body_or_auth_exposure()
-> Result<(), ApiClientError> {
    let low = run_debug_safety_request(DebugLevel::V).await?;
    let high = run_debug_safety_request(DebugLevel::VV).await?;

    assert!(high.len() >= low.len());
    for rendered in [low.join("\n"), high.join("\n")] {
        assert!(!rendered.contains(REQUEST_BODY_SENTINEL));
        assert!(!rendered.contains(RESPONSE_BODY_SENTINEL));
        assert!(!rendered.contains(RAW_AUTH_SENTINEL));
    }
    Ok(())
}

#[tokio::test]
async fn runtime_hooks_config_is_request_scoped() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::OK, "one"),
            MockResponse::text(StatusCode::OK, "two"),
        ],
    );
    let mut client = client(TestAuthVars::default(), transport);
    client.configure(|cfg| {
        cfg.runtime_hooks(Arc::new(NamedHooks::new("A", events.clone())));
    });
    client
        .request(TextEndpoint::default())
        .execute_decoded()
        .await?;
    client.configure(|cfg| {
        cfg.runtime_hooks(Arc::new(NamedHooks::new("B", events.clone())));
    });
    client
        .request(TextEndpoint::default())
        .execute_decoded()
        .await?;

    let events = transport_events(&events).await;
    assert!(events.contains(&"hook_pre_send:A".to_string()));
    assert!(events.contains(&"hook_post_response:A:200 OK".to_string()));
    assert!(events.contains(&"hook_pre_send:B".to_string()));
    assert!(events.contains(&"hook_post_response:B:200 OK".to_string()));
    assert_no_body_or_auth(&events.join("\n"));
    Ok(())
}

#[tokio::test]
async fn rate_limiter_config_is_request_scoped() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::OK, "one"),
            MockResponse::text(StatusCode::OK, "two"),
        ],
    );
    let mut client = client(TestAuthVars::default(), transport);
    client.configure(|cfg| {
        cfg.rate_limiter(Arc::new(NamedRateLimiter::new("A", events.clone())));
    });
    client
        .request(TextEndpoint {
            policy: cache_policy_with_rate_limit(None),
            ..TextEndpoint::default()
        })
        .execute_decoded()
        .await?;
    client.configure(|cfg| {
        cfg.rate_limiter(Arc::new(NamedRateLimiter::new("B", events.clone())));
    });
    client
        .request(TextEndpoint {
            policy: cache_policy_with_rate_limit(None),
            ..TextEndpoint::default()
        })
        .execute_decoded()
        .await?;

    let events = transport_events(&events).await;
    assert!(events.contains(&"rate_acquire:A".to_string()));
    assert!(events.contains(&"rate_response:A:200 OK".to_string()));
    assert!(events.contains(&"rate_acquire:B".to_string()));
    assert!(events.contains(&"rate_response:B:200 OK".to_string()));
    assert_no_body_or_auth(&events.join("\n"));
    Ok(())
}

#[tokio::test]
async fn cache_store_config_is_request_scoped() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::OK, "one"),
            MockResponse::text(StatusCode::OK, "two"),
        ],
    );
    let mut client = client(TestAuthVars::default(), transport);
    let endpoint = TextEndpoint {
        policy: cache_policy(),
        ..TextEndpoint::default()
    };

    client.configure(|cfg| {
        cfg.cache_store(Arc::new(NamedCache::new("A", events.clone())));
    });
    client.request(endpoint.clone()).execute_decoded().await?;
    client.configure(|cfg| {
        cfg.cache_store(Arc::new(NamedCache::new("B", events.clone())));
    });
    client.request(endpoint).execute_decoded().await?;

    let events = transport_events(&events).await;
    assert!(events.contains(&"cache_before:A".to_string()));
    assert!(events.contains(&"cache_after:A:3".to_string()));
    assert!(events.contains(&"cache_before:B".to_string()));
    assert!(events.contains(&"cache_after:B:3".to_string()));
    assert_no_body_or_auth(&events.join("\n"));
    Ok(())
}

fn cache_policy_with_rate_limit(cache_max_body: Option<usize>) -> ResolvedPolicy {
    let mut policy = cache_policy();
    if let Some(max_body) = cache_max_body {
        policy.cache = CacheSetting::Config(
            concord_core::advanced::CacheConfig::new().with_max_body_bytes(max_body),
        );
    }
    policy.rate_limit = RateLimitPlan::from_buckets(vec![
        RateLimitBucketUse::new(
            "test",
            "runtime_config",
            RateLimitKey::new(vec![
                RateLimitKeyPart::endpoint(),
                RateLimitKeyPart::method(),
            ]),
        )
        .with_window(RateLimitWindow::from_u32(10, Duration::from_secs(1)).unwrap())
        .with_cost(NonZeroU32::new(1).unwrap()),
    ]);
    policy
}

async fn run_debug_safety_request(level: DebugLevel) -> Result<Vec<String>, ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![MockResponse::text(StatusCode::OK, RESPONSE_BODY_SENTINEL)],
    );
    let mut client = client(
        TestAuthVars {
            token: Some(RAW_AUTH_SENTINEL.to_string()),
            identity: "debug",
        },
        transport,
    );
    let debug = Arc::new(RecordingDebugSink::default());
    client.configure(|cfg| {
        cfg.debug_level(level).debug_sink(debug.clone());
    });
    let policy = auth_policy(concord_core::advanced::AuthPlacement::Bearer);

    client
        .request(TextEndpoint {
            policy,
            ..TextEndpoint::default()
        })
        .execute_decoded()
        .await?;
    Ok(debug.events().await)
}

fn assert_response_too_large(err: &ApiClientError) {
    match err {
        ApiClientError::ResponseTooLarge { .. }
        | ApiClientError::ResponseBodyLimitExceeded { .. } => {}
        other => panic!("expected body-limit error, got {other:?}"),
    }
}

fn body_reads(read_count: &AtomicUsize) -> usize {
    read_count.load(Ordering::Relaxed)
}

async fn transport_events(events: &Arc<Mutex<Vec<String>>>) -> Vec<String> {
    events.lock().await.clone()
}

async fn all_observed_text(events: &Arc<Mutex<Vec<String>>>, debug: &RecordingDebugSink) -> String {
    let mut out = transport_events(events).await;
    out.extend(debug.events().await);
    out.join("\n")
}

fn assert_no_body_or_auth(rendered: &str) {
    assert!(!rendered.contains(REQUEST_BODY_SENTINEL));
    assert!(!rendered.contains(RESPONSE_BODY_SENTINEL));
    assert!(!rendered.contains(RAW_AUTH_SENTINEL));
}

#[derive(Default)]
struct RecordingDebugSink {
    events: Mutex<Vec<String>>,
}

impl RecordingDebugSink {
    async fn events(&self) -> Vec<String> {
        self.events.lock().await.clone()
    }

    fn record(&self, value: String) {
        let mut events = self.events.try_lock().expect("debug events lock");
        events.push(value);
    }
}

impl DebugSink for RecordingDebugSink {
    fn request_start(
        &self,
        dbg: DebugLevel,
        method: &Method,
        url: &str,
        endpoint: &'static str,
        page_index: u32,
    ) {
        self.record(format!(
            "request:{dbg}:{method}:{url}:{endpoint}:{page_index}"
        ));
    }

    fn request_headers(&self, dbg: DebugLevel, headers: &HeaderMap) {
        self.record(format!("request_headers:{dbg}:{headers:?}"));
    }

    fn response_status(&self, dbg: DebugLevel, status: StatusCode, url: &str, ok: bool) {
        self.record(format!("response:{dbg}:{status}:{url}:{ok}"));
    }

    fn response_headers(&self, dbg: DebugLevel, headers: &HeaderMap) {
        self.record(format!("response_headers:{dbg}:{headers:?}"));
    }
}

struct NamedHooks {
    name: &'static str,
    events: Arc<Mutex<Vec<String>>>,
}

impl NamedHooks {
    fn new(name: &'static str, events: Arc<Mutex<Vec<String>>>) -> Self {
        Self { name, events }
    }
}

impl RuntimeHooks for NamedHooks {
    fn pre_send<'a>(
        &'a self,
        _ctx: concord_core::advanced::PreSendHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ApiClientError>> + Send + 'a>> {
        Box::pin(async move {
            self.events
                .lock()
                .await
                .push(format!("hook_pre_send:{}", self.name));
            Ok(())
        })
    }

    fn post_response<'a>(
        &'a self,
        ctx: concord_core::advanced::PostResponseHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            self.events
                .lock()
                .await
                .push(format!("hook_post_response:{}:{}", self.name, ctx.status));
        })
    }
}

struct NamedRateLimiter {
    name: &'static str,
    events: Arc<Mutex<Vec<String>>>,
}

impl NamedRateLimiter {
    fn new(name: &'static str, events: Arc<Mutex<Vec<String>>>) -> Self {
        Self { name, events }
    }
}

impl RateLimiter for NamedRateLimiter {
    fn acquire<'a>(
        &'a self,
        _ctx: RateLimitContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitPermit, ApiClientError>> {
        Box::pin(async move {
            self.events
                .lock()
                .await
                .push(format!("rate_acquire:{}", self.name));
            Ok(RateLimitPermit)
        })
    }

    fn on_response<'a>(
        &'a self,
        ctx: RateLimitResponseContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitResponseAction, ApiClientError>> {
        Box::pin(async move {
            self.events
                .lock()
                .await
                .push(format!("rate_response:{}:{}", self.name, ctx.status));
            Ok(RateLimitResponseAction::Continue)
        })
    }
}

struct NamedCache {
    name: &'static str,
    events: Arc<Mutex<Vec<String>>>,
}

impl NamedCache {
    fn new(name: &'static str, events: Arc<Mutex<Vec<String>>>) -> Self {
        Self { name, events }
    }
}

impl CacheStore for NamedCache {
    fn before_request<'a>(&'a self, _request: &'a BuiltRequest) -> CacheFuture<'a, CacheBefore> {
        Box::pin(async move {
            self.events
                .lock()
                .await
                .push(format!("cache_before:{}", self.name));
            CacheBefore::Miss
        })
    }

    fn after_response<'a>(
        &'a self,
        _request: &'a BuiltRequest,
        response: &'a BuiltResponse,
        _revalidation: Option<CacheRevalidation>,
    ) -> CacheFuture<'a, CacheAfter> {
        Box::pin(async move {
            self.events.lock().await.push(format!(
                "cache_after:{}:{}",
                self.name,
                response.body.len()
            ));
            CacheAfter::Stored
        })
    }
}

use super::common::*;
use bytes::Bytes;
use concord_core::advanced::{AuthPlacement, Caps, DebugSink, NoopCacheStore, NoopRateLimiter};
use concord_core::prelude::{ApiClientError, DebugLevel};
use http::{HeaderMap, Method, StatusCode};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
use tokio::sync::Mutex;

#[tokio::test]
async fn fresh_cache_hit_bypasses_inflight_rate_limit_and_transport() -> Result<(), ApiClientError>
{
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
    let inflight_events = Arc::new(StdMutex::new(Vec::new()));
    let mut client = client(
        TestAuthVars {
            token: Some("secret-token".to_string()),
            identity: "user-a",
        },
        transport,
    );
    client.set_runtime_hooks(Arc::new(RecordingRuntimeHooks::new(events.clone())));
    client.set_inflight_policy(Arc::new(RecordingInflightPolicy::new(
        inflight_events.clone(),
    )));
    configure_runtime(&mut client, Some(cache), Some(limiter), false, None);

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
            .any(|event| event == "cache_before:Bearer secret-token")
    );
    assert!(!events.iter().any(|event| event == "pre_send"));
    assert!(!events.iter().any(|event| event == "rate_acquire"));
    assert!(!events.iter().any(|event| event == "transport"));
    assert!(inflight_events.lock().expect("inflight events").is_empty());
    Ok(())
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
    configure_runtime(&mut client, Some(cache), None, false, None);

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
        requests[0].cache_mode,
        concord_core::advanced::CacheRequestMode::Bypass
    );
    assert_eq!(debug.events(), vec!["request_start:v:Text:0"]);
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

    fn request_headers(&self, _dbg: DebugLevel, _headers: &HeaderMap) {}

    fn request_body(
        &self,
        _dbg: DebugLevel,
        _body: &Bytes,
        _format: concord_core::internal::Format,
        _max_chars: usize,
    ) {
    }

    fn response_status(&self, _dbg: DebugLevel, _status: StatusCode, _url: &str, _ok: bool) {}

    fn response_headers(&self, _dbg: DebugLevel, _headers: &HeaderMap) {}

    fn response_body(
        &self,
        _dbg: DebugLevel,
        _body: &Bytes,
        _format: concord_core::internal::Format,
        _max_chars: usize,
    ) {
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

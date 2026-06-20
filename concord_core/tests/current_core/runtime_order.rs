use super::common::*;
use bytes::Bytes;
use concord_core::advanced::{AuthPlacement, Caps, DebugSink, NoopCacheStore, NoopRateLimiter};
use concord_core::internal::ResolvedPolicy;
use concord_core::prelude::{ApiClientError, DebugLevel};
use http::{HeaderMap, Method, StatusCode};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
use tokio::sync::Mutex;

#[tokio::test]
async fn fresh_cache_hit_bypasses_rate_limit_and_transport() -> Result<(), ApiClientError> {
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
    let mut client = client(
        TestAuthVars {
            token: Some("secret-token".to_string()),
            identity: "user-a",
        },
        transport,
    );
    client.set_runtime_hooks(Arc::new(RecordingRuntimeHooks::new(events.clone())));
    configure_runtime(&mut client, Some(cache), Some(limiter));

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
            .any(|event| event.starts_with("cache_before:hash:"))
    );
    assert!(!events.iter().any(|event| event.contains("secret-token")));
    assert!(!events.iter().any(|event| event == "pre_send"));
    assert!(!events.iter().any(|event| event == "rate_acquire"));
    assert!(!events.iter().any(|event| event == "transport"));
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
    configure_runtime(&mut client, Some(cache), None);

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

#[tokio::test]
async fn response_content_length_above_limit_fails_before_decode() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![MockResponse::text(StatusCode::OK, "too-large").with_content_length(Some(9))],
    );
    let mut client = client(TestAuthVars::default(), transport);
    client.configure(|cfg| {
        cfg.max_response_body_bytes(4);
    });

    let err = client
        .request(TextEndpoint::default())
        .execute_decoded()
        .await
        .expect_err("known content length above limit should fail");

    assert!(matches!(
        err,
        ApiClientError::ResponseTooLarge {
            limit: 4,
            actual: 9,
            ..
        }
    ));
}

#[tokio::test]
async fn response_unknown_length_above_limit_fails_while_reading() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, Bytes::new())
                .with_content_length(None)
                .with_chunks(vec![Bytes::from_static(b"abcd"), Bytes::from_static(b"e")]),
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
        .expect_err("chunked body above limit should fail");

    assert!(matches!(
        err,
        ApiClientError::ResponseBodyLimitExceeded { limit: 4, .. }
    ));
}

#[tokio::test]
async fn response_exactly_at_limit_succeeds() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, "abcd")]);
    let mut client = client(TestAuthVars::default(), transport);
    client.configure(|cfg| {
        cfg.max_response_body_bytes(4);
    });

    let decoded = client
        .request(TextEndpoint::default())
        .execute_decoded()
        .await?;

    assert_eq!(decoded.value(), "abcd");
    Ok(())
}

#[tokio::test]
async fn response_below_limit_succeeds() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, "abc")]);
    let mut client = client(TestAuthVars::default(), transport);
    client.configure(|cfg| {
        cfg.max_response_body_bytes(4);
    });

    let decoded = client
        .request(TextEndpoint::default())
        .execute_decoded()
        .await?;

    assert_eq!(decoded.value(), "abc");
    Ok(())
}

#[tokio::test]
async fn response_too_large_does_not_decode() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, Bytes::from_static(b"\xff"))
                .with_content_length(Some(8)),
        ],
    );
    let mut client = client(TestAuthVars::default(), transport);
    client.configure(|cfg| {
        cfg.max_response_body_bytes(1);
    });

    let err = client
        .request(TextEndpoint::default())
        .execute_decoded()
        .await
        .expect_err("body limit should fail before utf-8 decode");

    assert!(matches!(err, ApiClientError::ResponseTooLarge { .. }));
    assert!(!err.to_string().contains("utf-8"));
}

#[tokio::test]
async fn response_too_large_does_not_cache() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let cache = Arc::new(RecordingCache::miss(events.clone()));
    let after_response_count = cache.after_response_count.clone();
    let transport = MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, "large")]);
    let mut client = client(TestAuthVars::default(), transport);
    configure_runtime(&mut client, Some(cache), None);
    client.configure(|cfg| {
        cfg.max_response_body_bytes(4);
    });

    let endpoint = TextEndpoint {
        policy: cache_policy(),
        ..Default::default()
    };
    let err = client
        .request(endpoint)
        .execute_decoded()
        .await
        .expect_err("body limit should fail before cache write");

    assert!(matches!(err, ApiClientError::ResponseTooLarge { .. }));
    assert_eq!(*after_response_count.lock().await, 0);
}

#[tokio::test]
async fn response_limit_applies_when_cache_is_off() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, "large")]);
    let mut client = client(TestAuthVars::default(), transport);
    client.configure(|cfg| {
        cfg.max_response_body_bytes(4);
    });

    let err = client
        .request(TextEndpoint::default())
        .execute_decoded()
        .await
        .expect_err("response limit applies independently from cache");

    assert!(matches!(err, ApiClientError::ResponseTooLarge { .. }));
}

#[tokio::test]
async fn cache_max_body_smaller_than_response_limit_skips_store_but_decode_succeeds()
-> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let cache = Arc::new(RecordingCache::miss(events.clone()));
    let after_response_count = cache.after_response_count.clone();
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "2k")],
    );
    let mut client = client(TestAuthVars::default(), transport);
    configure_runtime(&mut client, Some(cache), None);
    client.configure(|cfg| {
        cfg.max_response_body_bytes(4 * 1024);
    });

    let endpoint = TextEndpoint {
        policy: ResolvedPolicy {
            cache: concord_core::internal::CacheSetting::Config(
                concord_core::advanced::CacheConfig::new().with_max_body_bytes(1),
            ),
            ..Default::default()
        },
        ..Default::default()
    };
    let decoded = client.request(endpoint).execute_decoded().await?;

    assert_eq!(decoded.value(), "2k");
    assert_eq!(*after_response_count.lock().await, 1);
    assert!(
        events
            .lock()
            .await
            .iter()
            .any(|event| event == "cache_max_body_skip")
    );
    Ok(())
}

#[tokio::test]
async fn response_limit_smaller_than_cache_max_body_fails_before_cache() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let cache = Arc::new(RecordingCache::miss(events.clone()));
    let after_response_count = cache.after_response_count.clone();
    let transport = MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, "large")]);
    let mut client = client(TestAuthVars::default(), transport);
    configure_runtime(&mut client, Some(cache), None);
    client.configure(|cfg| {
        cfg.max_response_body_bytes(4);
    });

    let endpoint = TextEndpoint {
        policy: ResolvedPolicy {
            cache: concord_core::internal::CacheSetting::Config(
                concord_core::advanced::CacheConfig::new().with_max_body_bytes(4 * 1024),
            ),
            ..Default::default()
        },
        ..Default::default()
    };
    let err = client
        .request(endpoint)
        .execute_decoded()
        .await
        .expect_err("response limit should fail before cache max_body is considered");

    assert!(matches!(err, ApiClientError::ResponseTooLarge { .. }));
    assert_eq!(*after_response_count.lock().await, 0);
}

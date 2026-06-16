use super::common::*;
use bytes::Bytes;
use concord_core::advanced::{
    BuiltResponse, CacheAfter, CacheBefore, CacheFuture, CacheKey, CacheRevalidation, CacheStore,
    DebugSink,
};
use concord_core::prelude::{ApiClientError, DebugLevel};
use http::{HeaderMap, Method, StatusCode};
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use tokio::sync::Mutex;

#[tokio::test]
async fn fresh_cache_hit_skips_transport_and_rate_limit() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let cached = built_response("Text", StatusCode::OK, "cached");
    let cache = Arc::new(RecordingCache::hit(events.clone(), cached));
    let limiter = Arc::new(RecordingRateLimiter::new(events.clone()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "transport")],
    );
    let sent_transport = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    configure_runtime(&mut client, Some(cache), Some(limiter), false, None);

    let endpoint = TextEndpoint {
        policy: cache_policy(),
        ..Default::default()
    };
    let decoded = client.request(endpoint).execute_decoded().await?;

    assert_eq!(decoded.value(), "cached");
    assert_eq!(sent_transport.sent_count().await, 0);
    let events = events.lock().await.clone();
    assert!(!events.iter().any(|event| event == "rate_acquire"));
    assert!(!events.iter().any(|event| event == "transport"));
    Ok(())
}

#[tokio::test]
async fn stale_revalidation_goes_through_rate_limit_and_transport() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let stale = built_response("Text", StatusCode::OK, "stale");
    let cache = Arc::new(RecordingCache::revalidate(events.clone(), stale));
    let limiter = Arc::new(RecordingRateLimiter::new(events.clone()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "fresh")],
    );
    let sent_transport = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    configure_runtime(&mut client, Some(cache), Some(limiter), false, None);

    let endpoint = TextEndpoint {
        policy: cache_policy(),
        ..Default::default()
    };
    let decoded = client.request(endpoint).execute_decoded().await?;

    assert_eq!(decoded.value(), "fresh");
    assert_eq!(sent_transport.sent_count().await, 1);
    let events = events.lock().await.clone();
    assert!(events.iter().any(|event| event == "rate_acquire"));
    assert!(events.iter().any(|event| event == "transport"));
    Ok(())
}

#[tokio::test]
async fn not_modified_revalidation_runs_post_response_before_rate_limit_observation()
-> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let cached = built_response("Text", StatusCode::OK, "cached");
    let cache = Arc::new(NotModifiedRevalidationCache {
        cached: cached.clone(),
    });
    let limiter = Arc::new(RecordingRateLimiter::new(events.clone()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::NOT_MODIFIED, "")],
    );
    let mut client = client(TestAuthVars::default(), transport);
    client.set_runtime_hooks(Arc::new(RecordingRuntimeHooks::new(events.clone())));
    configure_runtime(&mut client, Some(cache), Some(limiter), false, None);

    let endpoint = TextEndpoint {
        policy: cache_policy(),
        ..Default::default()
    };
    let decoded = client.request(endpoint).execute_decoded().await?;

    assert_eq!(decoded.value(), "cached");
    let events = events.lock().await.clone();
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
    assert!(transport < classify);
    assert!(classify < observe);
    Ok(())
}

#[tokio::test]
async fn stale_is_not_returned_before_retry_exhaustion() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let stale = built_response("Text", StatusCode::OK, "stale");
    let cache = Arc::new(RecordingCache::revalidate_stale_on_error(
        events.clone(),
        stale,
    ));
    let after_error_count = cache.after_error_count.clone();
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
    assert_eq!(*after_error_count.lock().await, 0);
    Ok(())
}

#[tokio::test]
async fn stale_cache_fallback_happens_after_retry_exhaustion() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let stale = built_response("Text", StatusCode::OK, "stale");
    let cache = Arc::new(RecordingCache::revalidate_stale_on_error(
        events.clone(),
        stale,
    ));
    let after_error_count = cache.after_error_count.clone();
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, "retry-me"),
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, "still-failing"),
        ],
    );
    let sent_transport = transport.clone();
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

    assert_eq!(decoded.value(), "stale");
    assert_eq!(sent_transport.sent_count().await, 2);
    assert_eq!(*after_error_count.lock().await, 1);
    Ok(())
}

#[tokio::test]
async fn stale_is_not_returned_when_policy_disallows() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let stale = built_response("Text", StatusCode::OK, "stale");
    let cache = Arc::new(RecordingCache::revalidate(events.clone(), stale));
    let after_error_count = cache.after_error_count.clone();
    let transport = MockTransport::new(
        events,
        vec![MockResponse::text(
            StatusCode::INTERNAL_SERVER_ERROR,
            "failing",
        )],
    );
    let mut client = client(TestAuthVars::default(), transport);
    configure_runtime(&mut client, Some(cache), None, false, None);

    let endpoint = TextEndpoint {
        policy: cache_policy(),
        ..Default::default()
    };
    let err = client
        .request(endpoint)
        .execute_decoded()
        .await
        .expect_err("stale fallback disabled");

    assert!(err.to_string().contains("status 500"));
    assert_eq!(*after_error_count.lock().await, 1);
}

#[tokio::test]
async fn stale_decode_failure_includes_endpoint_context() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let stale = built_response("Text", StatusCode::OK, Bytes::from_static(b"\xff"));
    let cache = Arc::new(RecordingCache::revalidate_stale_on_error(
        events.clone(),
        stale,
    ));
    let transport = MockTransport::new(
        events,
        vec![MockResponse::text(
            StatusCode::INTERNAL_SERVER_ERROR,
            "failing",
        )],
    );
    let mut client = client(TestAuthVars::default(), transport);
    configure_runtime(&mut client, Some(cache), None, false, None);

    let endpoint = TextEndpoint {
        policy: cache_policy(),
        ..Default::default()
    };
    let err = client
        .request(endpoint)
        .execute_decoded()
        .await
        .expect_err("invalid stale body should fail decode");
    let msg = err.to_string();

    assert!(msg.contains("GET Text"));
    assert!(msg.contains("decode error"));
}

#[derive(Clone)]
struct NotModifiedRevalidationCache {
    cached: BuiltResponse,
}

impl CacheStore for NotModifiedRevalidationCache {
    fn before_request<'a>(
        &'a self,
        _request: &'a concord_core::advanced::BuiltRequest,
    ) -> CacheFuture<'a, CacheBefore> {
        Box::pin(async move {
            CacheBefore::Revalidate {
                request_headers: HeaderMap::new(),
                cached: CacheRevalidation {
                    key: CacheKey::new("revalidate-304".to_string()),
                    cached_response: self.cached.clone(),
                },
            }
        })
    }

    fn after_response<'a>(
        &'a self,
        _request: &'a concord_core::advanced::BuiltRequest,
        response: &'a BuiltResponse,
        revalidation: Option<CacheRevalidation>,
    ) -> CacheFuture<'a, CacheAfter> {
        Box::pin(async move {
            if response.status == StatusCode::NOT_MODIFIED {
                if let Some(revalidation) = revalidation {
                    return CacheAfter::Updated(Box::new(revalidation.cached_response));
                }
            }
            CacheAfter::Stored
        })
    }

    fn after_error<'a>(
        &'a self,
        _request: &'a concord_core::advanced::BuiltRequest,
        _error: &'a ApiClientError,
        _revalidation: Option<CacheRevalidation>,
    ) -> CacheFuture<'a, Option<BuiltResponse>> {
        Box::pin(async move { None })
    }
}

#[tokio::test]
async fn stale_fallback_emits_debug_event() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let stale = built_response("Text", StatusCode::OK, "stale");
    let cache = Arc::new(RecordingCache::revalidate_stale_on_error(
        events.clone(),
        stale,
    ));
    let transport = MockTransport::new(
        events,
        vec![MockResponse::text(
            StatusCode::INTERNAL_SERVER_ERROR,
            "failing",
        )],
    );
    let mut client = client(TestAuthVars::default(), transport);
    let debug = Arc::new(RecordingDebugSink::default());
    client.set_debug_sink(debug.clone());
    configure_runtime(&mut client, Some(cache), None, false, None);

    let endpoint = TextEndpoint {
        policy: cache_policy(),
        ..Default::default()
    };
    let decoded = client.request(endpoint).execute_decoded().await?;

    assert_eq!(decoded.value(), "stale");
    assert_eq!(
        debug.events.lock().expect("debug events lock").as_slice(),
        ["stale_fallback:GET:https://example.com/text:Text:0"]
    );
    Ok(())
}

#[tokio::test]
async fn cache_key_partitions_by_auth_identity() {
    let mut built_req_a = BuiltRequestFixture::new("https://example.com/text").into_request();
    built_req_a
        .extensions
        .auth_identities
        .push("user:a".to_string());
    let mut built_req_b = BuiltRequestFixture::new("https://example.com/text").into_request();
    built_req_b
        .extensions
        .auth_identities
        .push("user:b".to_string());

    let key_a = concord_core::advanced::default_cache_key(&built_req_a);
    let key_b = concord_core::advanced::default_cache_key(&built_req_b);
    assert_ne!(key_a, key_b);
}

struct BuiltRequestFixture {
    request: concord_core::advanced::BuiltRequest,
}

impl BuiltRequestFixture {
    fn new(url: &str) -> Self {
        Self {
            request: concord_core::advanced::BuiltRequest {
                meta: concord_core::advanced::RequestMeta {
                    endpoint: "Text",
                    method: http::Method::GET,
                    idempotent: true,
                    attempt: 0,
                    page_index: 0,
                },
                url: url.parse().expect("test url"),
                headers: Default::default(),
                body: None,
                timeout: None,
                retry: concord_core::internal::RetrySetting::Inherit,
                rate_limit: Default::default(),
                cache: concord_core::internal::CacheSetting::default(),
                cache_mode: concord_core::advanced::CacheRequestMode::Default,
                cache_revalidation: None,
                extensions: Default::default(),
            },
        }
    }

    fn into_request(self) -> concord_core::advanced::BuiltRequest {
        self.request
    }
}

#[derive(Default)]
struct RecordingDebugSink {
    events: StdMutex<Vec<String>>,
}

impl DebugSink for RecordingDebugSink {
    fn request_start(
        &self,
        _dbg: DebugLevel,
        _method: &Method,
        _url: &str,
        _endpoint: &'static str,
        _page_index: u32,
    ) {
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

    fn stale_fallback(
        &self,
        _dbg: DebugLevel,
        method: &Method,
        url: &str,
        endpoint: &'static str,
        page_index: u32,
    ) {
        self.events.lock().expect("debug events lock").push(format!(
            "stale_fallback:{method}:{url}:{endpoint}:{page_index}"
        ));
    }
}

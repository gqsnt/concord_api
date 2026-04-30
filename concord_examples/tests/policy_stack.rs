use bytes::Bytes;
use concord_core::advanced::{
    BuiltRequest, BuiltResponse, CacheAfter, CacheBefore, CacheFuture, CacheKey, CacheRevalidation,
    CacheStore, RateLimitContext, RateLimitFuture, RateLimitPermit, RateLimitResponseAction,
    RateLimitResponseContext, RateLimiter, RequestMeta,
};
use concord_examples::policy_stack::PolicyApi;
use concord_test_support::{MockReply, mock};
use http::header::RETRY_AFTER;
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[tokio::test]
async fn fresh_cache_hit_skips_rate_limiter_and_transport() {
    let cache = Arc::new(TestCache::hit(text_response("cached")));
    let limiter = Arc::new(RecordingLimiter::default());
    let (transport, handle) = mock()
        .reply(MockReply::ok_text(Bytes::from_static(b"transport")))
        .build();
    let mut api = PolicyApi::new_with_transport(transport);
    api.configure_mut(|cfg| {
        cfg.cache_store(cache.clone());
        cfg.rate_limiter(limiter.clone());
    });

    let value = api.text().execute().await.expect("cache hit decodes");

    assert_eq!(value, "cached");
    assert_eq!(cache.events(), vec!["cache_before"]);
    assert!(limiter.events().is_empty());
    handle.assert_recorded_len(0);
    std::mem::forget(handle);
}

#[tokio::test]
async fn retry_profile_honors_max_attempts() {
    let (transport, handle) = mock()
        .reply(MockReply::status(StatusCode::INTERNAL_SERVER_ERROR))
        .reply(MockReply::status(StatusCode::INTERNAL_SERVER_ERROR))
        .build();
    let api = PolicyApi::new_with_transport(transport);

    let err = api
        .retry_only()
        .execute()
        .await
        .expect_err("max_attempts=2 should stop after two 500s");

    assert_eq!(err.http_status(), Some(StatusCode::INTERNAL_SERVER_ERROR));
    handle.assert_recorded_len(2);
    handle.finish();
}

#[tokio::test]
async fn retry_after_handled_by_rate_limiter_does_not_sleep_twice() {
    let limiter = Arc::new(RecordingLimiter::limited_with_cooldown(
        Duration::from_secs(5),
    ));
    let (transport, handle) = mock()
        .reply(
            MockReply::status(StatusCode::TOO_MANY_REQUESTS)
                .with_header(RETRY_AFTER, HeaderValue::from_static("5")),
        )
        .reply(MockReply::ok_text(Bytes::from_static(b"ok")))
        .build();
    let mut api = PolicyApi::new_with_transport(transport);
    api.configure_mut(|cfg| {
        cfg.rate_limiter(limiter.clone());
    });

    let value = tokio::time::timeout(Duration::from_millis(250), api.rate_limited().execute())
        .await
        .expect("rate limiter handled retry-after; client must not sleep for header duration")
        .expect("retry after 429 succeeds");

    assert_eq!(value, "ok");
    assert_eq!(
        limiter.events(),
        vec![
            "rate_acquire",
            "rate_response:429",
            "rate_acquire",
            "rate_response:200",
        ]
    );
    handle.assert_recorded_len(2);
    handle.finish();
}

#[tokio::test]
async fn rate_limit_limiter_observes_successful_response() {
    let limiter = Arc::new(RecordingLimiter::default());
    let (transport, handle) = mock()
        .reply(MockReply::ok_text(Bytes::from_static(b"limited-ok")))
        .build();
    let mut api = PolicyApi::new_with_transport(transport);
    api.configure_mut(|cfg| {
        cfg.rate_limiter(limiter.clone());
    });

    let value = api.rate_limited().execute().await.unwrap();

    assert_eq!(value, "limited-ok");
    assert_eq!(limiter.events(), vec!["rate_acquire", "rate_response:200"]);
    handle.assert_recorded_len(1);
    handle.finish();
}

#[tokio::test]
async fn stale_on_error_returns_revalidated_cached_response_after_retry_exhaustion() {
    let cache = Arc::new(TestCache::revalidate(text_response("stale")));
    let limiter = Arc::new(RecordingLimiter::default());
    let (transport, handle) = mock()
        .reply(MockReply::status(StatusCode::INTERNAL_SERVER_ERROR))
        .reply(MockReply::status(StatusCode::INTERNAL_SERVER_ERROR))
        .build();
    let mut api = PolicyApi::new_with_transport(transport);
    api.configure_mut(|cfg| {
        cfg.cache_store(cache.clone());
        cfg.rate_limiter(limiter);
    });

    let value = api
        .text()
        .execute()
        .await
        .expect("stale cache fallback succeeds after retry exhaustion");

    assert_eq!(value, "stale");
    assert_eq!(
        cache.events(),
        vec!["cache_before", "cache_before", "cache_after_error"]
    );
    handle.assert_recorded_len(2);
    handle.finish();
}

#[derive(Clone)]
enum CacheMode {
    Hit(BuiltResponse),
    Revalidate(BuiltResponse),
}

struct TestCache {
    mode: CacheMode,
    events: Mutex<Vec<&'static str>>,
}

impl TestCache {
    fn hit(response: BuiltResponse) -> Self {
        Self {
            mode: CacheMode::Hit(response),
            events: Mutex::new(Vec::new()),
        }
    }

    fn revalidate(response: BuiltResponse) -> Self {
        Self {
            mode: CacheMode::Revalidate(response),
            events: Mutex::new(Vec::new()),
        }
    }

    fn events(&self) -> Vec<&'static str> {
        self.events.lock().expect("cache events lock").clone()
    }

    fn record(&self, event: &'static str) {
        self.events.lock().expect("cache events lock").push(event);
    }
}

impl CacheStore for TestCache {
    fn key_for(&self, _request: &BuiltRequest) -> Option<CacheKey> {
        Some(CacheKey::new("policy-test-cache-key".to_string()))
    }

    fn before_request<'a>(&'a self, _request: &'a BuiltRequest) -> CacheFuture<'a, CacheBefore> {
        Box::pin(async move {
            self.record("cache_before");
            match &self.mode {
                CacheMode::Hit(response) => CacheBefore::Hit(response.clone()),
                CacheMode::Revalidate(response) => CacheBefore::Revalidate {
                    request_headers: HeaderMap::new(),
                    cached: CacheRevalidation {
                        key: CacheKey::new("policy-test-cache-key".to_string()),
                        cached_response: response.clone(),
                    },
                },
            }
        })
    }

    fn after_response<'a>(
        &'a self,
        _request: &'a BuiltRequest,
        _response: &'a BuiltResponse,
        _revalidation: Option<CacheRevalidation>,
    ) -> CacheFuture<'a, CacheAfter> {
        Box::pin(async move {
            self.record("cache_after_response");
            CacheAfter::Stored
        })
    }

    fn after_error<'a>(
        &'a self,
        _request: &'a BuiltRequest,
        _error: &'a concord_core::prelude::ApiClientError,
        revalidation: Option<CacheRevalidation>,
    ) -> CacheFuture<'a, Option<BuiltResponse>> {
        Box::pin(async move {
            self.record("cache_after_error");
            revalidation.map(|cached| cached.cached_response)
        })
    }
}

#[derive(Default)]
struct RecordingLimiter {
    action: Mutex<RateLimitResponseAction>,
    events: Mutex<Vec<String>>,
}

impl RecordingLimiter {
    fn limited_with_cooldown(delay: Duration) -> Self {
        Self {
            action: Mutex::new(RateLimitResponseAction::Limited {
                retry_after: Some(delay),
                target: Default::default(),
                cooldown_stored: true,
            }),
            events: Mutex::new(Vec::new()),
        }
    }

    fn events(&self) -> Vec<String> {
        self.events.lock().expect("limiter events lock").clone()
    }
}

impl RateLimiter for RecordingLimiter {
    fn acquire<'a>(
        &'a self,
        _ctx: RateLimitContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitPermit, concord_core::prelude::ApiClientError>> {
        Box::pin(async move {
            self.events
                .lock()
                .expect("limiter events lock")
                .push("rate_acquire".to_string());
            Ok(RateLimitPermit)
        })
    }

    fn on_response<'a>(
        &'a self,
        ctx: RateLimitResponseContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitResponseAction, concord_core::prelude::ApiClientError>>
    {
        Box::pin(async move {
            self.events
                .lock()
                .expect("limiter events lock")
                .push(format!("rate_response:{}", ctx.status.as_u16()));
            Ok(self.action.lock().expect("limiter action lock").clone())
        })
    }
}

fn text_response(body: &'static str) -> BuiltResponse {
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain"),
    );
    BuiltResponse {
        meta: RequestMeta {
            endpoint: "Text",
            method: Method::GET,
            idempotent: true,
            attempt: 0,
            page_index: 0,
        },
        url: "https://example.com/text".parse().unwrap(),
        status: StatusCode::OK,
        headers,
        body: Bytes::from_static(body.as_bytes()),
        rate_limit: Default::default(),
    }
}

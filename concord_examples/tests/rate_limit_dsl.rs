use bytes::Bytes;
use concord_core::advanced::*;
use concord_core::prelude::*;
use concord_core::transport::{
    BuiltRequest, BuiltResponse, TransportBody, TransportError, TransportResponse,
};
use concord_macros::api;
use concord_test_support::*;
use http::HeaderMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Default)]
struct RecordingLimiter {
    plans: Arc<Mutex<Vec<RateLimitPlan>>>,
}

impl RateLimiter for RecordingLimiter {
    fn acquire<'a>(
        &'a self,
        ctx: RateLimitContext<'a>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<RateLimitPermit, ApiClientError>> + Send + 'a>,
    > {
        Box::pin(async move {
            self.plans.lock().expect("plan lock").push(ctx.plan.clone());
            Ok(RateLimitPermit)
        })
    }
}

#[derive(Clone)]
struct SlowTransport {
    started: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Notify>,
    calls: Arc<AtomicUsize>,
}

impl Transport for SlowTransport {
    fn send(
        &self,
        req: BuiltRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send>> {
        let started = self.started.clone();
        let release = self.release.clone();
        let calls = self.calls.clone();
        Box::pin(async move {
            calls.fetch_add(1, Ordering::SeqCst);
            started.notify_waiters();
            release.notified().await;
            let body = json_bytes(&());
            Ok(TransportResponse {
                meta: req.meta,
                url: req.url,
                status: http::StatusCode::OK,
                headers: json_headers(),
                content_length: Some(body.len() as u64),
                rate_limit: req.rate_limit,
                body: Box::new(StaticBody { chunk: Some(body) }),
            })
        })
    }
}

struct StaticBody {
    chunk: Option<Bytes>,
}

impl TransportBody for StaticBody {
    fn next_chunk<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Bytes>, TransportError>> + Send + 'a>> {
        Box::pin(async move { Ok(self.chunk.take()) })
    }
}

fn json_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("application/json"),
    );
    headers
}

#[derive(Clone)]
struct OrderedTransport {
    events: Arc<Mutex<Vec<&'static str>>>,
}

impl Transport for OrderedTransport {
    fn send(
        &self,
        req: BuiltRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send>> {
        let events = self.events.clone();
        Box::pin(async move {
            events.lock().expect("events lock").push("send");
            let body = json_bytes(&());
            Ok(TransportResponse {
                meta: req.meta,
                url: req.url,
                status: http::StatusCode::OK,
                headers: json_headers(),
                content_length: Some(body.len() as u64),
                rate_limit: req.rate_limit,
                body: Box::new(StaticBody { chunk: Some(body) }),
            })
        })
    }
}

#[derive(Clone)]
struct OrderedLimiter {
    events: Arc<Mutex<Vec<&'static str>>>,
}

impl RateLimiter for OrderedLimiter {
    fn acquire<'a>(
        &'a self,
        _ctx: RateLimitContext<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<RateLimitPermit, ApiClientError>> + Send + 'a>> {
        Box::pin(async move {
            self.events
                .lock()
                .expect("events lock")
                .push("rate_limit_acquire");
            Ok(RateLimitPermit)
        })
    }

    fn on_response<'a>(
        &'a self,
        _ctx: RateLimitResponseContext<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<RateLimitResponseAction, ApiClientError>> + Send + 'a>>
    {
        Box::pin(async move {
            self.events
                .lock()
                .expect("events lock")
                .push("rate_limit_observe");
            Ok(RateLimitResponseAction::Continue)
        })
    }
}

struct OrderedCache {
    events: Arc<Mutex<Vec<&'static str>>>,
}

impl CacheStore for OrderedCache {
    fn key_for(&self, _request: &BuiltRequest) -> Option<CacheKey> {
        Some(CacheKey::new("ordered".to_string()))
    }

    fn before_request<'a>(
        &'a self,
        _request: &'a BuiltRequest,
    ) -> Pin<Box<dyn Future<Output = CacheBefore> + Send + 'a>> {
        Box::pin(async move {
            self.events
                .lock()
                .expect("events lock")
                .push("cache_before");
            CacheBefore::Miss
        })
    }

    fn after_response<'a>(
        &'a self,
        _request: &'a BuiltRequest,
        _response: &'a BuiltResponse,
        _revalidation: Option<CacheRevalidation>,
    ) -> Pin<Box<dyn Future<Output = CacheAfter> + Send + 'a>> {
        Box::pin(async move {
            self.events
                .lock()
                .expect("events lock")
                .push("cache_after_response");
            CacheAfter::Stored
        })
    }
}

#[derive(Default)]
struct AlwaysHitCache {
    response: Mutex<Option<BuiltResponse>>,
}

impl CacheStore for AlwaysHitCache {
    fn key_for(&self, request: &BuiltRequest) -> Option<CacheKey> {
        let body = json_bytes(&());
        *self.response.lock().expect("cache lock") = Some(BuiltResponse {
            meta: request.meta.clone(),
            url: request.url.clone(),
            status: http::StatusCode::OK,
            headers: json_headers(),
            body,
            rate_limit: request.rate_limit.clone(),
        });
        Some(CacheKey::new("hit".to_string()))
    }

    fn get<'a>(
        &'a self,
        _key: &'a CacheKey,
    ) -> Pin<Box<dyn Future<Output = Option<BuiltResponse>> + Send + 'a>> {
        Box::pin(async move { self.response.lock().expect("cache lock").clone() })
    }
}

#[derive(Default)]
struct HeaderScopePolicy;

impl RateLimitObserver for HeaderScopePolicy {
    fn observe(&self, ctx: RateLimitResponseContext<'_>) -> RateLimitObservation {
        ctx.on_429().scope_header("x-rate-limit-type").retry_after()
    }
}

#[tokio::test]
async fn runtime_order_runs_cache_before_rate_limit_send_observe_cache_after() {
    api! {
        client RuntimeOrderApi {
            base https "example.com"
            default {
                cache short
                rate_limit app
            }
            cache short {
                ttl 60 seconds
            }
            rate_limit app {
                bucket application by [host] {
                    10 / 1s
                }
            }
        }

        GET Cached
            as cached
            path ["cached"]
            -> Json<()>;
    }

    use runtime_order_api::*;

    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = OrderedTransport {
        events: events.clone(),
    };
    let api = RuntimeOrderApi::new_with_transport(transport).with_configure(|cfg| {
        cfg.cache_store(Arc::new(OrderedCache {
            events: events.clone(),
        }));
        cfg.rate_limiter(Arc::new(OrderedLimiter {
            events: events.clone(),
        }));
    });

    api.cached().await.unwrap();

    assert_eq!(
        events.lock().expect("events lock").as_slice(),
        &[
            "cache_before",
            "rate_limit_acquire",
            "send",
            "rate_limit_observe",
            "cache_after_response"
        ]
    );
}

#[tokio::test]
async fn rate_limit_profiles_generate_request_plan_and_allow_custom_limiter() {
    api! {
        client RateLimitDslApi {
            base https "example.com"
            default {
                rate_limit app
            }
            rate_limit app {
                    bucket application by [host] {
                        500 / 10s
                    }
            }
            rate_limit method_read {
                    bucket method by [host, endpoint] {
                        30 / 10s
                        500 / 10m
                    }
            }
        }

        GET Ping
        -> Json<()>
        {
            rate_limit method_read
        }

        GET NoLimit
        -> Json<()>
        {
            rate_limit off
        }
    }

    use rate_limit_dsl_api::*;

    let (transport, h) = mock()
        .replies([
            MockReply::ok_json(json_bytes(&())),
            MockReply::ok_json(json_bytes(&())),
        ])
        .build();
    let limiter = RecordingLimiter::default();
    let plans = limiter.plans.clone();
    let api = RateLimitDslApi::new_with_transport(transport).with_configure(|cfg| {
        cfg.rate_limiter(Arc::new(limiter));
    });

    api.request(endpoints::Ping::new()).execute().await.unwrap();
    api.request(endpoints::NoLimit::new())
        .execute()
        .await
        .unwrap();

    let plans = plans.lock().expect("plan lock").clone();
    assert_eq!(plans.len(), 2);
    assert_eq!(plans[0].buckets().len(), 2);
    assert_eq!(plans[0].buckets()[0].id.kind, "application");
    assert_eq!(plans[0].buckets()[1].id.kind, "method");
    assert_eq!(plans[0].buckets()[1].windows.len(), 2);
    assert!(
        plans[1].is_empty(),
        "rate_limit off should clear inherited profiles"
    );
    h.finish();
}

#[tokio::test]
async fn rate_limit_custom_response_policy_marks_limited_response() {
    api! {
        client RateLimitCustomResponseApi {
            base https "example.com"
            observe rate_limit HeaderScopePolicy
            default {
                rate_limit app
            }
            rate_limit app {
                    bucket application by [host] {
                        500 / 10s
                    }
            }
            rate_limit method_read {
                    bucket method by [host, endpoint] {
                        30 / 10s
                    }
            }
        }

        GET Limited
        -> Json<()>
        {
            rate_limit method_read
        }
    }

    use rate_limit_custom_response_api::*;

    let throttled = MockReply::status(http::StatusCode::TOO_MANY_REQUESTS)
        .with_header(
            http::header::RETRY_AFTER,
            http::HeaderValue::from_static("1"),
        )
        .with_header(
            http::HeaderName::from_static("x-limit-scope"),
            http::HeaderValue::from_static("method"),
        );
    let (transport, h) = mock().reply(throttled).build();
    let api = RateLimitCustomResponseApi::new_with_transport(transport);

    let err = api
        .request(endpoints::Limited::new())
        .execute()
        .await
        .expect_err("429 should be returned as an HTTP status error");

    match err {
        ApiClientError::HttpStatus {
            rate_limit: Some(action),
            ..
        } => {
            assert!(action.is_limited());
            assert_eq!(action.retry_after(), Some(Duration::from_secs(1)));
            assert!(
                action.cooldown_stored(),
                "the method bucket exists, so the limiter should store the cooldown"
            );
        }
        other => panic!("unexpected error: {other:?}"),
    }

    h.finish();
}

#[tokio::test]
async fn rate_limit_response_bucket_scope_falls_back_when_bucket_is_missing() {
    api! {
        client RateLimitMissingBucketFallbackApi {
            base https "example.com"
            observe rate_limit HeaderScopePolicy
        }

        GET NoBucket
        -> Json<()>
        {
            rate_limit off
        }
    }

    use rate_limit_missing_bucket_fallback_api::*;

    let throttled = MockReply::status(http::StatusCode::TOO_MANY_REQUESTS)
        .with_header(
            http::HeaderName::from_static("x-rate-limit-type"),
            http::HeaderValue::from_static("method"),
        )
        .with_header(
            http::header::RETRY_AFTER,
            http::HeaderValue::from_static("1"),
        );
    let (transport, h) = mock()
        .replies([throttled, MockReply::ok_json(json_bytes(&()))])
        .build();
    let api = RateLimitMissingBucketFallbackApi::new_with_transport(transport);

    let first = api
        .request(endpoints::NoBucket::new())
        .execute()
        .await
        .expect_err("first request should receive the scripted 429");

    match first {
        ApiClientError::HttpStatus {
            rate_limit: Some(action),
            ..
        } => {
            assert!(action.cooldown_stored());
        }
        other => panic!("unexpected error: {other:?}"),
    }

    let started = Instant::now();
    api.request(endpoints::NoBucket::new())
        .execute()
        .await
        .unwrap();
    assert!(
        started.elapsed() >= Duration::from_millis(25),
        "endpoint fallback cooldown should be checked even when the request has no buckets"
    );

    h.finish();
}

#[tokio::test]
async fn retry_does_not_duplicate_delay_when_rate_limiter_stores_cooldown() {
    api! {
        client RateLimitRetryCoordinationApi {
            base https "example.com"
            default {
                retry read
            }
            retry read {
                    attempts 2
                    methods [GET]
                    on [429]
                    retry_after
            }
            observe rate_limit HeaderScopePolicy
            rate_limit method_read {
                    bucket method by [host, endpoint] {
                        30 / 10s
                    }
            }
        }

        GET Limited
        -> Json<()>
        {
            rate_limit method_read
        }
    }

    use rate_limit_retry_coordination_api::*;

    let throttled = MockReply::status(http::StatusCode::TOO_MANY_REQUESTS)
        .with_header(
            http::header::RETRY_AFTER,
            http::HeaderValue::from_static("1"),
        )
        .with_header(
            http::HeaderName::from_static("x-rate-limit-type"),
            http::HeaderValue::from_static("method"),
        );
    let (transport, h) = mock()
        .replies([throttled, MockReply::ok_json(json_bytes(&()))])
        .build();
    let api = RateLimitRetryCoordinationApi::new_with_transport(transport);

    let started = Instant::now();
    api.request(endpoints::Limited::new())
        .execute()
        .await
        .unwrap();
    let elapsed = started.elapsed();

    assert!(
        elapsed >= Duration::from_millis(900),
        "retry should still pass through the limiter cooldown"
    );
    assert!(
        elapsed < Duration::from_millis(1500),
        "retry must not also sleep the HTTP Retry-After header when limiter stored the cooldown"
    );

    let reqs = h.recorded();
    assert_eq!(reqs.len(), 2);
    assert_eq!(reqs[0].meta.attempt, 0);
    assert_eq!(reqs[1].meta.attempt, 1);
    h.finish();
}

#[tokio::test]
async fn rate_limit_scope_key_binding_materializes_param_key() {
    api! {
        client RateLimitScopeKeyApi {
            base https "example.com"
            default {
                rate_limit app
            }
            rate_limit app {
                    bucket application by [host] {
                        500 / 10s
                    }
            }
            rate_limit regional_method {
                    bucket method by [region, endpoint] {
                        1600 / 1m
                    }
            }
        }

        scope platform(platform: String) {
            host [platform, "api"]
            rate_limit key region = platform

            GET ByRegion
            -> Json<()>
            {
                rate_limit regional_method
            }
        }
    }

    use rate_limit_scope_key_api::*;

    let (transport, h) = mock().reply(MockReply::ok_json(json_bytes(&()))).build();
    let limiter = RecordingLimiter::default();
    let plans = limiter.plans.clone();
    let api = RateLimitScopeKeyApi::new_with_transport(transport).with_configure(|cfg| {
        cfg.rate_limiter(Arc::new(limiter));
    });

    api.request(endpoints::platform::ByRegion::new("euw1".to_string()))
        .execute()
        .await
        .unwrap();

    let plans = plans.lock().expect("plan lock").clone();
    assert_eq!(plans.len(), 1);
    let method_bucket = &plans[0].buckets()[1];
    let region_part = &method_bucket.key.parts()[0];
    assert_eq!(region_part.name, "region");
    match &region_part.value {
        RateLimitKeyValue::Static(value) => assert_eq!(value.as_ref(), "euw1"),
        other => panic!("expected static region key, got {other:?}"),
    }
    h.finish();
}

#[tokio::test]
async fn inflight_followers_do_not_consume_rate_limit_permits() {
    api! {
        client RateLimitInflightApi {
            base https "example.com"
            default {
                rate_limit app
            }
            rate_limit app {
                    bucket application by [host] {
                        500 / 10s
                    }
            }
        }

        GET Ping
        -> Json<()>
        {
        }
    }

    use rate_limit_inflight_api::*;

    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let transport_calls = Arc::new(AtomicUsize::new(0));
    let transport = SlowTransport {
        started: started.clone(),
        release: release.clone(),
        calls: transport_calls.clone(),
    };
    let limiter = RecordingLimiter::default();
    let plans = limiter.plans.clone();
    let api = RateLimitInflightApi::new_with_transport(transport).with_configure(|cfg| {
        cfg.rate_limiter(Arc::new(limiter));
        cfg.inflight_policy(Arc::new(SafeMethodInflightPolicy));
    });

    let api_a = api.clone();
    let first = tokio::spawn(async move { api_a.request(endpoints::Ping::new()).execute().await });
    started.notified().await;

    let api_b = api.clone();
    let second = tokio::spawn(async move { api_b.request(endpoints::Ping::new()).execute().await });
    tokio::task::yield_now().await;
    release.notify_waiters();

    first.await.expect("first task").unwrap();
    second.await.expect("second task").unwrap();

    assert_eq!(transport_calls.load(Ordering::SeqCst), 1);
    assert_eq!(plans.lock().expect("plan lock").len(), 1);
}

#[tokio::test]
async fn cache_hits_do_not_consume_rate_limit_permits() {
    api! {
        client RateLimitCacheApi {
            base https "example.com"
            default {
                rate_limit app
            }
            rate_limit app {
                    bucket application by [host] {
                        500 / 10s
                    }
            }
        }

        GET Cached
        -> Json<()>
        {
        }
    }

    use rate_limit_cache_api::*;

    let (transport, h) = mock().build();
    let limiter = RecordingLimiter::default();
    let plans = limiter.plans.clone();
    let api = RateLimitCacheApi::new_with_transport(transport).with_configure(|cfg| {
        cfg.rate_limiter(Arc::new(limiter));
        cfg.cache_store(Arc::new(AlwaysHitCache::default()));
    });

    api.request(endpoints::Cached::new())
        .execute()
        .await
        .unwrap();

    assert!(plans.lock().expect("plan lock").is_empty());
    assert_eq!(h.recorded_len(), 0);
    h.finish();
}

#[tokio::test]
async fn duplicate_rate_limit_profiles_do_not_duplicate_buckets_after_canonicalization() {
    api! {
        client RateLimitCanonicalizationApi {
            base https "example.com"
            default {
                rate_limit app
            }
            rate_limit app {
                    bucket application by [host] {
                        500 / 10s
                    }
            }
        }

        GET Ping
        -> Json<()>
        {
            rate_limit app
        }
    }

    use rate_limit_canonicalization_api::*;

    let (transport, h) = mock().reply(MockReply::ok_json(json_bytes(&()))).build();
    let limiter = RecordingLimiter::default();
    let plans = limiter.plans.clone();
    let api = RateLimitCanonicalizationApi::new_with_transport(transport).with_configure(|cfg| {
        cfg.rate_limiter(Arc::new(limiter));
    });

    api.request(endpoints::Ping::new()).execute().await.unwrap();

    let plans = plans.lock().expect("plan lock").clone();
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].buckets().len(), 1);
    assert_eq!(plans[0].buckets()[0].id.kind, "application");
    h.finish();
}

#[tokio::test]
async fn endpoint_rate_limit_key_binding_materializes_scope_param_key() {
    api! {
        client RateLimitEndpointKeyApi {
            base https "example.com"
            rate_limit regional_method {
                    bucket method by [region, endpoint] {
                        1600 / 1m
                    }
            }
        }

        scope platform(platform: String) {
            host [platform, "api"]

            GET ByRegion
            -> Json<()>
            {
                rate_limit key region = platform
                rate_limit regional_method
            }
        }
    }

    use rate_limit_endpoint_key_api::*;

    let (transport, h) = mock().reply(MockReply::ok_json(json_bytes(&()))).build();
    let limiter = RecordingLimiter::default();
    let plans = limiter.plans.clone();
    let api = RateLimitEndpointKeyApi::new_with_transport(transport).with_configure(|cfg| {
        cfg.rate_limiter(Arc::new(limiter));
    });

    api.request(endpoints::platform::ByRegion::new("euw1".to_string()))
        .execute()
        .await
        .unwrap();

    let plans = plans.lock().expect("plan lock").clone();
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].buckets().len(), 1);
    let region_part = &plans[0].buckets()[0].key.parts()[0];
    assert_eq!(region_part.name, "region");
    match &region_part.value {
        RateLimitKeyValue::Static(value) => assert_eq!(value.as_ref(), "euw1"),
        other => panic!("expected static region key, got {other:?}"),
    }
    h.finish();
}

#[tokio::test]
async fn rate_limit_bucket_cost_is_emitted_to_runtime_plan() {
    api! {
        client RateLimitCostApi {
            base https "example.com"
            default {
                rate_limit weighted
            }
            rate_limit weighted {
                    bucket method by [host, endpoint] {
                        cost 3
                        30 / 10s
                    }
            }
        }

        GET Ping
        -> Json<()>
        {
        }
    }

    use rate_limit_cost_api::*;

    let (transport, h) = mock().reply(MockReply::ok_json(json_bytes(&()))).build();
    let limiter = RecordingLimiter::default();
    let plans = limiter.plans.clone();
    let api = RateLimitCostApi::new_with_transport(transport).with_configure(|cfg| {
        cfg.rate_limiter(Arc::new(limiter));
    });

    api.request(endpoints::Ping::new()).execute().await.unwrap();

    let plans = plans.lock().expect("plan lock").clone();
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].buckets().len(), 1);
    assert_eq!(plans[0].buckets()[0].cost.get(), 3);
    h.finish();
}

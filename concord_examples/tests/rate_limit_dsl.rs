use bytes::Bytes;
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

impl RateLimitResponsePolicy for HeaderScopePolicy {
    fn observe(&self, ctx: &RateLimitResponseContext<'_>) -> RateLimitObservation {
        if ctx.status != http::StatusCode::TOO_MANY_REQUESTS {
            return RateLimitObservation::continue_();
        }

        let target = ctx
            .headers
            .get(http::HeaderName::from_static("x-limit-scope"))
            .and_then(|value| value.to_str().ok())
            .map(|value| match value.trim() {
                "application" => RateLimitTarget::bucket_kind("application", RateLimitTarget::Host),
                "method" => RateLimitTarget::bucket_kind("method", RateLimitTarget::Endpoint),
                _ => RateLimitTarget::current_plan_or_endpoint(),
            })
            .unwrap_or_else(RateLimitTarget::current_plan_or_endpoint);

        let delay = ctx
            .headers
            .get(http::HeaderName::from_static("x-delay-ms"))
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            .map(Duration::from_millis)
            .or_else(|| parse_retry_after(ctx.headers));

        let mut observation = RateLimitObservation::limited().with_target(target);
        if let Some(delay) = delay {
            observation = observation.with_delay(delay);
        }
        observation
    }
}

#[tokio::test]
async fn rate_limit_profiles_generate_request_plan_and_allow_custom_limiter() {
    api! {
        client RateLimitDslApi {
            scheme: https,
            host: "example.com",
            rate_limit {
                profile app {
                    bucket application by [route.host] {
                        limit 500 every 10 seconds
                    }
                }
                profile method_read {
                    bucket method by [route.host, endpoint] {
                        limit 30 every 10 seconds
                        limit 500 every 10 minutes
                    }
                }
                default app
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
    let api = RateLimitDslApi::new_with_transport(transport).with_rate_limiter(Arc::new(limiter));

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
            scheme: https,
            host: "example.com",
            rate_limit {
                response custom HeaderScopePolicy

                profile app {
                    bucket application by [route.host] {
                        limit 500 every 10 seconds
                    }
                }
                profile method_read {
                    bucket method by [route.host, endpoint] {
                        limit 30 every 10 seconds
                    }
                }
                default app
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
            scheme: https,
            host: "example.com",
            rate_limit {
                response custom HeaderScopePolicy
            }
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
            http::HeaderName::from_static("x-limit-scope"),
            http::HeaderValue::from_static("method"),
        )
        .with_header(
            http::HeaderName::from_static("x-delay-ms"),
            http::HeaderValue::from_static("40"),
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
            scheme: https,
            host: "example.com",
            retry {
                profile read {
                    attempts 2
                    methods [GET]
                    on status[429]
                    retry_after honor
                    backoff none
                }
                default read
            }
            rate_limit {
                response custom HeaderScopePolicy

                profile method_read {
                    bucket method by [route.host, endpoint] {
                        limit 30 every 10 seconds
                    }
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
            http::HeaderName::from_static("x-limit-scope"),
            http::HeaderValue::from_static("method"),
        )
        .with_header(
            http::HeaderName::from_static("x-delay-ms"),
            http::HeaderValue::from_static("40"),
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
        elapsed >= Duration::from_millis(25),
        "retry should still pass through the limiter cooldown"
    );
    assert!(
        elapsed < Duration::from_millis(500),
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
            scheme: https,
            host: "example.com",
            rate_limit {
                profile app {
                    bucket application by [route.host] {
                        limit 500 every 10 seconds
                    }
                }
                profile regional_method {
                    bucket method by [region, endpoint] {
                        limit 1600 every 1 minute
                    }
                }
                default app
            }
        }

        scope platform(platform: String) {
            host[platform, "api"]
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
    let api =
        RateLimitScopeKeyApi::new_with_transport(transport).with_rate_limiter(Arc::new(limiter));

    api.request(endpoints::ByRegion::new("euw1".to_string()))
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
            scheme: https,
            host: "example.com",
            rate_limit {
                profile app {
                    bucket application by [route.host] {
                        limit 500 every 10 seconds
                    }
                }
                default app
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
    let api = RateLimitInflightApi::new_with_transport(transport)
        .with_rate_limiter(Arc::new(limiter))
        .with_inflight_policy(Arc::new(SafeMethodInflightPolicy));

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
            scheme: https,
            host: "example.com",
            rate_limit {
                profile app {
                    bucket application by [route.host] {
                        limit 500 every 10 seconds
                    }
                }
                default app
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
    let api = RateLimitCacheApi::new_with_transport(transport)
        .with_rate_limiter(Arc::new(limiter))
        .with_cache_store(Arc::new(AlwaysHitCache::default()));

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
            scheme: https,
            host: "example.com",
            rate_limit {
                profile app {
                    bucket application by [route.host] {
                        limit 500 every 10 seconds
                    }
                }
                default app
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
    let api = RateLimitCanonicalizationApi::new_with_transport(transport)
        .with_rate_limiter(Arc::new(limiter));

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
            scheme: https,
            host: "example.com",
            rate_limit {
                profile regional_method {
                    bucket method by [region, endpoint] {
                        limit 1600 every 1 minute
                    }
                }
            }
        }

        scope platform(platform: String) {
            host[platform, "api"]

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
    let api =
        RateLimitEndpointKeyApi::new_with_transport(transport).with_rate_limiter(Arc::new(limiter));

    api.request(endpoints::ByRegion::new("euw1".to_string()))
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
            scheme: https,
            host: "example.com",
            rate_limit {
                profile weighted {
                    bucket method by [route.host, endpoint] {
                        cost 3
                        limit 30 every 10 seconds
                    }
                }
                default weighted
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
    let api = RateLimitCostApi::new_with_transport(transport).with_rate_limiter(Arc::new(limiter));

    api.request(endpoints::Ping::new()).execute().await.unwrap();

    let plans = plans.lock().expect("plan lock").clone();
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].buckets().len(), 1);
    assert_eq!(plans[0].buckets()[0].cost.get(), 3);
    h.finish();
}

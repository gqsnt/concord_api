use concord_core::prelude::*;
use concord_core::transport::{
    BuiltRequest, BuiltResponse, TransportBody, TransportError, TransportErrorKind,
    TransportResponse,
};
use concord_macros::api;
use concord_test_support::*;
use http::header::{AUTHORIZATION, CACHE_CONTROL, ETAG, IF_NONE_MATCH, VARY};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

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

#[derive(Clone, Default)]
struct FailingTransport {
    calls: Arc<AtomicUsize>,
}

impl Transport for FailingTransport {
    fn send(
        &self,
        _req: BuiltRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send>> {
        let calls = self.calls.clone();
        Box::pin(async move {
            calls.fetch_add(1, Ordering::SeqCst);
            Err(TransportError::with_kind(
                TransportErrorKind::Timeout,
                std::io::Error::new(std::io::ErrorKind::TimedOut, "revalidation timed out"),
            ))
        })
    }
}

#[derive(Default)]
struct RevalidateThenStaleCache;

impl CacheStore for RevalidateThenStaleCache {
    fn before_request<'a>(
        &'a self,
        request: &'a BuiltRequest,
    ) -> Pin<Box<dyn Future<Output = CacheBefore> + Send + 'a>> {
        Box::pin(async move {
            let mut headers = request.headers.clone();
            headers.insert(IF_NONE_MATCH, http::HeaderValue::from_static("\"etag-1\""));
            CacheBefore::Revalidate {
                request_headers: headers,
                cached: CacheRevalidation {
                    key: CacheKey::new("stale".to_string()),
                    cached_response: BuiltResponse {
                        meta: request.meta.clone(),
                        url: request.url.clone(),
                        status: http::StatusCode::OK,
                        headers: json_headers(),
                        body: json_bytes(&"stale".to_string()),
                        rate_limit: request.rate_limit.clone(),
                    },
                },
            }
        })
    }

    fn after_error<'a>(
        &'a self,
        _request: &'a BuiltRequest,
        _error: &'a ApiClientError,
        revalidation: Option<CacheRevalidation>,
    ) -> Pin<Box<dyn Future<Output = Option<BuiltResponse>> + Send + 'a>> {
        Box::pin(async move { revalidation.map(|cached| cached.cached_response) })
    }
}

#[derive(Default)]
struct RevalidateWithoutMergeCache;

impl CacheStore for RevalidateWithoutMergeCache {
    fn before_request<'a>(
        &'a self,
        request: &'a BuiltRequest,
    ) -> Pin<Box<dyn Future<Output = CacheBefore> + Send + 'a>> {
        Box::pin(async move {
            let mut headers = request.headers.clone();
            headers.insert(IF_NONE_MATCH, http::HeaderValue::from_static("\"etag-1\""));
            CacheBefore::Revalidate {
                request_headers: headers,
                cached: CacheRevalidation {
                    key: CacheKey::new("missing".to_string()),
                    cached_response: BuiltResponse {
                        meta: request.meta.clone(),
                        url: request.url.clone(),
                        status: http::StatusCode::OK,
                        headers: json_headers(),
                        body: json_bytes(&"stale".to_string()),
                        rate_limit: request.rate_limit.clone(),
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
    ) -> Pin<Box<dyn Future<Output = CacheAfter> + Send + 'a>> {
        Box::pin(async move { CacheAfter::NotStored(CacheSkipReason::Backend) })
    }
}

struct OneChunkBody {
    chunk: Option<bytes::Bytes>,
}

impl TransportBody for OneChunkBody {
    fn next_chunk<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<bytes::Bytes>, TransportError>> + Send + 'a>>
    {
        Box::pin(async move { Ok(self.chunk.take()) })
    }
}

#[derive(Clone, Default)]
struct FirstOkThenTimeoutTransport {
    calls: Arc<AtomicUsize>,
}

impl Transport for FirstOkThenTimeoutTransport {
    fn send(
        &self,
        req: BuiltRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send>> {
        let calls = self.calls.clone();
        Box::pin(async move {
            let call = calls.fetch_add(1, Ordering::SeqCst);
            if call == 0 {
                let mut headers = json_headers();
                headers.insert(CACHE_CONTROL, http::HeaderValue::from_static("max-age=0"));
                headers.insert(ETAG, http::HeaderValue::from_static("\"etag-1\""));
                let body = json_bytes(&"first".to_string());
                return Ok(TransportResponse {
                    meta: req.meta,
                    url: req.url,
                    status: http::StatusCode::OK,
                    headers,
                    content_length: Some(body.len() as u64),
                    rate_limit: req.rate_limit,
                    body: Box::new(OneChunkBody { chunk: Some(body) }),
                });
            }
            Err(TransportError::with_kind(
                TransportErrorKind::Timeout,
                std::io::Error::new(std::io::ErrorKind::TimedOut, "revalidation timed out"),
            ))
        })
    }
}

fn json_headers() -> http::HeaderMap {
    let mut headers = http::HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("application/json"),
    );
    headers
}

#[tokio::test]
async fn cache_bypass_hits_transport_without_overwriting_cached_value() {
    api! {
        client CacheBypassApi {
            scheme: https,
            host: "example.com",
            cache {
                profile short { ttl 60 seconds }
                default short
            }
        }

        GET Cached
        -> Json<String>
        {
            path["cached"]
        }
    }

    use cache_bypass_api::*;

    let (transport, h) = mock()
        .reply(
            MockReply::ok_json(json_bytes(&"first".to_string()))
                .with_header(CACHE_CONTROL, http::HeaderValue::from_static("max-age=60")),
        )
        .reply(
            MockReply::ok_json(json_bytes(&"bypass".to_string()))
                .with_header(CACHE_CONTROL, http::HeaderValue::from_static("max-age=60")),
        )
        .build();

    let api = CacheBypassApi::new_with_transport(transport);
    let first = api
        .request(endpoints::Cached::new())
        .execute()
        .await
        .unwrap();
    let bypass = api
        .request(endpoints::Cached::new())
        .cache_bypass()
        .execute()
        .await
        .unwrap();
    let third = api
        .request(endpoints::Cached::new())
        .execute()
        .await
        .unwrap();

    assert_eq!(first, "first");
    assert_eq!(bypass, "bypass");
    assert_eq!(third, "first");
    h.assert_recorded_len(2);
    h.finish();
}

#[tokio::test]
async fn cache_refresh_hits_transport_and_updates_cached_value() {
    api! {
        client CacheRefreshApi {
            scheme: https,
            host: "example.com",
            cache {
                profile short { ttl 60 seconds }
                default short
            }
        }

        GET Cached
        -> Json<String>
        {
            path["cached"]
        }
    }

    use cache_refresh_api::*;

    let (transport, h) = mock()
        .reply(
            MockReply::ok_json(json_bytes(&"first".to_string()))
                .with_header(CACHE_CONTROL, http::HeaderValue::from_static("max-age=60")),
        )
        .reply(
            MockReply::ok_json(json_bytes(&"refreshed".to_string()))
                .with_header(CACHE_CONTROL, http::HeaderValue::from_static("max-age=60")),
        )
        .build();

    let api = CacheRefreshApi::new_with_transport(transport);
    let first = api
        .request(endpoints::Cached::new())
        .execute()
        .await
        .unwrap();
    let refreshed = api
        .request(endpoints::Cached::new())
        .cache_refresh()
        .execute()
        .await
        .unwrap();
    let third = api
        .request(endpoints::Cached::new())
        .execute()
        .await
        .unwrap();

    assert_eq!(first, "first");
    assert_eq!(refreshed, "refreshed");
    assert_eq!(third, "refreshed");
    h.assert_recorded_len(2);
    h.finish();
}

#[tokio::test]
async fn cache_profile_revalidate_false_skips_conditional_headers() {
    api! {
        client CacheNoRevalidateApi {
            scheme: https,
            host: "example.com",
            cache {
                profile read {
                    ttl 60 seconds
                    revalidate false
                }
                default read
            }
        }

        GET Cached
        -> Json<String>
        {
            path["cached"]
        }
    }

    use cache_no_revalidate_api::*;

    let (transport, h) = mock()
        .reply(
            MockReply::ok_json(json_bytes(&"first".to_string()))
                .with_header(CACHE_CONTROL, http::HeaderValue::from_static("max-age=0"))
                .with_header(ETAG, http::HeaderValue::from_static("\"etag-1\"")),
        )
        .reply(
            MockReply::ok_json(json_bytes(&"second".to_string()))
                .with_header(CACHE_CONTROL, http::HeaderValue::from_static("max-age=60")),
        )
        .build();

    let api = CacheNoRevalidateApi::new_with_transport(transport);
    let first = api
        .request(endpoints::Cached::new())
        .execute()
        .await
        .unwrap();
    let second = api
        .request(endpoints::Cached::new())
        .execute()
        .await
        .unwrap();

    assert_eq!(first, "first");
    assert_eq!(second, "second");
    h.assert_recorded_len(2);
    let reqs = h.recorded();
    assert_eq!(reqs[1].headers.get(IF_NONE_MATCH), None);
    h.finish();
}

#[tokio::test]
async fn cache_profile_on_error_serve_stale_returns_stale_after_revalidation_error() {
    api! {
        client CacheServeStaleApi {
            scheme: https,
            host: "example.com",
            cache {
                profile read {
                    ttl 60 seconds
                    on_error serve_stale
                }
                default read
            }
        }

        GET Cached
        -> Json<String>
        {
            path["cached"]
        }
    }

    use cache_serve_stale_api::*;

    let transport = FirstOkThenTimeoutTransport::default();
    let calls = transport.calls.clone();
    let api = CacheServeStaleApi::new_with_transport(transport);

    let first = api
        .request(endpoints::Cached::new())
        .execute()
        .await
        .unwrap();
    let second = api
        .request(endpoints::Cached::new())
        .execute()
        .await
        .unwrap();

    assert_eq!(first, "first");
    assert_eq!(second, "first");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn revalidation_304_without_cache_merge_retries_once_with_unconditional_fetch() {
    api! {
        client CacheRevalidationFallbackApi {
            scheme: https,
            host: "example.com",
        }

        GET Cached
        -> Json<String>
        {
            path["cached"]
        }
    }

    use cache_revalidation_fallback_api::*;

    let (transport, h) = mock()
        .reply(
            MockReply::status(http::StatusCode::NOT_MODIFIED)
                .with_header(ETAG, http::HeaderValue::from_static("\"etag-1\"")),
        )
        .reply(MockReply::ok_json(json_bytes(&"fresh".to_string())))
        .build();

    let api = CacheRevalidationFallbackApi::new_with_transport(transport)
        .with_cache_store(Arc::new(RevalidateWithoutMergeCache));

    let value = api
        .request(endpoints::Cached::new())
        .execute()
        .await
        .unwrap();
    assert_eq!(value, "fresh");
    h.assert_recorded_len(2);
    let reqs = h.recorded();
    assert_eq!(
        reqs[0].headers.get(IF_NONE_MATCH),
        Some(&http::HeaderValue::from_static("\"etag-1\""))
    );
    assert_eq!(reqs[1].headers.get(IF_NONE_MATCH), None);
    h.finish();
}

#[tokio::test]
async fn cache_profile_fresh_hit_skips_transport() {
    api! {
        client CacheFreshApi {
            scheme: https,
            host: "example.com",
            cache {
                profile short {
                    ttl 60 seconds
                }
                default short
            }
        }

        GET Cached
        -> Json<String>
        {
            path["cached"]
        }
    }

    use cache_fresh_api::*;

    let (transport, h) = mock()
        .reply(
            MockReply::ok_json(json_bytes(&"first".to_string()))
                .with_header(CACHE_CONTROL, http::HeaderValue::from_static("max-age=60")),
        )
        .build();

    let api = CacheFreshApi::new_with_transport(transport);
    let first = api
        .request(endpoints::Cached::new())
        .execute()
        .await
        .unwrap();
    let second = api
        .request(endpoints::Cached::new())
        .execute()
        .await
        .unwrap();

    assert_eq!(first, "first");
    assert_eq!(second, "first");
    h.assert_recorded_len(1);
    h.finish();
}

#[tokio::test]
async fn endpoint_inline_cache_sets_up_default_backend_without_client_cache_block() {
    api! {
        client EndpointOnlyCacheApi {
            scheme: https,
            host: "example.com",
        }

        GET Cached
        -> Json<String>
        {
            path["cached"]
            cache {
                ttl 60 seconds
                max_body 2 mib
            }
        }
    }

    use endpoint_only_cache_api::*;

    let (transport, h) = mock()
        .reply(MockReply::ok_json(json_bytes(&"first".to_string())))
        .build();

    let api = EndpointOnlyCacheApi::new_with_transport(transport);
    let first = api
        .request(endpoints::Cached::new())
        .execute()
        .await
        .unwrap();
    let second = api
        .request(endpoints::Cached::new())
        .execute()
        .await
        .unwrap();

    assert_eq!(first, "first");
    assert_eq!(second, "first");
    h.assert_recorded_len(1);
    h.finish();
}

#[tokio::test]
async fn cache_inline_patch_inherits_profile_ttl_and_overrides_max_body() {
    api! {
        client CacheInlinePatchApi {
            scheme: https,
            host: "example.com",
            cache {
                profile tiny {
                    ttl 60 seconds
                    max_body 1 bytes
                }
                default tiny
            }
        }

        GET Cached
        -> Json<String>
        {
            path["cached"]
            cache {
                max_body 2 mib
            }
        }
    }

    use cache_inline_patch_api::*;

    let (transport, h) = mock()
        .reply(MockReply::ok_json(json_bytes(&"first".to_string())))
        .build();

    let api = CacheInlinePatchApi::new_with_transport(transport);
    let first = api
        .request(endpoints::Cached::new())
        .execute()
        .await
        .unwrap();
    let second = api
        .request(endpoints::Cached::new())
        .execute()
        .await
        .unwrap();

    assert_eq!(first, "first");
    assert_eq!(second, "first");
    h.assert_recorded_len(1);
    h.finish();
}

#[tokio::test]
async fn cache_profile_http_capacity_and_max_body_are_honored() {
    api! {
        client CacheFullProfileApi {
            scheme: https,
            host: "example.com",
            cache {
                profile http_profile {
                    http
                    ttl 60 seconds
                    capacity 64 mib
                    max_body 1 bytes
                    revalidate true
                    shared false
                }
                default http_profile
            }
        }

        GET Cached
        -> Json<String>
        {
            path["cached"]
        }
    }

    use cache_full_profile_api::*;

    let (transport, h) = mock()
        .reply(
            MockReply::ok_json(json_bytes(&"first".to_string()))
                .with_header(CACHE_CONTROL, http::HeaderValue::from_static("max-age=60")),
        )
        .reply(
            MockReply::ok_json(json_bytes(&"second".to_string()))
                .with_header(CACHE_CONTROL, http::HeaderValue::from_static("max-age=60")),
        )
        .build();

    let api = CacheFullProfileApi::new_with_transport(transport);
    let first = api
        .request(endpoints::Cached::new())
        .execute()
        .await
        .unwrap();
    let second = api
        .request(endpoints::Cached::new())
        .execute()
        .await
        .unwrap();

    assert_eq!(first, "first");
    assert_eq!(second, "second");
    h.assert_recorded_len(2);
    h.finish();
}

#[tokio::test]
async fn cache_hit_skips_rate_limit_after_initial_store() {
    api! {
        client CacheRateLimitApi {
            scheme: https,
            host: "example.com",
            cache {
                profile short {
                    ttl 60 seconds
                }
                default short
            }
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
        -> Json<String>
        {
            path["cached"]
        }
    }

    use cache_rate_limit_api::*;

    let (transport, h) = mock()
        .reply(
            MockReply::ok_json(json_bytes(&"first".to_string()))
                .with_header(CACHE_CONTROL, http::HeaderValue::from_static("max-age=60")),
        )
        .build();
    let limiter = RecordingLimiter::default();
    let plans = limiter.plans.clone();
    let api = CacheRateLimitApi::new_with_transport(transport).with_rate_limiter(Arc::new(limiter));

    let first = api
        .request(endpoints::Cached::new())
        .execute()
        .await
        .unwrap();
    let second = api
        .request(endpoints::Cached::new())
        .execute()
        .await
        .unwrap();

    assert_eq!(first, "first");
    assert_eq!(second, "first");
    assert_eq!(plans.lock().expect("plan lock").len(), 1);
    h.assert_recorded_len(1);
    h.finish();
}

#[tokio::test]
async fn cache_hit_skips_retry_and_transport_after_initial_store() {
    api! {
        client CacheRetryApi {
            scheme: https,
            host: "example.com",
            cache {
                profile short {
                    ttl 60 seconds
                }
                default short
            }
            retry {
                profile read {
                    attempts 2
                    methods [GET]
                    on status[500]
                    backoff none
                }
                default read
            }
        }

        GET Cached
        -> Json<String>
        {
            path["cached"]
        }
    }

    use cache_retry_api::*;

    let (transport, h) = mock()
        .reply(
            MockReply::ok_json(json_bytes(&"first".to_string()))
                .with_header(CACHE_CONTROL, http::HeaderValue::from_static("max-age=60")),
        )
        .build();
    let api = CacheRetryApi::new_with_transport(transport);

    let first = api
        .request(endpoints::Cached::new())
        .execute()
        .await
        .unwrap();
    let second = api
        .request(endpoints::Cached::new())
        .execute()
        .await
        .unwrap();

    assert_eq!(first, "first");
    assert_eq!(second, "first");
    h.assert_recorded_len(1);
    h.finish();
}

#[tokio::test]
async fn cache_control_no_store_is_not_stored() {
    api! {
        client CacheNoStoreApi {
            scheme: https,
            host: "example.com",
            cache {
                profile short {
                    ttl 60 seconds
                }
                default short
            }
        }

        GET Cached
        -> Json<String>
        {
            path["cached"]
        }
    }

    use cache_no_store_api::*;

    let (transport, h) = mock()
        .reply(
            MockReply::ok_json(json_bytes(&"first".to_string()))
                .with_header(CACHE_CONTROL, http::HeaderValue::from_static("no-store")),
        )
        .reply(
            MockReply::ok_json(json_bytes(&"second".to_string()))
                .with_header(CACHE_CONTROL, http::HeaderValue::from_static("max-age=60")),
        )
        .build();

    let api = CacheNoStoreApi::new_with_transport(transport);
    let first = api
        .request(endpoints::Cached::new())
        .execute()
        .await
        .unwrap();
    let second = api
        .request(endpoints::Cached::new())
        .execute()
        .await
        .unwrap();

    assert_eq!(first, "first");
    assert_eq!(second, "second");
    h.assert_recorded_len(2);
    h.finish();
}

#[tokio::test]
async fn cache_vary_header_keeps_variants_separate() {
    api! {
        client CacheVaryApi {
            scheme: https,
            host: "example.com",
            cache {
                profile short {
                    ttl 60 seconds
                }
                default short
            }
        }

        GET Localized(lang: String)
        -> Json<String>
        {
            path["localized"]
            headers { "accept-language" = lang }
        }
    }

    use cache_vary_api::*;

    let (transport, h) = mock()
        .reply(
            MockReply::ok_json(json_bytes(&"hello".to_string()))
                .with_header(CACHE_CONTROL, http::HeaderValue::from_static("max-age=60"))
                .with_header(VARY, http::HeaderValue::from_static("accept-language")),
        )
        .reply(
            MockReply::ok_json(json_bytes(&"bonjour".to_string()))
                .with_header(CACHE_CONTROL, http::HeaderValue::from_static("max-age=60"))
                .with_header(VARY, http::HeaderValue::from_static("accept-language")),
        )
        .build();

    let api = CacheVaryApi::new_with_transport(transport);
    let en1 = api
        .request(endpoints::Localized::new("en-US".to_string()))
        .execute()
        .await
        .unwrap();
    let fr = api
        .request(endpoints::Localized::new("fr-FR".to_string()))
        .execute()
        .await
        .unwrap();
    let en2 = api
        .request(endpoints::Localized::new("en-US".to_string()))
        .execute()
        .await
        .unwrap();

    assert_eq!(en1, "hello");
    assert_eq!(fr, "bonjour");
    assert_eq!(en2, "hello");
    h.assert_recorded_len(2);
    h.finish();
}

#[tokio::test]
async fn authenticated_cache_keys_are_isolated_by_auth_identity() {
    api! {
        client CacheAuthApi {
            scheme: https,
            host: "example.com",
            secret {
                api_key: String
            }
            auth {
                credential api_key: ApiKey(secret.api_key)
            }
            use_auth HeaderAuth("Authorization", api_key)
            cache {
                profile short {
                    ttl 60 seconds
                }
                default short
            }
        }

        GET Cached
        -> Json<String>
        {
            path["cached"]
        }
    }

    use cache_auth_api::*;

    let (transport, h) = mock()
        .reply(
            MockReply::ok_json(json_bytes(&"token-one".to_string()))
                .with_header(CACHE_CONTROL, http::HeaderValue::from_static("max-age=60")),
        )
        .reply(
            MockReply::ok_json(json_bytes(&"token-two".to_string()))
                .with_header(CACHE_CONTROL, http::HeaderValue::from_static("max-age=60")),
        )
        .build();

    let mut api = CacheAuthApi::new_with_transport("one".to_string(), transport);
    let first = api
        .request(endpoints::Cached::new())
        .execute()
        .await
        .unwrap();
    api.set_api_key("two");
    let second = api
        .request(endpoints::Cached::new())
        .execute()
        .await
        .unwrap();
    api.set_api_key("one");
    let third = api
        .request(endpoints::Cached::new())
        .execute()
        .await
        .unwrap();

    assert_eq!(first, "token-one");
    assert_eq!(second, "token-two");
    assert_eq!(third, "token-one");
    h.assert_recorded_len(2);
    let reqs = h.recorded();
    assert_eq!(
        reqs[0].headers.get(AUTHORIZATION),
        Some(&http::HeaderValue::from_static("one"))
    );
    assert_eq!(
        reqs[1].headers.get(AUTHORIZATION),
        Some(&http::HeaderValue::from_static("two"))
    );
    h.finish();
}

#[tokio::test]
async fn stale_cache_revalidates_with_etag_and_uses_304_body() {
    api! {
        client CacheRevalidateApi {
            scheme: https,
            host: "example.com",
            cache {
                profile short {
                    ttl 60 seconds
                }
                default short
            }
        }

        GET Cached
        -> Json<String>
        {
            path["cached"]
        }
    }

    use cache_revalidate_api::*;

    let (transport, h) = mock()
        .reply(
            MockReply::ok_json(json_bytes(&"first".to_string()))
                .with_header(CACHE_CONTROL, http::HeaderValue::from_static("max-age=0"))
                .with_header(ETAG, http::HeaderValue::from_static("\"etag-1\"")),
        )
        .reply(
            MockReply::status(http::StatusCode::NOT_MODIFIED)
                .with_header(ETAG, http::HeaderValue::from_static("\"etag-1\"")),
        )
        .build();

    let api = CacheRevalidateApi::new_with_transport(transport);
    let first = api
        .request(endpoints::Cached::new())
        .execute()
        .await
        .unwrap();
    let second = api
        .request(endpoints::Cached::new())
        .execute()
        .await
        .unwrap();

    assert_eq!(first, "first");
    assert_eq!(second, "first");
    h.assert_recorded_len(2);
    let reqs = h.recorded();
    assert_eq!(
        reqs[1].headers.get(IF_NONE_MATCH),
        Some(&http::HeaderValue::from_static("\"etag-1\""))
    );
    h.finish();
}

#[tokio::test]
async fn revalidation_transport_errors_retry_before_cache_after_error_fallback() {
    api! {
        client CacheRevalidationErrorApi {
            scheme: https,
            host: "example.com",
            retry {
                profile read {
                    attempts 2
                    methods [GET]
                    on transport[Timeout]
                    backoff none
                }
                default read
            }
        }

        GET Cached
        -> Json<String>
        {
            path["cached"]
        }
    }

    use cache_revalidation_error_api::*;

    let transport = FailingTransport::default();
    let calls = transport.calls.clone();
    let api = CacheRevalidationErrorApi::new_with_transport(transport)
        .with_cache_store(Arc::new(RevalidateThenStaleCache));

    let value = api
        .request(endpoints::Cached::new())
        .execute()
        .await
        .unwrap();

    assert_eq!(value, "stale");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "revalidation transport error should use retry before after_error fallback"
    );
}

#[tokio::test]
async fn cache_off_clears_inherited_cache() {
    api! {
        client CacheOffApi {
            scheme: https,
            host: "example.com",
            cache {
                profile short {
                    ttl 60 seconds
                }
                default short
            }
        }

        GET Uncached
        -> Json<String>
        {
            path["uncached"]
            cache off
        }
    }

    use cache_off_api::*;

    let (transport, h) = mock()
        .reply(
            MockReply::ok_json(json_bytes(&"first".to_string()))
                .with_header(CACHE_CONTROL, http::HeaderValue::from_static("max-age=60")),
        )
        .reply(
            MockReply::ok_json(json_bytes(&"second".to_string()))
                .with_header(CACHE_CONTROL, http::HeaderValue::from_static("max-age=60")),
        )
        .build();

    let api = CacheOffApi::new_with_transport(transport);
    let first = api
        .request(endpoints::Uncached::new())
        .execute()
        .await
        .unwrap();
    let second = api
        .request(endpoints::Uncached::new())
        .execute()
        .await
        .unwrap();

    assert_eq!(first, "first");
    assert_eq!(second, "second");
    h.assert_recorded_len(2);
    h.finish();
}

#[tokio::test]
async fn unsafe_success_invalidates_cached_get_for_same_uri() {
    api! {
        client CacheInvalidateApi {
            scheme: https,
            host: "example.com",
            cache {
                profile short {
                    ttl 60 seconds
                }
                default short
            }
        }

        GET Read
        -> Json<String>
        {
            path["resource"]
        }

        POST Write
        -> Json<String>
        {
            path["resource"]
        }
    }

    use cache_invalidate_api::*;

    let (transport, h) = mock()
        .reply(
            MockReply::ok_json(json_bytes(&"before".to_string()))
                .with_header(CACHE_CONTROL, http::HeaderValue::from_static("max-age=60")),
        )
        .reply(MockReply::ok_json(json_bytes(&"written".to_string())))
        .reply(
            MockReply::ok_json(json_bytes(&"after".to_string()))
                .with_header(CACHE_CONTROL, http::HeaderValue::from_static("max-age=60")),
        )
        .build();

    let api = CacheInvalidateApi::new_with_transport(transport);
    let first = api.request(endpoints::Read::new()).execute().await.unwrap();
    let second = api.request(endpoints::Read::new()).execute().await.unwrap();
    let written = api
        .request(endpoints::Write::new())
        .execute()
        .await
        .unwrap();
    let after = api.request(endpoints::Read::new()).execute().await.unwrap();

    assert_eq!(first, "before");
    assert_eq!(second, "before");
    assert_eq!(written, "written");
    assert_eq!(after, "after");
    h.assert_recorded_len(3);
    h.finish();
}

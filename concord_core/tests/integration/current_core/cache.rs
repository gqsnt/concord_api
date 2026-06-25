use super::common::*;
use bytes::Bytes;
use concord_core::advanced::{
    AuthApplicationRequest, AuthAppliedCredential, AuthError, AuthIdentity, AuthPlacement,
    BuiltRequest, BuiltResponse, CacheAfter, CacheBefore, CacheFuture, CacheKey, CacheRevalidation,
    CacheStore, CredentialMaterial, DebugSink, PreparedAuthCredential, RequestMeta,
    SecretCredential, apply_basic_credential, apply_secret_credential,
};
use concord_core::prelude::{ApiClient, ApiClientError, ClientContext, DebugLevel, Endpoint};
use http::{HeaderMap, Method, StatusCode};
use std::collections::HashMap;
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
    client.set_runtime_hooks(Arc::new(RecordingRuntimeHooks::new(events.clone())));
    configure_runtime(&mut client, Some(cache), Some(limiter));

    let endpoint = TextEndpoint {
        policy: cache_policy(),
        ..Default::default()
    };
    let decoded = client.request(endpoint).execute_decoded().await?;

    assert_eq!(decoded.value(), "cached");
    assert_eq!(sent_transport.sent_count().await, 0);
    let events = events.lock().await.clone();
    assert!(!events.iter().any(|event| event == "rate_acquire"));
    assert!(!events.iter().any(|event| event == "rate_response"));
    assert!(!events.iter().any(|event| event == "transport"));
    assert!(!events.iter().any(|event| event == "classify_response"));
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
    configure_runtime(&mut client, Some(cache), Some(limiter));

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
    configure_runtime(&mut client, Some(cache), Some(limiter));

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

    assert_eq!(decoded.value(), "stale");
    assert_eq!(sent_transport.sent_count().await, 2);
    assert_eq!(*after_error_count.lock().await, 1);
    Ok(())
}

#[tokio::test]
async fn stale_fallback_remote_attempt_observes_rate_limit_once() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let stale = built_response("Text", StatusCode::OK, "stale");
    let cache = Arc::new(RecordingCache::revalidate_stale_on_error(
        events.clone(),
        stale,
    ));
    let after_error_count = cache.after_error_count.clone();
    let after_response_count = cache.after_response_count.clone();
    let limiter = Arc::new(ObservationRateLimiter::new(events.clone()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(
            StatusCode::INTERNAL_SERVER_ERROR,
            "retry-me",
        )],
    );
    let sent_transport = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    configure_runtime(&mut client, Some(cache), Some(limiter));

    let endpoint = TextEndpoint {
        policy: {
            let mut policy = retry_policy(1);
            policy.cache = concord_core::internal::CacheSetting::Config(
                concord_core::advanced::CacheConfig::new(),
            );
            policy
        },
        ..Default::default()
    };
    let decoded = client.request(endpoint).execute_decoded().await?;

    assert_eq!(decoded.value(), "stale");
    assert_eq!(sent_transport.sent_count().await, 1);
    assert_eq!(*after_response_count.lock().await, 0);
    assert_eq!(*after_error_count.lock().await, 1);

    let events = events.lock().await.clone();
    let acquire = events
        .iter()
        .position(|event| event == "rate_acquire")
        .expect("rate limit acquired");
    let transport = events
        .iter()
        .position(|event| event == "transport")
        .expect("transport sent");
    let rate = events
        .iter()
        .position(|event| event == "rate_status:500 Internal Server Error")
        .expect("rate limit observed failed response");
    let stale = events
        .iter()
        .position(|event| event == "cache_after_error")
        .expect("stale fallback recorded");
    assert!(acquire < transport);
    assert!(transport < rate);
    assert!(rate < stale);
    assert_eq!(
        events
            .iter()
            .filter(|event| event.starts_with("rate_status:"))
            .count(),
        1
    );
    assert!(!events.iter().any(|event| event == "cache_after_response"));
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
    configure_runtime(&mut client, Some(cache), None);

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
    configure_runtime(&mut client, Some(cache), None);

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
            if response.status == StatusCode::NOT_MODIFIED
                && let Some(revalidation) = revalidation
            {
                return CacheAfter::Updated(Box::new(revalidation.cached_response));
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
    configure_runtime(&mut client, Some(cache), None);

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
async fn protected_auth_rejection_does_not_use_stale_fallback() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let stale = built_response(
        "Text",
        StatusCode::OK,
        "STALE_PROTECTED_RESPONSE_MUST_NOT_BE_SERVED_AFTER_AUTH_REJECTION",
    );
    let cache = Arc::new(RecordingCache::revalidate_stale_on_error(
        events.clone(),
        stale,
    ));
    let after_error_count = cache.after_error_count.clone();
    let transport = MockTransport::new(
        events,
        vec![MockResponse::text(StatusCode::FORBIDDEN, "forbidden")],
    );
    let mut client = client(
        TestAuthVars {
            token: Some("bad".to_string()),
            identity: "user-a",
        },
        transport,
    );
    configure_runtime(&mut client, Some(cache), None);
    let endpoint = TextEndpoint {
        policy: {
            let mut policy = auth_policy(AuthPlacement::Bearer);
            policy.cache = concord_core::internal::CacheSetting::Config(
                concord_core::advanced::CacheConfig::new(),
            );
            policy
        },
        ..Default::default()
    };

    let err = client
        .request(endpoint)
        .execute_decoded()
        .await
        .expect_err("protected auth rejection should not serve stale cache");

    assert!(err.to_string().contains("auth challenge rejected"));
    assert_eq!(*after_error_count.lock().await, 0);
}

#[tokio::test]
async fn never_refresh_protected_auth_rejection_does_not_use_stale_fallback() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let stale = built_response(
        "Text",
        StatusCode::OK,
        "STALE_PROTECTED_RESPONSE_MUST_NOT_BE_SERVED_AFTER_AUTH_REJECTION",
    );
    let cache = Arc::new(RecordingCache::revalidate_stale_on_error(
        events.clone(),
        stale,
    ));
    let after_error_count = cache.after_error_count.clone();
    let transport = MockTransport::new(
        events,
        vec![MockResponse::text(StatusCode::FORBIDDEN, "forbidden")],
    );
    let mut client = client(
        TestAuthVars {
            token: Some("bad".to_string()),
            identity: "user-a",
        },
        transport,
    );
    configure_runtime(&mut client, Some(cache), None);
    let mut endpoint = TextEndpoint {
        policy: auth_policy(AuthPlacement::Bearer),
        ..Default::default()
    };
    endpoint.policy.auth.requirements[0].challenge =
        concord_core::advanced::AuthChallengePolicy::NeverRefresh;

    let err = client
        .request(endpoint)
        .execute_decoded()
        .await
        .expect_err("NeverRefresh protected auth rejection should not serve stale cache");

    assert!(err.to_string().contains("auth challenge rejected"));
    assert_eq!(*after_error_count.lock().await, 0);
}

#[tokio::test]
async fn auth_rejection_refresh_retry_caches_success_only() -> Result<(), ApiClientError> {
    let cache = Arc::new(DefaultKeyMemoryCache::default());
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(
                StatusCode::UNAUTHORIZED,
                "AUTH_REJECTION_RESPONSE_MUST_NOT_BE_CACHED",
            ),
            MockResponse::text(StatusCode::OK, "SUCCESS_AFTER_AUTH_REFRESH"),
        ],
    );
    let sent = transport.clone();
    let mut client = client(
        TestAuthVars {
            token: Some("refreshable".to_string()),
            identity: "refresh",
        },
        transport,
    );
    configure_runtime(&mut client, Some(cache.clone()), None);
    let endpoint = TextEndpoint {
        policy: {
            let mut policy = auth_policy(AuthPlacement::Bearer);
            policy.cache = concord_core::internal::CacheSetting::Config(
                concord_core::advanced::CacheConfig::new(),
            );
            policy
        },
        ..Default::default()
    };

    let first = client.request(endpoint.clone()).execute_decoded().await?;
    let second = client.request(endpoint).execute_decoded().await?;

    assert_eq!(first.value(), "SUCCESS_AFTER_AUTH_REFRESH");
    assert_eq!(second.value(), "SUCCESS_AFTER_AUTH_REFRESH");
    assert_eq!(sent.sent_count().await, 2);
    let keys = cache.keys().await;
    assert_eq!(keys.len(), 1);
    assert!(keys[0].contains("|auth="));
    assert!(!keys[0].contains("AUTH_REJECTION_RESPONSE_MUST_NOT_BE_CACHED"));
    Ok(())
}

#[tokio::test]
async fn bearer_auth_cache_is_partitioned_by_safe_identity() -> Result<(), ApiClientError> {
    let cache = Arc::new(DefaultKeyMemoryCache::default());
    let events_a = Arc::new(Mutex::new(Vec::new()));
    let events_b = Arc::new(Mutex::new(Vec::new()));
    let transport_a = MockTransport::new(
        events_a,
        vec![MockResponse::text(
            StatusCode::OK,
            "CACHE_RESPONSE_FOR_AUTH_A",
        )],
    );
    let transport_b = MockTransport::new(
        events_b,
        vec![MockResponse::text(
            StatusCode::OK,
            "CACHE_RESPONSE_FOR_AUTH_B",
        )],
    );
    let sent_a = transport_a.clone();
    let sent_b = transport_b.clone();
    let mut client_a = client(
        TestAuthVars {
            token: Some("BEARER_CACHE_SECRET_A".to_string()),
            ..Default::default()
        },
        transport_a,
    );
    let mut client_b = client(
        TestAuthVars {
            token: Some("BEARER_CACHE_SECRET_B".to_string()),
            ..Default::default()
        },
        transport_b,
    );
    configure_runtime(&mut client_a, Some(cache.clone()), None);
    configure_runtime(&mut client_b, Some(cache.clone()), None);
    let endpoint = TextEndpoint {
        policy: {
            let mut policy = auth_policy(AuthPlacement::Bearer);
            policy.cache = concord_core::internal::CacheSetting::Config(
                concord_core::advanced::CacheConfig::new(),
            );
            policy
        },
        ..Default::default()
    };

    let first = client_a.request(endpoint.clone()).execute_decoded().await?;
    let second = client_b.request(endpoint).execute_decoded().await?;

    assert_eq!(first.value(), "CACHE_RESPONSE_FOR_AUTH_A");
    assert_eq!(second.value(), "CACHE_RESPONSE_FOR_AUTH_B");
    assert_eq!(sent_a.sent_count().await, 1);
    assert_eq!(sent_b.sent_count().await, 1);
    let keys = cache.keys().await;
    assert_eq!(keys.len(), 2);
    assert!(keys.iter().all(|key| key.contains("|auth=")));
    assert!(keys.iter().all(|key| key.contains("place:6:bearer")));
    assert!(
        keys.iter()
            .all(|key| !key.contains("BEARER_CACHE_SECRET_A"))
    );
    assert!(
        keys.iter()
            .all(|key| !key.contains("BEARER_CACHE_SECRET_B"))
    );
    Ok(())
}

#[tokio::test]
async fn query_auth_cache_is_partitioned_without_raw_query_secret() -> Result<(), ApiClientError> {
    let cache = Arc::new(DefaultKeyMemoryCache::default());
    let events_a = Arc::new(Mutex::new(Vec::new()));
    let events_b = Arc::new(Mutex::new(Vec::new()));
    let transport_a = MockTransport::new(
        events_a,
        vec![MockResponse::text(
            StatusCode::OK,
            "CACHE_RESPONSE_FOR_QUERY_AUTH_A",
        )],
    );
    let transport_b = MockTransport::new(
        events_b,
        vec![MockResponse::text(
            StatusCode::OK,
            "CACHE_RESPONSE_FOR_QUERY_AUTH_B",
        )],
    );
    let sent_a = transport_a.clone();
    let sent_b = transport_b.clone();
    let mut client_a = client(
        TestAuthVars {
            token: Some("QUERY_AUTH_SECRET_A".to_string()),
            ..Default::default()
        },
        transport_a,
    );
    let mut client_b = client(
        TestAuthVars {
            token: Some("QUERY_AUTH_SECRET_B".to_string()),
            ..Default::default()
        },
        transport_b,
    );
    configure_runtime(&mut client_a, Some(cache.clone()), None);
    configure_runtime(&mut client_b, Some(cache.clone()), None);
    let endpoint = TextEndpoint {
        policy: {
            let mut policy = auth_policy(AuthPlacement::Query("api_key"));
            policy.cache = concord_core::internal::CacheSetting::Config(
                concord_core::advanced::CacheConfig::new(),
            );
            policy
        },
        ..Default::default()
    };

    let first = client_a.request(endpoint.clone()).execute_decoded().await?;
    let second = client_b.request(endpoint).execute_decoded().await?;

    assert_eq!(first.value(), "CACHE_RESPONSE_FOR_QUERY_AUTH_A");
    assert_eq!(second.value(), "CACHE_RESPONSE_FOR_QUERY_AUTH_B");
    assert_eq!(sent_a.sent_count().await, 1);
    assert_eq!(sent_b.sent_count().await, 1);
    let keys = cache.keys().await;
    assert_eq!(keys.len(), 2);
    assert!(keys.iter().all(|key| key.contains("place:5:query")));
    assert!(keys.iter().all(|key| key.contains("query:7:api_key")));
    assert!(keys.iter().all(|key| !key.contains("QUERY_AUTH_SECRET_A")));
    assert!(keys.iter().all(|key| !key.contains("QUERY_AUTH_SECRET_B")));
    Ok(())
}

#[tokio::test]
async fn header_auth_cache_is_partitioned_by_safe_identity() -> Result<(), ApiClientError> {
    let cache = Arc::new(DefaultKeyMemoryCache::default());
    let events_a = Arc::new(Mutex::new(Vec::new()));
    let events_b = Arc::new(Mutex::new(Vec::new()));
    let transport_a = MockTransport::new(
        events_a,
        vec![MockResponse::text(
            StatusCode::OK,
            "CACHE_RESPONSE_FOR_HEADER_AUTH_A",
        )],
    );
    let transport_b = MockTransport::new(
        events_b,
        vec![MockResponse::text(
            StatusCode::OK,
            "CACHE_RESPONSE_FOR_HEADER_AUTH_B",
        )],
    );
    let sent_a = transport_a.clone();
    let sent_b = transport_b.clone();
    let mut client_a = client(
        TestAuthVars {
            token: Some("HEADER_CACHE_SECRET_A".to_string()),
            ..Default::default()
        },
        transport_a,
    );
    let mut client_b = client(
        TestAuthVars {
            token: Some("HEADER_CACHE_SECRET_B".to_string()),
            ..Default::default()
        },
        transport_b,
    );
    configure_runtime(&mut client_a, Some(cache.clone()), None);
    configure_runtime(&mut client_b, Some(cache.clone()), None);
    let endpoint = TextEndpoint {
        policy: {
            let mut policy = auth_policy(AuthPlacement::Header("X-Api-Key"));
            policy.cache = concord_core::internal::CacheSetting::Config(
                concord_core::advanced::CacheConfig::new(),
            );
            policy
        },
        ..Default::default()
    };

    let first = client_a.request(endpoint.clone()).execute_decoded().await?;
    let second = client_b.request(endpoint).execute_decoded().await?;

    assert_eq!(first.value(), "CACHE_RESPONSE_FOR_HEADER_AUTH_A");
    assert_eq!(second.value(), "CACHE_RESPONSE_FOR_HEADER_AUTH_B");
    assert_eq!(sent_a.sent_count().await, 1);
    assert_eq!(sent_b.sent_count().await, 1);
    let keys = cache.keys().await;
    assert_eq!(keys.len(), 2);
    assert!(keys.iter().all(|key| key.contains("place:6:header")));
    assert!(keys.iter().all(|key| key.contains("header:9:x-api-key")));
    assert!(
        keys.iter()
            .all(|key| !key.contains("HEADER_CACHE_SECRET_A"))
    );
    assert!(
        keys.iter()
            .all(|key| !key.contains("HEADER_CACHE_SECRET_B"))
    );
    Ok(())
}

#[tokio::test]
async fn basic_auth_cache_is_partitioned_by_safe_identity() -> Result<(), ApiClientError> {
    let cache = Arc::new(DefaultKeyMemoryCache::default());
    let events_a = Arc::new(Mutex::new(Vec::new()));
    let events_b = Arc::new(Mutex::new(Vec::new()));
    let transport_a = MockTransport::new(
        events_a,
        vec![MockResponse::text(
            StatusCode::OK,
            "CACHE_RESPONSE_FOR_BASIC_AUTH_A",
        )],
    );
    let transport_b = MockTransport::new(
        events_b,
        vec![MockResponse::text(
            StatusCode::OK,
            "CACHE_RESPONSE_FOR_BASIC_AUTH_B",
        )],
    );
    let sent_a = transport_a.clone();
    let sent_b = transport_b.clone();
    let mut client_a = concord_core::prelude::ApiClient::<BasicCacheCx, _>::with_transport(
        (),
        BasicCacheVars {
            username: "BASIC_CACHE_USERNAME_A".to_string(),
            password: "BASIC_CACHE_PASSWORD_A".to_string(),
        },
        transport_a,
    );
    let mut client_b = concord_core::prelude::ApiClient::<BasicCacheCx, _>::with_transport(
        (),
        BasicCacheVars {
            username: "BASIC_CACHE_USERNAME_B".to_string(),
            password: "BASIC_CACHE_PASSWORD_B".to_string(),
        },
        transport_b,
    );
    configure_runtime(&mut client_a, Some(cache.clone()), None);
    configure_runtime(&mut client_b, Some(cache.clone()), None);
    let endpoint = BasicCacheEndpoint {
        policy: {
            let mut policy = auth_policy(AuthPlacement::Basic);
            policy.cache = concord_core::internal::CacheSetting::Config(
                concord_core::advanced::CacheConfig::new(),
            );
            policy
        },
    };

    let first = client_a.request(endpoint.clone()).execute_decoded().await?;
    let second = client_b.request(endpoint).execute_decoded().await?;

    assert_eq!(first.value(), "CACHE_RESPONSE_FOR_BASIC_AUTH_A");
    assert_eq!(second.value(), "CACHE_RESPONSE_FOR_BASIC_AUTH_B");
    assert_eq!(sent_a.sent_count().await, 1);
    assert_eq!(sent_b.sent_count().await, 1);
    let keys = cache.keys().await;
    assert_eq!(keys.len(), 2);
    assert!(keys.iter().all(|key| key.contains("place:5:basic")));
    assert!(
        keys.iter()
            .all(|key| !key.contains("BASIC_CACHE_USERNAME_A"))
    );
    assert!(
        keys.iter()
            .all(|key| !key.contains("BASIC_CACHE_USERNAME_B"))
    );
    assert!(
        keys.iter()
            .all(|key| !key.contains("BASIC_CACHE_PASSWORD_A"))
    );
    assert!(
        keys.iter()
            .all(|key| !key.contains("BASIC_CACHE_PASSWORD_B"))
    );
    Ok(())
}

#[tokio::test]
async fn basic_auth_collision_fails_before_cache_and_rate_limit() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let cache = Arc::new(RecordingCache::hit(
        events.clone(),
        built_response("BasicCache", StatusCode::OK, "cached"),
    ));
    let rate_limiter = Arc::new(RecordingRateLimiter::new(events.clone()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "should-not-send")],
    );
    let sent = transport.clone();
    let mut client = concord_core::prelude::ApiClient::<BasicCacheCx, _>::with_transport(
        (),
        BasicCacheVars {
            username: "BASIC_COLLISION_USERNAME".to_string(),
            password: "BASIC_COLLISION_PASSWORD".to_string(),
        },
        transport,
    );
    configure_runtime(&mut client, Some(cache), Some(rate_limiter));
    let endpoint = BasicCacheEndpoint {
        policy: {
            let mut policy = auth_policy(AuthPlacement::Basic);
            policy.cache = concord_core::internal::CacheSetting::Config(
                concord_core::advanced::CacheConfig::new(),
            );
            policy.headers.insert(
                http::header::AUTHORIZATION,
                http::HeaderValue::from_static("public"),
            );
            policy
        },
    };

    let err = client
        .request(endpoint)
        .execute_decoded()
        .await
        .expect_err("basic auth collision should fail before cache or rate limit");

    match err {
        ApiClientError::Auth { source, .. } => {
            assert_eq!(
                source.kind,
                concord_core::advanced::AuthErrorKind::InvalidConfiguration
            );
            let msg = source.to_string();
            assert!(msg.contains("Authorization"));
            assert!(!msg.contains("BASIC_COLLISION_USERNAME"));
            assert!(!msg.contains("BASIC_COLLISION_PASSWORD"));
        }
        other => panic!("expected auth error, got {other:?}"),
    }
    assert_eq!(sent.sent_count().await, 0);
    let events = events.lock().await.clone();
    assert!(
        !events
            .iter()
            .any(|event| event.starts_with("cache_before:"))
    );
    assert!(!events.iter().any(|event| event == "cache_hit"));
    assert!(!events.iter().any(|event| event == "rate_acquire"));
    assert!(!events.iter().any(|event| event == "rate_response"));
}

#[tokio::test]
async fn same_auth_identity_hits_cache() -> Result<(), ApiClientError> {
    let cache = Arc::new(DefaultKeyMemoryCache::default());
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![MockResponse::text(
            StatusCode::OK,
            "CACHE_RESPONSE_FOR_SAME_AUTH",
        )],
    );
    let sent = transport.clone();
    let mut client = client(
        TestAuthVars {
            token: Some("SAME_AUTH_CACHE_SECRET".to_string()),
            ..Default::default()
        },
        transport,
    );
    configure_runtime(&mut client, Some(cache), None);
    let endpoint = TextEndpoint {
        policy: {
            let mut policy = auth_policy(AuthPlacement::Bearer);
            policy.cache = concord_core::internal::CacheSetting::Config(
                concord_core::advanced::CacheConfig::new(),
            );
            policy
        },
        ..Default::default()
    };

    let first = client.request(endpoint.clone()).execute_decoded().await?;
    let second = client.request(endpoint).execute_decoded().await?;

    assert_eq!(first.value(), "CACHE_RESPONSE_FOR_SAME_AUTH");
    assert_eq!(second.value(), "CACHE_RESPONSE_FOR_SAME_AUTH");
    assert_eq!(sent.sent_count().await, 1);
    Ok(())
}

#[tokio::test]
async fn public_unauthenticated_cache_still_works() -> Result<(), ApiClientError> {
    let cache = Arc::new(DefaultKeyMemoryCache::default());
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![MockResponse::text(StatusCode::OK, "PUBLIC_CACHE_RESPONSE")],
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

    assert_eq!(first.value(), "PUBLIC_CACHE_RESPONSE");
    assert_eq!(second.value(), "PUBLIC_CACHE_RESPONSE");
    assert_eq!(sent.sent_count().await, 1);
    Ok(())
}

#[tokio::test]
async fn anonymous_protected_identity_bypasses_cache_runtime() -> Result<(), ApiClientError> {
    let cache = Arc::new(CountingCache::default());
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "ANON_RESPONSE_ONE"),
            MockResponse::text(StatusCode::OK, "ANON_RESPONSE_TWO"),
        ],
    );
    let sent = transport.clone();
    let mut client = ApiClient::<AnonymousCacheCx, _>::with_transport(
        (),
        AnonymousAuthVars {
            token: "ANONYMOUS_CACHE_SECRET".to_string(),
        },
        transport,
    );
    client.configure(|cfg| {
        cfg.cache_store(cache.clone());
    });
    let endpoint = AnonymousTextEndpoint;

    let first = client.request(endpoint.clone()).execute_decoded().await?;
    let second = client.request(endpoint).execute_decoded().await?;

    assert_eq!(first.value(), "ANON_RESPONSE_ONE");
    assert_eq!(second.value(), "ANON_RESPONSE_TWO");
    assert_eq!(sent.sent_count().await, 2);
    assert_eq!(*cache.before_count.lock().await, 0);
    assert_eq!(*cache.after_response_count.lock().await, 0);
    assert_eq!(*cache.after_error_count.lock().await, 0);
    Ok(())
}

#[tokio::test]
async fn protected_request_without_safe_identity_does_not_use_stale_fallback()
-> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let stale = built_response("Text", StatusCode::OK, "stale");
    let cache = Arc::new(RecordingCache::revalidate_stale_on_error(
        events.clone(),
        stale,
    ));
    let after_error_count = cache.after_error_count.clone();
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(
            StatusCode::INTERNAL_SERVER_ERROR,
            "remote-fail",
        )],
    );
    let sent = transport.clone();
    let mut client = ApiClient::<AnonymousCacheCx, _>::with_transport(
        (),
        AnonymousAuthVars {
            token: "ANONYMOUS_CACHE_SECRET".to_string(),
        },
        transport,
    );
    client.configure(|cfg| {
        cfg.cache_store(cache.clone());
    });

    let err = client
        .request(AnonymousTextEndpoint)
        .execute_decoded()
        .await
        .expect_err("protected request without safe identity should not serve stale");

    assert!(err.to_string().contains("500"));
    assert_eq!(sent.sent_count().await, 1);
    assert_eq!(*after_error_count.lock().await, 0);
    let events = events.lock().await.clone();
    assert!(
        !events
            .iter()
            .any(|event| event.starts_with("cache_before:"))
    );
    assert!(!events.iter().any(|event| event == "cache_after_error"));
    Ok(())
}

#[derive(Default)]
struct DefaultKeyMemoryCache {
    entries: Mutex<HashMap<String, BuiltResponse>>,
}

impl DefaultKeyMemoryCache {
    async fn keys(&self) -> Vec<String> {
        self.entries.lock().await.keys().cloned().collect()
    }
}

#[derive(Clone)]
struct AnonymousAuthVars {
    token: String,
}

#[derive(Clone)]
struct AnonymousCacheCx;

#[derive(Clone)]
struct AnonymousSecret(String);

impl CredentialMaterial for AnonymousSecret {
    fn safe_identity(&self) -> AuthIdentity {
        AuthIdentity::Anonymous
    }
}

impl SecretCredential for AnonymousSecret {
    fn secret_value(&self) -> &str {
        &self.0
    }
}

impl ClientContext for AnonymousCacheCx {
    type Vars = ();
    type AuthVars = AnonymousAuthVars;
    type AuthState = ();
    const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
    const DOMAIN: &'static str = "example.com";

    fn init_auth_state(_vars: &Self::Vars, _auth: &Self::AuthVars) -> Self::AuthState {}

    fn prepare_auth_requirement<'a>(
        requirement: &'a concord_core::advanced::AuthRequirement,
        request: &'a mut AuthApplicationRequest<'_>,
        _vars: &'a Self::Vars,
        auth: &'a Self::AuthVars,
        _auth_state: &'a Self::AuthState,
        _executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
        _meta: &'a concord_core::advanced::RequestMeta,
    ) -> concord_core::advanced::AuthFuture<'a, Result<PreparedAuthCredential, AuthError>> {
        Box::pin(async move {
            let material = AnonymousSecret(auth.token.clone());
            let application = apply_secret_credential(request, requirement, &material)?;
            let applied = AuthAppliedCredential {
                credential_id: requirement.credential.id.clone(),
                usage_id: requirement.usage_id.clone(),
                step_id: requirement.step_id,
                generation: Some(1),
                identity: application.identity().clone(),
                provenance: requirement.provenance.clone(),
            };
            Ok(PreparedAuthCredential::new(applied, application))
        })
    }
}

#[derive(Clone)]
struct AnonymousTextEndpoint;

impl Endpoint<AnonymousCacheCx> for AnonymousTextEndpoint {
    type Response = String;

    fn plan(
        &self,
        _ctx: &concord_core::internal::ClientPlanContext<'_, AnonymousCacheCx>,
    ) -> Result<concord_core::internal::RequestPlan, ApiClientError> {
        let mut policy = auth_policy(AuthPlacement::Bearer);
        policy.cache = concord_core::internal::CacheSetting::Config(
            concord_core::advanced::CacheConfig::new(),
        );
        Ok(request_plan(
            "AnonymousText",
            Method::GET,
            "/text",
            policy,
            None,
            decode_string,
        ))
    }
}

#[derive(Clone)]
struct BasicCacheVars {
    username: String,
    password: String,
}

#[derive(Clone)]
struct BasicCacheCx;

impl ClientContext for BasicCacheCx {
    type Vars = ();
    type AuthVars = BasicCacheVars;
    type AuthState = ();
    const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
    const DOMAIN: &'static str = "example.com";

    fn init_auth_state(_vars: &Self::Vars, _auth: &Self::AuthVars) -> Self::AuthState {}

    fn prepare_auth_requirement<'a>(
        requirement: &'a concord_core::advanced::AuthRequirement,
        request: &'a mut AuthApplicationRequest<'_>,
        _vars: &'a Self::Vars,
        auth: &'a Self::AuthVars,
        _auth_state: &'a Self::AuthState,
        _executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
        _meta: &'a RequestMeta,
    ) -> concord_core::advanced::AuthFuture<'a, Result<PreparedAuthCredential, AuthError>> {
        Box::pin(async move {
            let material = concord_core::prelude::BasicCredential::new(
                auth.username.clone(),
                auth.password.clone(),
            );
            let application = apply_basic_credential(request, requirement, &material)?;
            let applied = AuthAppliedCredential {
                credential_id: requirement.credential.id.clone(),
                usage_id: requirement.usage_id.clone(),
                step_id: requirement.step_id,
                generation: Some(1),
                identity: application.identity().clone(),
                provenance: requirement.provenance.clone(),
            };
            Ok(PreparedAuthCredential::new(applied, application))
        })
    }

    fn handle_auth_response<'a>(
        _requirement: &'a concord_core::advanced::AuthRequirement,
        _applied: &'a AuthAppliedCredential,
        _vars: &'a Self::Vars,
        _auth: &'a Self::AuthVars,
        _auth_state: &'a Self::AuthState,
        _executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
        _meta: &'a RequestMeta,
        _status: StatusCode,
        _headers: &'a HeaderMap,
    ) -> concord_core::advanced::AuthFuture<
        'a,
        Result<concord_core::advanced::AuthDecision, AuthError>,
    > {
        Box::pin(async { Ok(concord_core::advanced::AuthDecision::Continue) })
    }
}

#[derive(Clone)]
struct BasicCacheEndpoint {
    policy: concord_core::internal::ResolvedPolicy,
}

impl Endpoint<BasicCacheCx> for BasicCacheEndpoint {
    type Response = String;

    fn plan(
        &self,
        _ctx: &concord_core::internal::ClientPlanContext<'_, BasicCacheCx>,
    ) -> Result<concord_core::internal::RequestPlan, ApiClientError> {
        Ok(request_plan(
            "BasicCache",
            Method::GET,
            "/text",
            self.policy.clone(),
            None,
            decode_string,
        ))
    }
}

#[derive(Default)]
struct CountingCache {
    before_count: Mutex<u32>,
    after_response_count: Mutex<u32>,
    after_error_count: Mutex<u32>,
}

impl CacheStore for CountingCache {
    fn before_request<'a>(&'a self, _request: &'a BuiltRequest) -> CacheFuture<'a, CacheBefore> {
        Box::pin(async move {
            *self.before_count.lock().await += 1;
            CacheBefore::Miss
        })
    }

    fn after_response<'a>(
        &'a self,
        _request: &'a BuiltRequest,
        _response: &'a BuiltResponse,
        _revalidation: Option<CacheRevalidation>,
    ) -> CacheFuture<'a, CacheAfter> {
        Box::pin(async move {
            *self.after_response_count.lock().await += 1;
            CacheAfter::Stored
        })
    }

    fn after_error<'a>(
        &'a self,
        _request: &'a BuiltRequest,
        _error: &'a ApiClientError,
        _revalidation: Option<CacheRevalidation>,
    ) -> CacheFuture<'a, Option<BuiltResponse>> {
        Box::pin(async move {
            *self.after_error_count.lock().await += 1;
            None
        })
    }
}

impl CacheStore for DefaultKeyMemoryCache {
    fn before_request<'a>(&'a self, request: &'a BuiltRequest) -> CacheFuture<'a, CacheBefore> {
        Box::pin(async move {
            let key = concord_core::advanced::default_cache_key(request);
            self.entries
                .lock()
                .await
                .get(key.as_str())
                .cloned()
                .map(CacheBefore::Hit)
                .unwrap_or(CacheBefore::Miss)
        })
    }

    fn after_response<'a>(
        &'a self,
        request: &'a BuiltRequest,
        response: &'a BuiltResponse,
        _revalidation: Option<CacheRevalidation>,
    ) -> CacheFuture<'a, CacheAfter> {
        Box::pin(async move {
            let key = concord_core::advanced::default_cache_key(request);
            self.entries
                .lock()
                .await
                .insert(key.as_str().to_string(), response.clone());
            CacheAfter::Stored
        })
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

    fn response_status(&self, _dbg: DebugLevel, _status: StatusCode, _url: &str, _ok: bool) {}

    fn response_headers(&self, _dbg: DebugLevel, _headers: &HeaderMap) {}

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

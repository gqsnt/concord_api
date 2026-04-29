use super::common::*;
use concord_core::prelude::ApiClientError;
use http::StatusCode;
use std::sync::Arc;
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

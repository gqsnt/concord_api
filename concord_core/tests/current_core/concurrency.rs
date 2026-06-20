use super::common::*;
use concord_core::advanced::{
    BuiltRequest, BuiltResponse, CacheAfter, CacheBefore, CacheFuture, CacheRevalidation,
    CacheStore,
};
use concord_core::prelude::ApiClientError;
use http::StatusCode;
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::test]
async fn identical_concurrent_get_requests_are_not_coalesced() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "first"),
            MockResponse::text(StatusCode::OK, "second"),
        ],
    )
    .delayed(std::time::Duration::from_millis(25));
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let (a, b) = tokio::join!(
        client.request(TextEndpoint::default()).execute_decoded(),
        client.request(TextEndpoint::default()).execute_decoded()
    );

    let mut values = vec![a?.into_value(), b?.into_value()];
    values.sort();
    assert_eq!(values, vec!["first".to_string(), "second".to_string()]);
    assert_eq!(sent.sent_count().await, 2);
    Ok(())
}

#[tokio::test]
async fn identical_concurrent_post_requests_are_not_coalesced() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "first"),
            MockResponse::text(StatusCode::OK, "second"),
        ],
    )
    .delayed(std::time::Duration::from_millis(25));
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);
    let endpoint = TextEndpoint {
        method: http::Method::POST,
        ..Default::default()
    };

    let (a, b) = tokio::join!(
        client.request(endpoint.clone()).execute_decoded(),
        client.request(endpoint).execute_decoded()
    );

    let mut values = vec![a?.into_value(), b?.into_value()];
    values.sort();
    assert_eq!(values, vec!["first".to_string(), "second".to_string()]);
    assert_eq!(sent.sent_count().await, 2);
    Ok(())
}

#[tokio::test]
async fn cache_hit_after_completed_response_still_avoids_transport() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let cache = Arc::new(StoringCache::default());
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "stored"),
            MockResponse::text(StatusCode::OK, "unexpected"),
        ],
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

    assert_eq!(first.value(), "stored");
    assert_eq!(second.value(), "stored");
    assert_eq!(sent.sent_count().await, 1);
    Ok(())
}

#[tokio::test]
async fn concurrent_cache_miss_requests_both_send_transport() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let cache = Arc::new(StoringCache::default());
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "first"),
            MockResponse::text(StatusCode::OK, "second"),
        ],
    )
    .delayed(std::time::Duration::from_millis(25));
    let sent = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    configure_runtime(&mut client, Some(cache), None);
    let endpoint = TextEndpoint {
        policy: cache_policy(),
        ..Default::default()
    };

    let (a, b) = tokio::join!(
        client.request(endpoint.clone()).execute_decoded(),
        client.request(endpoint).execute_decoded()
    );

    let mut values = vec![a?.into_value(), b?.into_value()];
    values.sort();
    assert_eq!(values, vec!["first".to_string(), "second".to_string()]);
    assert_eq!(sent.sent_count().await, 2);
    Ok(())
}

#[tokio::test]
async fn rate_limit_still_observes_each_non_coalesced_request() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let limiter = Arc::new(RecordingRateLimiter::new(events.clone()));
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::OK, "first"),
            MockResponse::text(StatusCode::OK, "second"),
        ],
    )
    .delayed(std::time::Duration::from_millis(25));
    let mut client = client(TestAuthVars::default(), transport);
    configure_runtime(&mut client, None, Some(limiter));

    let (a, b) = tokio::join!(
        client.request(TextEndpoint::default()).execute_decoded(),
        client.request(TextEndpoint::default()).execute_decoded()
    );
    a?;
    b?;

    let events = events.lock().await.clone();
    assert_eq!(
        events
            .iter()
            .filter(|event| event.as_str() == "rate_acquire")
            .count(),
        2
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.as_str() == "rate_response")
            .count(),
        2
    );
    Ok(())
}

#[tokio::test]
async fn retry_still_applies_per_non_coalesced_request() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, "retry-a"),
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, "retry-b"),
            MockResponse::text(StatusCode::OK, "first"),
            MockResponse::text(StatusCode::OK, "second"),
        ],
    )
    .delayed(std::time::Duration::from_millis(25));
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);
    let endpoint = TextEndpoint {
        policy: retry_policy(2),
        ..Default::default()
    };

    let (a, b) = tokio::join!(
        client.request(endpoint.clone()).execute_decoded(),
        client.request(endpoint).execute_decoded()
    );

    let mut values = vec![a?.into_value(), b?.into_value()];
    values.sort();
    assert_eq!(values, vec!["first".to_string(), "second".to_string()]);
    assert_eq!(sent.sent_count().await, 4);
    Ok(())
}

#[derive(Default)]
struct StoringCache {
    response: Mutex<Option<BuiltResponse>>,
}

impl CacheStore for StoringCache {
    fn before_request<'a>(&'a self, _request: &'a BuiltRequest) -> CacheFuture<'a, CacheBefore> {
        Box::pin(async move {
            match self.response.lock().await.clone() {
                Some(response) => CacheBefore::Hit(response),
                None => CacheBefore::Miss,
            }
        })
    }

    fn after_response<'a>(
        &'a self,
        _request: &'a BuiltRequest,
        response: &'a BuiltResponse,
        _revalidation: Option<CacheRevalidation>,
    ) -> CacheFuture<'a, CacheAfter> {
        Box::pin(async move {
            *self.response.lock().await = Some(response.clone());
            CacheAfter::Stored
        })
    }
}

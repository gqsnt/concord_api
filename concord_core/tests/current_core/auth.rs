use super::common::*;
use concord_core::advanced::AuthPlacement;
use concord_core::prelude::ApiClientError;
use http::StatusCode;
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::test]
async fn missing_credential_error_is_actionable() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![MockResponse::text(StatusCode::OK, "should-not-send")],
    );
    let client = client(TestAuthVars::default(), transport);
    let endpoint = TextEndpoint {
        policy: auth_policy(AuthPlacement::Bearer),
        ..Default::default()
    };

    let err = client
        .request(endpoint)
        .execute_decoded()
        .await
        .expect_err("missing token must fail before transport");

    let msg = err.to_string();
    assert!(msg.contains("missing credential"));
    assert!(msg.contains("test.token"));
    assert!(msg.contains("acquire or configure"));
}

#[tokio::test]
async fn auth_rejection_does_not_store_cache_entry() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let cache = Arc::new(RecordingCache::miss(events.clone()));
    let after_response_count = cache.after_response_count.clone();
    let transport = MockTransport::new(
        events,
        vec![MockResponse::text(StatusCode::UNAUTHORIZED, "nope")],
    );
    let mut client = client(
        TestAuthVars {
            token: Some("bad".to_string()),
            identity: "user-a",
        },
        transport,
    );
    configure_runtime(&mut client, Some(cache), None, false, None);
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
        .expect_err("401 auth rejection should fail");
    assert!(err.to_string().contains("auth challenge rejected"));
    assert_eq!(*after_response_count.lock().await, 0);
}

#[tokio::test]
async fn auth_rejection_preempts_retry_policy() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::UNAUTHORIZED, "unauthorized"),
            MockResponse::text(StatusCode::OK, "should-not-retry"),
        ],
    );
    let sent_transport = transport.clone();
    let client = client(
        TestAuthVars {
            token: Some("bad".to_string()),
            identity: "user-a",
        },
        transport,
    );
    let endpoint = TextEndpoint {
        policy: {
            let mut policy = auth_policy(AuthPlacement::Bearer);
            policy.retry = retry_policy_for_statuses(2, vec![StatusCode::UNAUTHORIZED]).retry;
            policy
        },
        ..Default::default()
    };

    let err = client
        .request(endpoint)
        .execute_decoded()
        .await
        .expect_err("auth rejection should not fall through to retry");

    assert!(err.to_string().contains("auth challenge rejected"));
    assert_eq!(sent_transport.sent_count().await, 1);
}

#[tokio::test]
async fn unauthorized_can_trigger_bounded_auth_refresh_before_retry() -> Result<(), ApiClientError>
{
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::UNAUTHORIZED, "expired"),
            MockResponse::text(StatusCode::OK, "refreshed"),
        ],
    );
    let sent_transport = transport.clone();
    let client = client(
        TestAuthVars {
            token: Some("refreshable".to_string()),
            identity: "refresh",
        },
        transport,
    );
    let endpoint = TextEndpoint {
        policy: {
            let mut policy = auth_policy(AuthPlacement::Bearer);
            policy.retry = retry_policy_for_statuses(2, vec![StatusCode::UNAUTHORIZED]).retry;
            policy
        },
        ..Default::default()
    };

    let decoded = client.request(endpoint).execute_decoded().await?;

    assert_eq!(decoded.value(), "refreshed");
    assert_eq!(sent_transport.sent_count().await, 2);
    Ok(())
}

#[tokio::test]
async fn auth_refresh_failure_is_terminal_auth_error() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::UNAUTHORIZED, "expired"),
            MockResponse::text(StatusCode::OK, "should-not-send"),
        ],
    );
    let sent_transport = transport.clone();
    let client = client(
        TestAuthVars {
            token: Some("refreshable".to_string()),
            identity: "refresh-error",
        },
        transport,
    );
    let endpoint = TextEndpoint {
        policy: auth_policy(AuthPlacement::Bearer),
        ..Default::default()
    };

    let err = client
        .request(endpoint)
        .execute_decoded()
        .await
        .expect_err("auth refresh failure is terminal");

    assert!(err.to_string().contains("auth refresh failed"));
    assert_eq!(sent_transport.sent_count().await, 1);
}

#[tokio::test]
async fn bearer_header_and_query_auth_are_applied() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "bearer"),
            MockResponse::text(StatusCode::OK, "query"),
        ],
    );
    let sent = transport.clone();
    let client = client(
        TestAuthVars {
            token: Some("token-1".to_string()),
            identity: "user-a",
        },
        transport,
    );

    client
        .request(TextEndpoint {
            policy: auth_policy(AuthPlacement::Bearer),
            ..Default::default()
        })
        .execute_decoded()
        .await?;
    client
        .request(TextEndpoint {
            policy: auth_policy(AuthPlacement::Query("api_key")),
            ..Default::default()
        })
        .execute_decoded()
        .await?;

    let requests = sent.requests().await;
    assert_eq!(
        requests[0]
            .headers
            .get(http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok()),
        Some("Bearer token-1")
    );
    assert_eq!(
        requests[1]
            .url
            .query_pairs()
            .find(|(key, _)| key == "api_key")
            .map(|(_, value)| value.into_owned()),
        Some("token-1".to_string())
    );
    Ok(())
}

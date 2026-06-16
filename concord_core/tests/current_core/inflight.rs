use super::common::*;
use concord_core::advanced::{AuthPlacement, InflightRegistry};
use concord_core::prelude::ApiClientError;
use http::StatusCode;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
use tokio::sync::Mutex;

#[tokio::test]
async fn inflight_dedupe_does_not_merge_different_auth_identities() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "a"),
            MockResponse::text(StatusCode::OK, "b"),
        ],
    )
    .delayed(Duration::from_millis(25));
    let sent = transport.clone();
    let registry = Arc::new(InflightRegistry::default());
    let mut client_a = client(
        TestAuthVars {
            token: Some("token-a".to_string()),
            identity: "a",
        },
        transport.clone(),
    );
    let mut client_b = client(
        TestAuthVars {
            token: Some("token-b".to_string()),
            identity: "b",
        },
        transport,
    );
    configure_runtime(&mut client_a, None, None, true, Some(registry.clone()));
    configure_runtime(&mut client_b, None, None, true, Some(registry));

    let endpoint_a = TextEndpoint {
        policy: auth_policy(AuthPlacement::Bearer),
        ..Default::default()
    };
    let endpoint_b = endpoint_a.clone();

    let (a, b) = tokio::join!(
        client_a.request(endpoint_a).execute_decoded(),
        client_b.request(endpoint_b).execute_decoded()
    );

    let mut values = vec![a?.into_value(), b?.into_value()];
    values.sort();
    assert_eq!(values, vec!["a".to_string(), "b".to_string()]);
    assert_eq!(sent.sent_count().await, 2);
    Ok(())
}

#[tokio::test]
async fn inflight_follower_joins_sender_without_rate_limit_or_transport()
-> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "shared")],
    )
    .delayed(Duration::from_millis(25));
    let sent = transport.clone();
    let limiter = Arc::new(RecordingRateLimiter::new(events.clone()));
    let registry = Arc::new(InflightRegistry::default());
    let inflight_events = Arc::new(StdMutex::new(Vec::new()));
    let mut client_a = client(TestAuthVars::default(), transport.clone());
    let mut client_b = client(TestAuthVars::default(), transport);
    configure_runtime(
        &mut client_a,
        None,
        Some(limiter.clone()),
        false,
        Some(registry.clone()),
    );
    configure_runtime(&mut client_b, None, Some(limiter), false, Some(registry));
    client_a.set_inflight_policy(Arc::new(RecordingInflightPolicy::new(
        inflight_events.clone(),
    )));
    client_b.set_inflight_policy(Arc::new(RecordingInflightPolicy::new(
        inflight_events.clone(),
    )));

    let (a, b) = tokio::join!(
        client_a.request(TextEndpoint::default()).execute_decoded(),
        client_b.request(TextEndpoint::default()).execute_decoded()
    );

    assert_eq!(a?.into_value(), "shared");
    assert_eq!(b?.into_value(), "shared");
    assert_eq!(sent.sent_count().await, 1);
    let events = events.lock().await.clone();
    assert_eq!(
        events
            .iter()
            .filter(|event| event.as_str() == "rate_acquire")
            .count(),
        1
    );
    assert_eq!(inflight_events.lock().expect("inflight events").len(), 2);
    Ok(())
}

#[tokio::test]
async fn unsafe_methods_are_not_deduped_by_safe_method_policy() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "first"),
            MockResponse::text(StatusCode::OK, "second"),
        ],
    )
    .delayed(Duration::from_millis(25));
    let sent = transport.clone();
    let registry = Arc::new(InflightRegistry::default());
    let mut client_a = client(TestAuthVars::default(), transport.clone());
    let mut client_b = client(TestAuthVars::default(), transport);
    configure_runtime(&mut client_a, None, None, true, Some(registry.clone()));
    configure_runtime(&mut client_b, None, None, true, Some(registry));

    let endpoint = TextEndpoint {
        method: http::Method::POST,
        ..Default::default()
    };
    let (a, b) = tokio::join!(
        client_a.request(endpoint.clone()).execute_decoded(),
        client_b.request(endpoint).execute_decoded()
    );

    let mut values = vec![a?.into_value(), b?.into_value()];
    values.sort();
    assert_eq!(values, vec!["first".to_string(), "second".to_string()]);
    assert_eq!(sent.sent_count().await, 2);
    Ok(())
}

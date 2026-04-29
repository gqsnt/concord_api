use super::common::*;
use concord_core::advanced::{AuthPlacement, InflightRegistry};
use concord_core::prelude::ApiClientError;
use http::StatusCode;
use std::sync::Arc;
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

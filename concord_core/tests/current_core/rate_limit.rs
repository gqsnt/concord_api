use super::common::*;
use concord_core::prelude::ApiClientError;
use http::StatusCode;
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::test]
async fn rate_limit_acquire_before_send_and_observe_after_response() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let limiter = Arc::new(RecordingRateLimiter::new(events.clone()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "ok")],
    );
    let mut client = client(TestAuthVars::default(), transport);
    configure_runtime(&mut client, None, Some(limiter), false, None);

    client
        .request(TextEndpoint::default())
        .execute_decoded()
        .await?;

    let events = events.lock().await.clone();
    let acquire = events
        .iter()
        .position(|event| event == "rate_acquire")
        .expect("rate limiter acquired");
    let transport = events
        .iter()
        .position(|event| event == "transport")
        .expect("transport sent");
    let observe = events
        .iter()
        .position(|event| event == "rate_response")
        .expect("rate limiter observed response");
    assert!(acquire < transport);
    assert!(transport < observe);
    Ok(())
}

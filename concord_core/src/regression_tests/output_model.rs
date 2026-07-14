#![allow(unused_imports)]

use super::common::{DeterministicHarness, MockResponse, TestAuthVars, TextEndpoint, client};
use bytes::Bytes;
use concord_core::prelude::ApiClientError;
use http::StatusCode;
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::test]
async fn decoded_response_exposes_user_metadata() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let harness = DeterministicHarness::new(
        events,
        vec![MockResponse::text(StatusCode::CREATED, "created")],
    );
    let client = client(TestAuthVars::default(), harness);

    let decoded = client.request(TextEndpoint::default()).response().await?;

    assert_eq!(decoded.status(), StatusCode::CREATED);
    assert_eq!(decoded.headers()[http::header::CONTENT_TYPE], "text/plain");
    assert_eq!(decoded.url().as_str(), "http://example.com/text");
    assert_eq!(decoded.meta().endpoint, "Text");
    assert_eq!(decoded.value(), "created");
    assert_eq!(decoded.into_value(), "created");
    Ok(())
}

#[tokio::test]
async fn direct_await_returns_decoded_value() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let harness =
        DeterministicHarness::new(events, vec![MockResponse::text(StatusCode::OK, "await")]);
    let client = client(TestAuthVars::default(), harness);

    let value = client
        .request(TextEndpoint::default())
        .response()
        .await?
        .into_value();

    assert_eq!(value, "await");
    Ok(())
}

#[tokio::test]
async fn execute_returns_same_decoded_value_as_await() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let harness =
        DeterministicHarness::new(events, vec![MockResponse::text(StatusCode::OK, "execute")]);
    let client = client(TestAuthVars::default(), harness);

    let value = client
        .request(TextEndpoint::default())
        .response()
        .await?
        .into_value();

    assert_eq!(value, "execute");
    Ok(())
}

#[cfg(feature = "dangerous-raw-response")]
#[tokio::test]
async fn execute_raw_returns_classified_raw_response() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let harness =
        DeterministicHarness::new(events, vec![MockResponse::text(StatusCode::OK, "raw")]);
    let client = client(TestAuthVars::default(), harness);

    let response = client
        .request(TextEndpoint::default())
        .execute_raw_response()
        .await?;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.meta().endpoint, "Text");
    assert_eq!(response.url().as_str(), "http://example.com/text");
    assert_eq!(response.body(), &Bytes::from_static(b"raw"));
    Ok(())
}

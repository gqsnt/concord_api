use super::common::{
    MockResponse, MockTransport, ObservationAuthVars, ObservationRuntimeHooks,
    RecordingRateLimiter, RecordingRuntimeHooks, TextEndpoint, auth_policy, configure_runtime,
    observation_client,
};
use concord_core::advanced::AuthPlacement;
use concord_core::prelude::ApiClientError;
use http::StatusCode;
use std::sync::Arc;
use tokio::sync::Mutex;

fn positions(events: &[String], needle: &str) -> Vec<usize> {
    events
        .iter()
        .enumerate()
        .filter_map(|(index, event)| (event == needle).then_some(index))
        .collect()
}

fn first(events: &[String], needle: &str) -> usize {
    events
        .iter()
        .position(|event| event == needle)
        .unwrap_or_else(|| panic!("missing event `{needle}` in {events:?}"))
}

#[tokio::test]
async fn runtime_order_auth_recovery_visible_execution_sequence() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::UNAUTHORIZED, "challenge"),
            MockResponse::text(StatusCode::OK, "recovered"),
        ],
    );
    let sent = transport.clone();
    let mut client = observation_client(
        ObservationAuthVars::bearer_replacing(
            "initial-token",
            "replacement-token",
            "refresh",
            events.clone(),
        ),
        &transport,
    );
    client.set_runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())));
    configure_runtime(
        &mut client,
        Some(Arc::new(RecordingRateLimiter::new(events.clone()))),
    );

    let response = client
        .request(TextEndpoint {
            policy: auth_policy(AuthPlacement::Bearer),
            ..Default::default()
        })
        .response()
        .await?;

    assert_eq!(response.value(), "recovered");
    assert_eq!(sent.sent_count().await, 2);
    let events = events.lock().await.clone();

    let auth = first(&events, "auth_prepare");
    let rate = first(&events, "rate_acquire");
    let pre = first(&events, "pre_send");
    let head = first(&events, "request_head");
    let body = first(&events, "request_body_complete");
    let post = first(&events, "hook_status:401 Unauthorized");
    let feedback = first(&events, "rate_response");
    let classify = first(&events, "auth_classify:401 Unauthorized");
    let released = first(&events, "response_released");
    let refresh = first(&events, "provider_refresh");

    assert!(auth < rate && rate < pre && pre < head, "{events:?}");
    assert!(head < body && body < post && post < feedback, "{events:?}");
    assert!(
        feedback < classify && classify < released && released < refresh,
        "{events:?}"
    );
    assert_eq!(positions(&events, "auth_prepare").len(), 2);
    assert_eq!(positions(&events, "rate_acquire").len(), 2);
    assert_eq!(positions(&events, "pre_send").len(), 2);
    assert_eq!(positions(&events, "request_head").len(), 2);
    assert_eq!(positions(&events, "request_body_complete").len(), 2);
    assert_eq!(positions(&events, "provider_refresh").len(), 1);
    Ok(())
}

#[tokio::test]
async fn runtime_order_terminal_second_challenge_releases_then_invalidates_without_third_send() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::UNAUTHORIZED, "first-challenge"),
            MockResponse::text(StatusCode::UNAUTHORIZED, "second-challenge"),
        ],
    );
    let sent = transport.clone();
    let mut client = observation_client(
        ObservationAuthVars::bearer_replacing(
            "initial-token",
            "replacement-token",
            "refresh",
            events.clone(),
        ),
        &transport,
    );
    client.set_runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())));
    configure_runtime(
        &mut client,
        Some(Arc::new(RecordingRateLimiter::new(events.clone()))),
    );

    let error = client
        .request(TextEndpoint {
            policy: auth_policy(AuthPlacement::Bearer),
            ..Default::default()
        })
        .response()
        .await
        .expect_err("the second challenge must remain terminal");

    assert!(error.to_string().contains("auth challenge rejected"));
    assert_eq!(sent.sent_count().await, 2);
    let events = events.lock().await.clone();
    let posts = positions(&events, "hook_status:401 Unauthorized");
    let feedback = positions(&events, "rate_response");
    let classifications = positions(&events, "auth_classify:401 Unauthorized");
    let releases = positions(&events, "response_released");
    let invalidation = first(&events, "generation_invalidation");

    assert_eq!(posts.len(), 2);
    assert_eq!(feedback.len(), 2);
    assert_eq!(classifications.len(), 2);
    assert_eq!(releases.len(), 2);
    assert!(posts[1] < feedback[1], "{events:?}");
    assert!(feedback[1] < classifications[1], "{events:?}");
    assert!(classifications[1] < releases[1], "{events:?}");
    assert!(releases[1] < invalidation, "{events:?}");
    assert_eq!(positions(&events, "rate_acquire").len(), 2);
    assert_eq!(positions(&events, "pre_send").len(), 2);
    assert_eq!(positions(&events, "request_head").len(), 2);
    assert_eq!(positions(&events, "request_body_complete").len(), 2);
    assert_eq!(positions(&events, "provider_refresh").len(), 1);
    assert_eq!(positions(&events, "generation_invalidation").len(), 1);
}

#[tokio::test]
async fn runtime_order_success_runs_post_hook_before_rate_feedback() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "ok")],
    );
    let mut client = observation_client(
        ObservationAuthVars::bearer("token", "phase", events.clone()),
        &transport,
    );
    client.set_runtime_hooks(Arc::new(RecordingRuntimeHooks::new(events.clone())));
    configure_runtime(
        &mut client,
        Some(Arc::new(RecordingRateLimiter::new(events.clone()))),
    );

    let response = client
        .request(TextEndpoint {
            policy: auth_policy(AuthPlacement::Bearer),
            ..Default::default()
        })
        .response()
        .await?;

    assert_eq!(response.value(), "ok");
    let events = events.lock().await.clone();
    assert!(first(&events, "classify_response") < first(&events, "rate_response"));
    Ok(())
}

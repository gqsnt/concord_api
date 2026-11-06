//! Public entry-point workflows, including preservation of an earlier successful submission.

mod support;

use concord_example_testserver::Reply;
use concord_sdk_twilio::MessageStatus;
use support::*;

#[tokio::test]
async fn workflow_a_submit_and_retain_resource_identity() {
    let (server, script, client) = scripted([Reply::json(201, CREATED)]).await;
    let submission = client.create_message(message()).await.unwrap();
    assert_eq!(submission.value.sid, SID);
    assert_eq!(submission.value.account_sid, ACCOUNT);
    assert_eq!(submission.value.status, MessageStatus::Queued);
    assert!(submission.value.error_code.is_none());
    assert_eq!(submission.meta.status.as_u16(), 201);
    assert_eq!(server.single().target, CREATE_PATH);
    assert_eq!(script.remaining(), 0);
    server.shutdown().await;
}

#[tokio::test]
async fn workflow_b_fetch_submitted_sid_and_expose_processing_failure() {
    let (server, script, client) =
        scripted([Reply::json(201, CREATED), Reply::json(200, UNDELIVERED)]).await;
    let submission = client.create_message(message()).await.unwrap();
    let observation = client.get_message(&submission.value.sid).await.unwrap();
    assert_eq!(observation.value.sid, submission.value.sid);
    assert_eq!(observation.value.status, MessageStatus::Undelivered);
    assert_eq!(observation.value.error_code, Some(30007));
    assert_eq!(
        observation.value.error_message.as_deref(),
        Some("Synthetic carrier filtering failure")
    );
    assert_eq!(observation.meta.status.as_u16(), 200);
    assert_eq!(submission.value.status, MessageStatus::Queued); // Earlier observation is retained.
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].body, FORM);
    assert_eq!(requests[1].target, FETCH_PATH);
    assert_eq!(script.remaining(), 0);
    server.shutdown().await;
}

#[tokio::test]
async fn failed_status_lookup_preserves_prior_submission_for_caller() {
    let (server, script, client) =
        scripted([Reply::json(201, CREATED), Reply::json(404, NOT_FOUND)]).await;
    let submission = client.create_message(message()).await.unwrap();
    let failure = client.get_message(&submission.value.sid).await.unwrap_err();
    assert_eq!(failure.meta().unwrap().status.as_u16(), 404);
    assert!(failure.provider_error().is_some());
    assert_eq!(submission.value.sid, SID);
    assert_eq!(submission.meta.status.as_u16(), 201);
    assert_eq!(server.requests().len(), 2); // No re-submission or polling hidden in either call.
    assert_eq!(script.remaining(), 0);
    server.shutdown().await;
}

#[tokio::test]
async fn rejected_submission_returns_provider_error_without_lookup() {
    let (server, script, client) = scripted([Reply::json(400, BAD_REQUEST)]).await;
    let failure = client.create_message(message()).await.unwrap_err();
    assert_eq!(failure.meta().unwrap().status.as_u16(), 400);
    assert_eq!(failure.provider_error().unwrap().code, Some(21201));
    assert_eq!(server.single().method, "POST");
    assert_eq!(script.remaining(), 0);
    server.shutdown().await;
}

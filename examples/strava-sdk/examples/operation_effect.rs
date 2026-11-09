//! Offline caller: the same activity read, with optional operation inspection.
use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_strava::{Credentials, Session, StravaOperation, Tokens};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let server = TestServer::start(|request: &Recorded| {
        assert_eq!(request.method, "GET");
        assert_eq!(request.path(), "/api/v3/activities/9007199254740999");
        assert!(request.header("authorization").is_some());
        Reply::json(200, include_str!("../fixtures/activity.json"))
    })
    .await;
    let tokens: Tokens = serde_json::from_str(include_str!("../fixtures/tokens.json"))?;
    let session = Session::connect(
        &server.url(),
        &server.url(),
        Credentials::new(123, "fixture-client-secret".into())?,
        tokens,
        Duration::from_secs(3),
    )?;
    let api = session.client();

    // Inspection is optional; the usual caller still just navigates and sends.
    assert!(StravaOperation::ActivitiesGet.operation().is_read());
    assert!(!StravaOperation::ActivitiesUpdate.operation().is_read());
    let activity = api.activities().get(9007199254740999).send().await?;
    println!("{}: {}", activity.value.id, activity.value.name);

    // The application persists the latest tokens after calls, including failures.
    let _tokens_to_persist = session.latest_tokens().await;
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
    Ok(())
}

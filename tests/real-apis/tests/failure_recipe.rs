//! Run the guide's local failure and status-arm recipe as printed.
// PANIC-SAFETY: Unwraps make unexpected scripted outcomes fail the test.
#![allow(clippy::unwrap_used)]

use concord::error::Kind;
use concord_testkit::Script;

include!("../fixtures/recipes/failure_stream.rs");
include!("../fixtures/recipes/failure_ticket.rs");

#[tokio::test(start_paused = true)]
async fn the_failure_recipe_preserves_the_owner_and_response_evidence() {
    let guide = include_str!("../../../docs/guide/responses-and-errors.md");
    for recipe in [
        include_str!("../fixtures/recipes/failure_stream.rs"),
        include_str!("../fixtures/recipes/failure_ticket.rs"),
    ] {
        assert!(
            guide.contains(&format!("```rust\n{recipe}```")),
            "the guide must print each compiled recipe"
        );
    }

    let script = Script::new();
    script
        .answer(200)
        .header("content-type", "text/event-stream")
        .body("event: message_start\ndata: start\n\nevent: message_stop\ndata: stop\n\n");
    script
        .answer(200)
        .header("content-type", "text/event-stream")
        .body("event: unknown\ndata: private\n\n");
    let messages = Messages::new().with_transport(script.clone());
    let mut events = messages.stream().await.unwrap();
    assert_eq!(events.try_next().await.unwrap(), Some(MessageEvent::Start));
    assert!(matches!(
        finish(events).await,
        Err(FinishError::StartAlreadyConsumed)
    ));
    match finish(messages.stream().await.unwrap()).await {
        Err(FinishError::Stream(error)) => {
            assert_eq!(error.endpoint(), "messages.stream");
            assert_eq!(error.reason(), Some("unexpected message event"));
            assert!(error.is_status(200));
        }
        other => assert!(matches!(other, Err(FinishError::Stream(_)))),
    }
    assert_eq!(script.requests().len(), 2);

    let script = Script::new();
    script.answer(503);
    script
        .answer(200)
        .header("x-trace", "kept")
        .body(r#"{"file_id":"private"}"#);
    let files = Files::new()
        .with_transport(script.clone())
        .with_retry(concord::Retry::standard());
    let error = files.upload_ticket().await.unwrap_err();
    assert_eq!(error.kind(), Kind::Mismatch);
    assert_eq!(
        error.reason(),
        Some("the upload ticket is missing its URL or file id")
    );
    assert_eq!(error.endpoint(), "files.upload_ticket");
    assert_eq!(error.tries(), 2);
    assert!(error.is_status(200));
    assert_eq!(error.headers().unwrap().get("x-trace"), Some("kept"));
    assert!(!format!("{error} {error:?}").contains("private"));
    assert_eq!(script.requests().len(), 2);
}

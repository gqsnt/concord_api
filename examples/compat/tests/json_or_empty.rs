//! Loopback compatibility evidence for an endpoint that answers JSON or nothing (C10): the head
//! decides, before the body is read.
mod support;
use concord::ErrorKind;
use concord_example_compat::{Playback, Player};
use support::{http, Reply, Server};

async fn playing(reply: Reply) -> Result<Option<Playback>, concord::Error> {
    let server = Server::reply(reply).await;
    let api = Player::new(http()).with_base_url(server.url()).unwrap();
    api.playing().send().await
}

#[tokio::test]
async fn json_or_no_content() {
    assert_eq!(
        playing(Reply::json(200, r#"{"track":"Intro"}"#))
            .await
            .unwrap(),
        Some(Playback {
            track: "Intro".into()
        })
    );
    // A bodyless 204 needs no `Content-Type`.
    assert_eq!(playing(Reply::new(204)).await.unwrap(), None);
    // `null` in a JSON 200 is `None` too.
    assert_eq!(playing(Reply::json(200, "null")).await.unwrap(), None);
    // A 200 must still be JSON-labelled: refused before its body is read.
    let error = playing(Reply::new(200).body(r#"{"track":"Intro"}"#))
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Metadata);
    // An empty JSON 200 is not a document: a decode error (the 204 rule is about the status).
    let error = playing(Reply::json(200, "")).await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Decode);
    // A malformed document is still a decode error.
    let error = playing(Reply::json(200, "{")).await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Decode);
}

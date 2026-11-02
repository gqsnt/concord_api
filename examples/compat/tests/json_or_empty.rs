//! Loopback compatibility evidence for an endpoint that answers JSON or nothing.
mod support;
use concord_example_compat::{Playback, Player};
use support::{http, Reply, Server};

#[tokio::test]
async fn json_or_no_content() {
    let server = Server::reply(Reply::json(200, r#"{"track":"Intro"}"#)).await;
    let api = Player::new(http()).with_base_url(server.url()).unwrap();
    assert_eq!(
        api.playing().send().await.unwrap(),
        Some(Playback {
            track: "Intro".into()
        })
    );
    let server = Server::reply(Reply::new(204)).await;
    let api = Player::new(http()).with_base_url(server.url()).unwrap();
    assert_eq!(api.playing().send().await.unwrap(), None);
    // A malformed document is still a decode error.
    let server = Server::reply(Reply::json(200, "{")).await;
    let api = Player::new(http()).with_base_url(server.url()).unwrap();
    assert!(api.playing().send().await.is_err());
}

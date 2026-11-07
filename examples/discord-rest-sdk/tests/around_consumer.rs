//! Public consumer: get a page centered on an existing message from a local fixture.
use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_discord_rest::{
    Anchor, ChannelId, Discord, DiscordConfig, MessageId, MessageLimit, RateLimiter,
    concord::reqwest,
};

#[tokio::test]
async fn read_messages_around_an_id() {
    let server = TestServer::start(|request: &Recorded| -> Reply {
        assert_eq!(request.method, "GET");
        assert_eq!(request.path(), "/api/v10/channels/123/messages");
        assert_eq!(request.query_all("around"), ["456"]);
        assert_eq!(request.query_all("limit"), ["2"]);
        assert_eq!(request.query_pairs().len(), 2);
        assert_eq!(request.header("authorization"), Some("Bot fixture-token"));
        Reply::json(200, r#"[{"id":"456","channel_id":"123","author":{"id":"7","username":"fixture"},"content":"center","timestamp":"2026-11-07T00:00:00Z","tts":false,"mention_everyone":false,"pinned":false,"type":0}]"#)
    }).await;
    let discord = Discord::new(
        reqwest::Client::builder().no_proxy().build().unwrap(),
        DiscordConfig {
            token: "fixture-token".into(),
            limiter: RateLimiter::new(),
        },
    )
    .with_base_url(format!("{}/api/v10", server.url()))
    .unwrap();
    let messages = discord
        .channel(ChannelId::new(123))
        .messages()
        .anchor(Anchor::Around(MessageId::new(456)))
        .set_limit(MessageLimit::new(2).unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].id, MessageId::new(456));
    assert_eq!(messages[0].content, "center");
    assert_eq!(server.requests().len(), 1);
    assert_eq!(server.shutdown().await.open_after, 0);
}

//! Verify every cursor transition on prepared requests, before any network I/O.
use concord_sdk_discord_rest::{
    Anchor, ChannelId, Discord, DiscordConfig, MessageId, RateLimiter, concord::reqwest,
};

#[test]
fn every_cursor_replaces_every_other_and_can_be_cleared() {
    let channel = Discord::new(
        reqwest::Client::new(),
        DiscordConfig {
            token: "fixture-token".into(),
            limiter: RateLimiter::new(),
        },
    )
    .channel(ChannelId::new(123));
    let choices = [
        (Anchor::Around(MessageId::new(11)), "around", "11"),
        (Anchor::Before(MessageId::new(22)), "before", "22"),
        (Anchor::After(MessageId::new(33)), "after", "33"),
    ];
    for (first, _, _) in choices {
        for (second, name, value) in choices {
            let prepared = channel
                .messages()
                .anchor(first)
                .anchor(second)
                .prepare()
                .unwrap();
            let pairs: Vec<_> = prepared
                .request()
                .url()
                .query_pairs()
                .into_owned()
                .collect();
            assert_eq!(pairs, [(name.to_owned(), value.to_owned())]);
        }
        let cleared = channel
            .messages()
            .anchor(first)
            .maybe_anchor(None)
            .prepare()
            .unwrap();
        assert!(cleared.request().url().query().is_none());
    }
}

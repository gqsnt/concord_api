#![allow(dead_code)]

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_discord_rest::concord::reqwest;
use concord_sdk_discord_rest::{
    ChannelId, DEFAULT_USER_AGENT, Discord, DiscordConfig, GuildId, MessageId, RateLimiter,
};

/// A synthetic bot token (never a real credential).
pub const TOKEN: &str = "synthetic-bot-token.Concord.000";

/// IDs from the official channel examples (guild, category, text channel).
pub const GUILD: GuildId = GuildId::new(290926798629997250);
pub const CATEGORY: ChannelId = ChannelId::new(399942396007890945);
pub const GENERAL: ChannelId = ChannelId::new(41771983423143937);
/// A second, synthetic text channel.
pub const SECOND: ChannelId = ChannelId::new(41771983423143938);
/// The channel and message of the official message example.
pub const EXAMPLE_CHANNEL: ChannelId = ChannelId::new(290926798999357250);
pub const EXAMPLE_MESSAGE: MessageId = MessageId::new(334385199974967042);

/// A monotonic clock the test advances by hand.
#[derive(Clone)]
pub struct ManualClock {
    start: Instant,
    offset: Arc<Mutex<Duration>>,
}

impl ManualClock {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
            offset: Arc::default(),
        }
    }

    pub fn advance(&self, by: Duration) {
        *self.offset.lock().unwrap() += by;
    }

    pub fn limiter(&self) -> RateLimiter {
        let clock = self.clone();
        RateLimiter::with_clock(move || clock.start + *clock.offset.lock().unwrap())
    }
}

/// A transport with no proxy, no redirects and no reqwest-level retry, so every request on the
/// wire is one the SDK made.
pub fn http() -> reqwest::Client {
    http_builder().build().unwrap()
}

pub fn http_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
}

pub fn discord(server: &TestServer, limiter: RateLimiter) -> Discord {
    discord_with(http(), &server.url(), limiter)
}

pub fn discord_with(http: reqwest::Client, origin: &str, limiter: RateLimiter) -> Discord {
    Discord::new(
        http,
        DiscordConfig {
            token: TOKEN.into(),
            limiter,
        },
    )
    .with_base_url(format!("{origin}/api/v10"))
    .unwrap()
}

/// The headers every request carries.
pub fn assert_common(request: &Recorded, method: &str, target: &str) {
    assert_eq!(request.method, method);
    assert_eq!(request.target, target);
    assert_eq!(
        request.header_all("authorization"),
        [format!("Bot {TOKEN}")]
    );
    assert_eq!(request.header_all("user-agent"), [DEFAULT_USER_AGENT]);
    assert_eq!(request.header_all("accept"), ["application/json"]);
}

pub fn assert_get(request: &Recorded, target: &str) {
    assert_common(request, "GET", target);
    assert_eq!(request.header("content-type"), None);
    assert_eq!(request.body_len, 0);
}

/// Route headers as in the rate-limits page's example (`X-RateLimit-*`).
pub fn with_bucket(reply: Reply, bucket: &str, remaining: u32, reset_after: &str) -> Reply {
    reply
        .header("x-ratelimit-limit", "5")
        .header("x-ratelimit-remaining", remaining.to_string())
        .header("x-ratelimit-reset", "1470173023")
        .header("x-ratelimit-reset-after", reset_after)
        .header("x-ratelimit-bucket", bucket)
}

/// A synthetic message object (shape of the message page's example; values invented).
pub fn message_json(id: u64, channel: ChannelId, content: &str, nonce: Option<&str>) -> String {
    let mut message = serde_json::json!({
        "id": id.to_string(),
        "channel_id": channel.to_string(),
        "author": {
            "id": "53908099506183681",
            "username": "synthetic-bot",
            "discriminator": "0",
            "global_name": null,
            "bot": true
        },
        "content": content,
        "timestamp": "2026-11-07T09:00:00.000000+00:00",
        "edited_timestamp": null,
        "tts": false,
        "mention_everyone": false,
        "mentions": [],
        "mention_roles": [],
        "attachments": [],
        "embeds": [],
        "pinned": false,
        "type": 0
    });
    if let Some(nonce) = nonce {
        message["nonce"] = nonce.into();
    }
    message.to_string()
}

pub fn fixture(name: &str) -> &'static [u8] {
    match name {
        "guild-channels" => include_bytes!("../../fixtures/guild-channels.json"),
        "message" => include_bytes!("../../fixtures/message.json"),
        "message-numeric-id" => include_bytes!("../../fixtures/message-numeric-id.json"),
        "message-truncated" => include_bytes!("../../fixtures/message-truncated.json"),
        "rate-limit-user" => include_bytes!("../../fixtures/rate-limit-user.json"),
        "rate-limit-shared" => include_bytes!("../../fixtures/rate-limit-shared.json"),
        "rate-limit-global" => include_bytes!("../../fixtures/rate-limit-global.json"),
        "error-invalid-form-body" => include_bytes!("../../fixtures/error-invalid-form-body.json"),
        "error-invalid-token" => include_bytes!("../../fixtures/error-invalid-token.json"),
        "error-missing-access" => include_bytes!("../../fixtures/error-missing-access.json"),
        "error-missing-permissions" => {
            include_bytes!("../../fixtures/error-missing-permissions.json")
        }
        "error-unknown-channel" => include_bytes!("../../fixtures/error-unknown-channel.json"),
        "error-unknown-message" => include_bytes!("../../fixtures/error-unknown-message.json"),
        "error-empty-message" => include_bytes!("../../fixtures/error-empty-message.json"),
        "error-method-not-allowed" => {
            include_bytes!("../../fixtures/error-method-not-allowed.json")
        }
        "error-marker" => include_bytes!("../../fixtures/error-marker.json"),
        "error-foreign-shape" => include_bytes!("../../fixtures/error-foreign-shape.json"),
        "bad-gateway" => include_bytes!("../../fixtures/bad-gateway.html"),
        _ => panic!("unknown fixture {name}"),
    }
}

/// The documented 429 examples, headers and body as on the rate-limits page.
pub fn documented_429(kind: &str) -> Reply {
    match kind {
        "user" => Reply::json(429, fixture("rate-limit-user"))
            .header("retry-after", "65")
            .header("x-ratelimit-limit", "10")
            .header("x-ratelimit-remaining", "0")
            .header("x-ratelimit-reset", "1470173023.123")
            .header("x-ratelimit-reset-after", "64.57")
            .header("x-ratelimit-bucket", "abcd1234")
            .header("x-ratelimit-scope", "user"),
        "shared" => Reply::json(429, fixture("rate-limit-shared"))
            .header("retry-after", "1337")
            .header("x-ratelimit-limit", "10")
            .header("x-ratelimit-remaining", "9")
            .header("x-ratelimit-reset", "1470173023.123")
            .header("x-ratelimit-reset-after", "64.57")
            .header("x-ratelimit-bucket", "abcd1234")
            .header("x-ratelimit-scope", "shared"),
        "global" => Reply::json(429, fixture("rate-limit-global"))
            .header("retry-after", "65")
            .header("x-ratelimit-global", "true")
            .header("x-ratelimit-scope", "global"),
        _ => panic!("unknown 429 {kind}"),
    }
}

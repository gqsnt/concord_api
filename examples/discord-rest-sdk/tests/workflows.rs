//! The two connected workflows end to end against a local fake of Discord, and the slice's
//! required failures: two channels sharing a bucket hash, a global 429, and a timeout after a
//! message creation. No Discord request is made.

mod support;

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use concord::ErrorKind;
use concord_example_testserver::{Gate, Recorded, Reply, TestServer};
use concord_sdk_discord_rest::pagination::Resume;
use concord_sdk_discord_rest::{
    Anchor, ChannelId, CreateMessage, Error, ErrorClass, GateScope, JsonErrorCode, MessageId,
    MessageLimit, Nonce, RateLimitScope, RateLimiter, Resource, concord,
};
use support::{
    GENERAL, GUILD, ManualClock, SECOND, assert_common, assert_get, discord, discord_with,
    documented_429, fixture, http_builder, message_json, with_bucket,
};

/// The newest message of `general` (the channel example's `last_message_id`).
const NEWEST: u64 = 155117677105512449;

/// Messages of one channel by ID: content and nonce.
type Stored = BTreeMap<u64, (String, Option<String>)>;

/// A stateful fake of the four routes, implementing the documented listing semantics
/// (newest to oldest, `before`/`after`/`around`, `limit` default 50) and `enforce_nonce`.
#[derive(Clone, Default)]
struct Fake {
    channels: Arc<Mutex<BTreeMap<u64, Stored>>>,
    /// Holds the head of the first `POST` until opened (`None`: answer at once).
    hold_first_post: Option<Gate>,
    posts: Arc<AtomicUsize>,
}

impl Fake {
    /// `general` holds five synthetic messages, `NEWEST - 4 ..= NEWEST`.
    fn seeded() -> Self {
        let fake = Self::default();
        let messages = (0..5)
            .map(|i| (NEWEST - 4 + i, (format!("synthetic message {i}"), None)))
            .collect();
        fake.channels
            .lock()
            .unwrap()
            .insert(GENERAL.get(), messages);
        fake
    }

    fn respond(&self, r: &Recorded) -> Reply {
        let path: Vec<&str> = r
            .path()
            .trim_start_matches("/api/v10/")
            .split('/')
            .collect();
        let reply = match (r.method.as_str(), path.as_slice()) {
            ("GET", ["guilds", _, "channels"]) => Reply::json(200, fixture("guild-channels")),
            ("GET", ["channels", channel, "messages"]) => self.list(r, channel.parse().unwrap()),
            ("GET", ["channels", channel, "messages", id]) => {
                let channel: u64 = channel.parse().unwrap();
                let id: u64 = id.parse().unwrap();
                match self
                    .channels
                    .lock()
                    .unwrap()
                    .get(&channel)
                    .and_then(|m| m.get(&id))
                {
                    Some((content, nonce)) => Reply::json(
                        200,
                        message_json(id, ChannelId::new(channel), content, nonce.as_deref()),
                    ),
                    None => Reply::json(404, fixture("error-unknown-message")),
                }
            }
            ("POST", ["channels", channel, "messages"]) => self.create(r, channel.parse().unwrap()),
            _ => Reply::json(405, fixture("error-method-not-allowed")),
        };
        with_bucket(reply, "abcd1234", 4, "1")
    }

    fn list(&self, r: &Recorded, channel: u64) -> Reply {
        let channels = self.channels.lock().unwrap();
        let Some(messages) = channels.get(&channel) else {
            return Reply::json(404, fixture("error-unknown-channel"));
        };
        let one = |name: &str| r.query_all(name).first().map(|v| v.parse::<u64>().unwrap());
        let limit = one("limit").unwrap_or(50) as usize;
        let mut ids: Vec<u64> = match (one("before"), one("after"), one("around")) {
            (Some(before), None, None) => messages
                .range(..before)
                .map(|(id, _)| *id)
                .rev()
                .take(limit)
                .collect(),
            (None, Some(after), None) => {
                let mut newer: Vec<u64> = messages
                    .range(after + 1..)
                    .map(|(id, _)| *id)
                    .take(limit)
                    .collect();
                newer.reverse();
                newer
            }
            (None, None, Some(_)) => panic!("around is not used by these workflows"),
            (None, None, None) => messages.keys().rev().take(limit).copied().collect(),
            _ => return Reply::json(400, fixture("error-invalid-form-body")),
        };
        ids.truncate(limit);
        let body: Vec<String> = ids
            .iter()
            .map(|id| {
                let (content, nonce) = &messages[id];
                message_json(*id, ChannelId::new(channel), content, nonce.as_deref())
            })
            .collect();
        Reply::json(200, format!("[{}]", body.join(",")))
    }

    fn create(&self, r: &Recorded, channel: u64) -> Reply {
        let first = self.posts.fetch_add(1, Ordering::SeqCst) == 0;
        let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
        let content = body["content"].as_str().unwrap().to_owned();
        let nonce = body["nonce"].as_str().map(str::to_owned);
        let mut channels = self.channels.lock().unwrap();
        let messages = channels.entry(channel).or_default();
        // `enforce_nonce`: the same author's earlier message with this nonce is returned.
        let existing = (body["enforce_nonce"] == true)
            .then(|| {
                messages
                    .iter()
                    .find(|(_, (_, n))| n.is_some() && *n == nonce)
                    .map(|(id, _)| *id)
            })
            .flatten();
        let id = existing.unwrap_or_else(|| {
            let id = messages
                .keys()
                .next_back()
                .map_or(NEWEST + 1, |last| last + 1);
            messages.insert(id, (content.clone(), nonce.clone()));
            id
        });
        let (content, nonce) = &messages[&id];
        let reply = Reply::json(
            200,
            message_json(id, ChannelId::new(channel), content, nonce.as_deref()),
        );
        match (&self.hold_first_post, first) {
            (Some(gate), true) => reply.after(gate.clone()),
            _ => reply,
        }
    }

    fn stored(&self, channel: ChannelId) -> usize {
        self.channels
            .lock()
            .unwrap()
            .get(&channel.get())
            .map_or(0, BTreeMap::len)
    }

    async fn serve(&self) -> TestServer {
        let fake = self.clone();
        TestServer::start(move |r| fake.respond(r)).await
    }
}

/// Workflow 1: list a guild's channels, page messages in a selected channel, fetch one by ID.
#[tokio::test]
async fn channels_then_message_history_then_one_message() {
    let server = Fake::seeded().serve().await;
    let discord = discord(&server, RateLimiter::new());

    let channels = discord.guild(GUILD).channels().send().await.unwrap();
    let selected = channels
        .iter()
        .find(|channel| channel.kind.is_guild_text())
        .expect("a text channel");
    assert_eq!(selected.id, GENERAL);
    assert_eq!(selected.last_message_id, Some(MessageId::new(NEWEST)));

    let channel = discord.channel(selected.id);
    let history = channel
        .history(MessageLimit::new(2).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap();
    let ids: Vec<u64> = history.iter().map(|m| m.id.get()).collect();
    assert_eq!(
        ids,
        [NEWEST, NEWEST - 1, NEWEST - 2, NEWEST - 3, NEWEST - 4],
        "newest first"
    );

    let picked = &history[2];
    let message = channel.message(picked.id).send().await.unwrap();
    assert_eq!(message.id, picked.id);
    assert_eq!(message.content, "synthetic message 2");
    assert_eq!(message.channel_id, GENERAL);

    let requests = server.requests();
    let base = "/api/v10/channels/41771983423143937/messages";
    assert_get(&requests[0], "/api/v10/guilds/290926798629997250/channels");
    assert_get(&requests[1], &format!("{base}?limit=2"));
    assert_get(
        &requests[2],
        &format!("{base}?before={}&limit=2", NEWEST - 1),
    );
    assert_get(
        &requests[3],
        &format!("{base}?before={}&limit=2", NEWEST - 3),
    );
    assert_get(&requests[4], &format!("{base}/{}", NEWEST - 2));
    assert_eq!(requests.len(), 5, "the short third page ended the walk");

    // A history that is an exact multiple of the page size ends on an empty page.
    let all = channel
        .history(MessageLimit::new(5).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(all.len(), 5);
    let requests = server.requests();
    assert_get(
        &requests[6],
        &format!("{base}?before={}&limit=5", NEWEST - 4),
    );
    assert_eq!(requests.len(), 7);
    server.shutdown().await;
}

/// Workflow 2: create a message through the fake transport and read it back by the returned ID.
#[tokio::test]
async fn create_a_message_then_read_it_back_by_returned_id() {
    let fake = Fake::seeded();
    let server = fake.serve().await;
    let channel = discord(&server, RateLimiter::new()).channel(GENERAL);

    let body = CreateMessage::text("deploy 42 finished")
        .unwrap()
        .with_nonce(Nonce::new("deploy-42").unwrap());
    let created = channel.create_message(body).send().await.unwrap();
    assert_eq!(created.id, MessageId::new(NEWEST + 1));
    assert_eq!(created.channel_id, GENERAL);

    let read = channel.message(created.id).send().await.unwrap();
    assert_eq!(read.id, created.id);
    assert_eq!(read.content, "deploy 42 finished");
    assert_eq!(read.nonce, created.nonce);
    assert_eq!(fake.stored(GENERAL), 6);

    let requests = server.requests();
    assert_common(
        &requests[0],
        "POST",
        "/api/v10/channels/41771983423143937/messages",
    );
    assert_eq!(
        requests[0].body_text(),
        r#"{"content":"deploy 42 finished","nonce":"deploy-42","enforce_nonce":true}"#
    );
    assert_get(
        &requests[1],
        &format!(
            "/api/v10/channels/41771983423143937/messages/{}",
            created.id
        ),
    );

    // An ID that does not exist is Discord's 404 / 10008, on this channel.
    let missing = channel.message(MessageId::new(1)).send().await.unwrap_err();
    let Error::Api(api) = &missing else {
        panic!("{missing:?}")
    };
    assert_eq!(api.class(), ErrorClass::NotFound);
    assert_eq!(api.code(), Some(JsonErrorCode::UNKNOWN_MESSAGE));
    assert_eq!(missing.resource(), Some(Resource::Channel(GENERAL)));
    server.shutdown().await;
}

/// Required failure: two channels sharing a bucket hash keep separate windows and their own
/// resource context, in both directions.
#[tokio::test]
async fn two_channels_sharing_a_bucket_hash_keep_their_resource_context() {
    let clock = ManualClock::new();
    let second_lists = Arc::new(AtomicUsize::new(0));
    let lists = second_lists.clone();
    let server = TestServer::start(move |r| {
        let general = r.path().contains(&GENERAL.to_string());
        let channel = if general { GENERAL } else { SECOND };
        if r.path().ends_with("/messages") {
            if general {
                // `general` exhausts the shared bucket for itself.
                return with_bucket(Reply::json(200, "[]"), "abcd1234", 0, "1.5");
            }
            if lists.fetch_add(1, Ordering::SeqCst) == 0 {
                return with_bucket(Reply::json(200, "[]"), "abcd1234", 4, "1.5");
            }
            return documented_429("shared");
        }
        with_bucket(
            Reply::json(200, message_json(9, channel, "m", None)),
            "abcd1234",
            3,
            "1.5",
        )
    })
    .await;
    let discord = discord(&server, clock.limiter());
    let (general, second) = (discord.channel(GENERAL), discord.channel(SECOND));
    let sent = || server.requests().len();
    let message = MessageId::new(9);

    general.message(message).send().await.unwrap(); // learns: `message` route -> abcd1234
    general.messages().send().await.unwrap(); // abcd1234 for general: 0 left for 1.5 s
    second.messages().send().await.unwrap(); // abcd1234 for second: 4 left
    assert_eq!(sent(), 3);

    // `general` is gated on both routes of the shared bucket, and the refusal names it.
    for error in [
        general.messages().send().await.unwrap_err(),
        general.message(message).send().await.unwrap_err(),
    ] {
        let Error::Gated(gated) = &error else {
            panic!("{error:?}")
        };
        assert_eq!(gated.route.resource(), Some(Resource::Channel(GENERAL)));
        let GateScope::Bucket { bucket } = &gated.scope else {
            panic!("{error:?}")
        };
        assert_eq!(bucket.as_ref().unwrap().as_str(), "abcd1234");
        assert_eq!(gated.retry_after, Duration::from_millis(1500));
        assert!(
            error.to_string().contains("channel 41771983423143937"),
            "{error}"
        );
    }
    assert_eq!(sent(), 3, "gated requests never left");

    // `second` shares the hash but not the window.
    second.message(message).send().await.unwrap();
    let error = second.messages().send().await.unwrap_err();
    let Error::RateLimited(limited) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(limited.scope, RateLimitScope::Shared);
    assert_eq!(limited.route.resource(), Some(Resource::Channel(SECOND)));
    assert_eq!(limited.bucket.as_ref().unwrap().as_str(), "abcd1234");
    assert_eq!(sent(), 5);

    // After 1.5 s `general` is admitted again, while `second`'s shared 429 (1337 s) holds.
    clock.advance(Duration::from_millis(1500));
    general.messages().send().await.unwrap();
    let error = second.messages().send().await.unwrap_err();
    let Error::Gated(gated) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(gated.route.resource(), Some(Resource::Channel(SECOND)));
    assert_eq!(gated.retry_after, Duration::from_millis(1_337_000 - 1500));
    assert_eq!(sent(), 6);
    server.shutdown().await;
}

/// Required failure: a global 429 gates the whole bot, every route and resource, for the larger
/// of its two delays.
#[tokio::test]
async fn a_global_429_gates_the_bot_globally() {
    let clock = ManualClock::new();
    let first = Arc::new(AtomicUsize::new(0));
    let calls = first.clone();
    let server = TestServer::start(move |_| {
        if calls.fetch_add(1, Ordering::SeqCst) == 0 {
            documented_429("global")
        } else {
            Reply::json(200, fixture("guild-channels"))
        }
    })
    .await;
    let discord = discord(&server, clock.limiter());

    let error = discord
        .channel(GENERAL)
        .message(MessageId::new(9))
        .send()
        .await
        .unwrap_err();
    let Error::RateLimited(limited) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(limited.scope, RateLimitScope::Global);
    assert_eq!(limited.retry_after, Some(Duration::from_secs(65)));

    let other_routes = [
        discord.guild(GUILD).channels().send().await.unwrap_err(),
        discord.channel(SECOND).messages().send().await.unwrap_err(),
        discord
            .channel(SECOND)
            .create_message(CreateMessage::text("held").unwrap())
            .send()
            .await
            .unwrap_err(),
    ];
    for error in &other_routes {
        let Error::Gated(gated) = error else {
            panic!("{error:?}")
        };
        assert_eq!(gated.scope, GateScope::Global);
        assert_eq!(gated.retry_after, Duration::from_secs(65));
        assert!(!error.is_ambiguous_write(), "a gated write was never sent");
    }
    assert_eq!(other_routes[0].resource(), Some(Resource::Guild(GUILD)));
    assert_eq!(server.requests().len(), 1);

    // The body's 64.57 s is not enough: the header said 65.
    clock.advance(Duration::from_millis(64_600));
    let error = discord.guild(GUILD).channels().send().await.unwrap_err();
    assert_eq!(error.retry_after(), Some(Duration::from_millis(400)));
    clock.advance(Duration::from_millis(400));
    let channels = discord.guild(GUILD).channels().send().await.unwrap();
    assert_eq!(channels.len(), 3);
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

/// A rate limit in the middle of a history walk ends it with a resumable position; resuming
/// after the delay completes the history without gaps or duplicates.
#[tokio::test]
async fn a_rate_limited_history_page_resumes_from_its_failed_position() {
    let clock = ManualClock::new();
    let fake = Fake::seeded();
    let pages = Arc::new(AtomicUsize::new(0));
    let count = pages.clone();
    let inner = fake.clone();
    let server = TestServer::start(move |r| {
        if r.path().ends_with("/messages") && count.fetch_add(1, Ordering::SeqCst) == 1 {
            return documented_429("user");
        }
        inner.respond(r)
    })
    .await;
    let channel = discord(&server, clock.limiter()).channel(GENERAL);
    let limit = MessageLimit::new(2).unwrap();

    let incomplete = channel
        .history(limit)
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 2);
    let failed = incomplete.error.resume().cloned().unwrap();
    assert_eq!(failed, Resume::At(MessageId::new(NEWEST - 1)));
    let error = incomplete.error.provider_error().unwrap();
    assert_eq!(error.retry_after(), Some(Duration::from_secs(65)));

    clock.advance(Duration::from_secs(65));
    let rest = channel
        .history_from(failed, limit)
        .items()
        .collect_complete()
        .await
        .unwrap();
    let ids: Vec<u64> = incomplete
        .collected
        .iter()
        .chain(&rest)
        .map(|m| m.id.get())
        .collect();
    assert_eq!(
        ids,
        [NEWEST, NEWEST - 1, NEWEST - 2, NEWEST - 3, NEWEST - 4]
    );
    server.shutdown().await;
}

/// Required failure: a timeout after a message creation is an ambiguous write. The SDK never
/// resends; the caller reconciles by reading, and a deliberate resend with the same nonce
/// (`enforce_nonce`) returns the existing message instead of creating a second one.
#[tokio::test]
async fn a_timeout_after_create_is_ambiguous_and_never_resent() {
    let gate = Gate::new();
    let fake = Fake {
        hold_first_post: Some(gate.clone()),
        ..Fake::seeded()
    };
    let server = fake.serve().await;
    let http = http_builder()
        .timeout(Duration::from_millis(300))
        .build()
        .unwrap();
    let channel = discord_with(http, &server.url(), RateLimiter::new()).channel(GENERAL);
    let body = CreateMessage::text("deploy 43 finished")
        .unwrap()
        .with_nonce(Nonce::new("deploy-43").unwrap());

    let error = channel
        .create_message(body.clone())
        .send()
        .await
        .unwrap_err();
    let Error::AmbiguousWrite(write) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(write.accepted, None, "no response head arrived");
    assert_eq!(write.error.kind(), ErrorKind::Transport);
    assert!(write.error.transport().unwrap().is_timeout());
    assert_eq!(write.route.resource(), Some(Resource::Channel(GENERAL)));
    assert!(error.status().is_none() && error.retry_after().is_none());

    // The request did reach the fake, which created the message; nothing was resent.
    assert_eq!(fake.stored(GENERAL), 6);
    let resent = server
        .wait_until(Duration::from_millis(500), |s| s.requests().len() > 1)
        .await;
    assert!(!resent, "the SDK never resends a write");

    // Reconcile by reading what is newer than the last message the caller knew.
    let newer = channel
        .messages()
        .anchor(Anchor::After(MessageId::new(NEWEST)))
        .send()
        .await
        .unwrap();
    let found = newer
        .iter()
        .find(|m| m.nonce.as_ref() == body.nonce())
        .expect("the ambiguous write happened");

    // A deliberate resend with the same nonce returns that message; no duplicate.
    let again = channel.create_message(body).send().await.unwrap();
    assert_eq!(again.id, found.id);
    assert_eq!(fake.stored(GENERAL), 6);
    let posts = server
        .requests()
        .into_iter()
        .filter(|r| r.method == "POST")
        .count();
    assert_eq!(
        posts, 2,
        "the caller's one deliberate resend, never the SDK's"
    );
    gate.open();
    server.shutdown().await;
}

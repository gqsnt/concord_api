//! Independent caller workflows built from public documentation and loopback fixtures.

use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use concord_example_testserver::{Recorded, Reply, TestServer, paused};
use concord_sdk_discord_rest::concord::ErrorKind;
use concord_sdk_discord_rest::concord::reqwest;
use concord_sdk_discord_rest::pagination::{Outcome, PagesError, Resume};
use concord_sdk_discord_rest::{
    ChannelId, CreateMessage, DEFAULT_USER_AGENT, Discord, DiscordConfig, Error, ErrorClass,
    GateScope, GuildId, JsonErrorCode, MessageId, MessageLimit, Nonce, ProtocolProblem,
    RateLimitScope, RateLimiter, Resource,
};
use serde_json::{Value, json};

const TOKEN: &str = "fixture-bot-token.DO-NOT-LOG";
const GUILD: u64 = 813_847_529_340_108_800;
const SALES: u64 = 1_050_012_345_678_901_234;
const SUPPORT: u64 = 1_050_012_345_678_901_235;
const NEWEST: u64 = 1_434_270_000_000_000_003;
const MIDDLE: u64 = NEWEST - 1;
const OLDEST: u64 = NEWEST - 2;

fn client(server: &TestServer) -> Discord {
    client_with_token(server, TOKEN)
}

fn client_with_token(server: &TestServer, token: &str) -> Discord {
    Discord::new(
        reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap(),
        DiscordConfig {
            token: token.to_owned(),
            limiter: RateLimiter::new(),
        },
    )
    .with_base_url(format!("{}/api/v10", server.url()))
    .unwrap()
}

fn message(id: u64, channel: u64, content: &str, nonce: Option<&str>) -> Value {
    json!({
        "id": id.to_string(), "channel_id": channel.to_string(),
        "author": {"id":"80351110224678912", "username":"crm-bot", "bot":true},
        "content":content, "timestamp":"2026-11-07T10:20:30.000000+00:00",
        "edited_timestamp":null, "tts":false, "mention_everyone":false,
        "mentions":[], "mention_roles":[], "attachments":[], "embeds":[],
        "pinned":false, "type":0, "flags":0, "nonce":nonce
    })
}

fn reply(status: u16, body: Value) -> Reply {
    Reply::json(status, serde_json::to_vec(&body).unwrap())
}

fn assert_common_wire(request: &Recorded) {
    assert_eq!(
        request.header("authorization"),
        Some(format!("Bot {TOKEN}").as_str())
    );
    assert_eq!(request.header("user-agent"), Some(DEFAULT_USER_AGENT));
    assert_eq!(request.header("accept"), Some("application/json"));
    assert!(!request.target.contains(TOKEN));
    if request.method == "GET" {
        assert!(request.body.is_empty());
    }
}

#[tokio::test]
async fn browse_guild_history_and_fetch_message() {
    let server = TestServer::start(|request: &Recorded| -> Reply {
        assert_eq!(request.method, "GET");
        if request.path() == format!("/api/v10/guilds/{GUILD}/channels") {
            assert!(request.query().is_none());
            return reply(200, json!([
                {"id":"1050012345678901233", "type":2, "name":"voice"},
                {"id":SALES.to_string(), "type":0, "guild_id":GUILD.to_string(), "name":"sales"},
                {"id":SUPPORT.to_string(), "type":0, "name":"support"}
            ]));
        }
        if request.path() == format!("/api/v10/channels/{SALES}/messages") {
            assert_eq!(request.query_all("limit"), ["2"]);
            assert!(request.query_all("around").is_empty());
            assert!(request.query_all("after").is_empty());
            return if request.query_all("before").is_empty() {
                assert_eq!(request.query_pairs().len(), 1);
                reply(200, json!([
                    message(NEWEST, SALES, "CRM case 427 opened", None),
                    message(MIDDLE, SALES, "Owner assigned", None)
                ]))
            } else {
                assert_eq!(request.query_pairs().len(), 2);
                assert_eq!(request.query_all("before"), [MIDDLE.to_string()]);
                reply(200, json!([message(OLDEST, SALES, "Earlier contact", None)]))
            };
        }
        assert_eq!(request.path(), format!("/api/v10/channels/{SALES}/messages/{NEWEST}"));
        assert!(request.query().is_none());
        reply(200, message(NEWEST, SALES, "CRM case 427 opened", None))
    }).await;
    let discord = client(&server);
    assert!(!format!("{discord:?}").contains(TOKEN));

    let channels = discord
        .guild(GuildId::new(GUILD))
        .channels()
        .send()
        .await
        .unwrap();
    let selected = channels
        .iter()
        .find(|channel| channel.kind.is_guild_text())
        .unwrap();
    assert_eq!(selected.id, ChannelId::new(SALES));
    assert_eq!(selected.name.as_deref(), Some("sales"));
    let channel = discord.channel(selected.id);
    let history = channel
        .history(MessageLimit::new(2).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(
        history.iter().map(|item| item.id.get()).collect::<Vec<_>>(),
        [NEWEST, MIDDLE, OLDEST]
    );
    let fetched = channel.message(history[0].id).send().await.unwrap();
    assert_eq!(fetched.id, history[0].id);
    assert_eq!(fetched.channel_id, selected.id);
    assert_eq!(fetched.content, "CRM case 427 opened");
    assert_eq!(fetched.author.username, "crm-bot");
    let requests = server.requests();
    assert_eq!(
        requests.len(),
        4,
        "short final page completes without another request"
    );
    for request in &requests {
        assert_common_wire(request);
    }

    let incomplete = channel
        .history(MessageLimit::new(2).unwrap())
        .max_pages(NonZeroUsize::new(1).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(
        incomplete
            .collected
            .iter()
            .map(|m| m.id.get())
            .collect::<Vec<_>>(),
        [NEWEST, MIDDLE]
    );
    assert!(matches!(incomplete.error, PagesError::Bounded { .. }));
    assert_eq!(incomplete.error.outcome(), Outcome::Bounded);
    assert_eq!(incomplete.error.progress().admitted, 1);
    assert_eq!(incomplete.error.progress().succeeded, 1);
    assert_eq!(incomplete.error.progress().failed, 0);
    let resume = incomplete.error.resume().unwrap().clone();
    assert_eq!(resume, Resume::At(MessageId::new(MIDDLE)));
    let mut resumed = channel.history_from(resume, MessageLimit::new(2).unwrap());
    let last = resumed.next_page().await.unwrap().unwrap();
    assert_eq!(last.items[0].id.get(), OLDEST);
    assert!(last.next.is_none());
    assert!(resumed.next_page().await.unwrap().is_none());
    assert!(resumed.next_page().await.unwrap().is_none());
    let report = resumed.finish();
    assert_eq!(report.outcome, Outcome::Exhausted);
    assert_eq!(report.progress.admitted, 1);
    assert!(report.resume.is_none());
    assert!(report.failed.is_none());
    assert_eq!(
        server.requests().len(),
        6,
        "the cap and completed traversal do no extra I/O"
    );
    for request in server.requests() {
        assert_common_wire(&request);
    }
    assert_eq!(server.shutdown().await.open_after, 0);

    shared_bucket_partial_history_and_global_gate().await;
    read_errors_and_protocol_failures().await;
}

#[tokio::test]
async fn create_message_read_back_and_reconcile_ambiguous_write() {
    let stored = Arc::new(Mutex::new(None::<Value>));
    let fixture = stored.clone();
    let server = TestServer::start(move |request: &Recorded| -> Reply {
        assert!(request.query().is_none());
        if request.method == "POST" {
            assert_eq!(
                request.path(),
                format!("/api/v10/channels/{SALES}/messages")
            );
            assert_eq!(request.header("content-type"), Some("application/json"));
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            assert_eq!(body["content"], "CRM case 427 resolved");
            assert_eq!(body["nonce"], "crm-case-427-v1");
            assert_eq!(body["enforce_nonce"], true);
            let created = message(
                NEWEST,
                SALES,
                body["content"].as_str().unwrap(),
                body["nonce"].as_str(),
            );
            *fixture.lock().unwrap() = Some(created.clone());
            return reply(200, created);
        }
        assert_eq!(request.method, "GET");
        assert_eq!(
            request.path(),
            format!("/api/v10/channels/{SALES}/messages/{NEWEST}")
        );
        reply(200, fixture.lock().unwrap().clone().unwrap())
    })
    .await;
    let discord = client(&server);
    let channel = discord.channel(ChannelId::new(SALES));
    let body = CreateMessage::text("CRM case 427 resolved")
        .unwrap()
        .with_nonce(Nonce::new("crm-case-427-v1").unwrap());
    assert!(!format!("{body:?}").contains("CRM case 427 resolved"));
    let created = channel.create_message(body).send().await.unwrap();
    assert_eq!(created.id, MessageId::new(NEWEST));
    assert_eq!(created.nonce.as_ref().unwrap().as_str(), "crm-case-427-v1");
    let fetched = channel.message(created.id).send().await.unwrap();
    assert_eq!(fetched.id, created.id);
    assert_eq!(fetched.channel_id, ChannelId::new(SALES));
    assert_eq!(fetched.content, created.content);
    assert_eq!(fetched.author.id, created.author.id);
    assert_eq!(fetched.nonce, created.nonce);
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    for request in &requests {
        assert_common_wire(request);
    }
    assert_eq!(server.shutdown().await.open_after, 0);

    reconcile_timed_out_creation().await;
    unreadable_creation_and_local_validation().await;
}

fn assert_safe(error: &Error) {
    let output = format!("{error}; {error:?}");
    for private in [
        TOKEN,
        "provider-private-detail",
        "http://",
        "CRM case 427 resolved",
    ] {
        assert!(
            !output.contains(private),
            "sensitive detail appeared in an error"
        );
    }
    assert!(error.operation().is_some());
}

async fn shared_bucket_partial_history_and_global_gate() {
    let server = TestServer::start(|request: &Recorded| -> Reply {
        assert_common_wire(request);
        let path = request.path();
        if path == format!("/api/v10/channels/{SALES}/messages") {
            assert_eq!(request.query_all("limit"), ["2"]);
            if request.query_all("before").is_empty() {
                return reply(
                    200,
                    json!([
                        message(NEWEST, SALES, "Lead opened", None),
                        message(MIDDLE, SALES, "Owner assigned", None)
                    ]),
                )
                .header("X-RateLimit-Bucket", "same-hash")
                .header("X-RateLimit-Remaining", "1")
                .header("X-RateLimit-Reset-After", "60.25");
            }
            assert_eq!(request.query_all("before"), [MIDDLE.to_string()]);
            return reply(
                429,
                json!({"message":"provider-private-detail", "retry_after":60.25,
                "global":false, "code":20028}),
            )
            .header("Retry-After", "0.125")
            .header("X-RateLimit-Scope", "shared")
            .header("X-RateLimit-Bucket", "same-hash")
            .header("X-RateLimit-Remaining", "0")
            .header("X-RateLimit-Reset-After", "30");
        }
        if path == format!("/api/v10/channels/{SUPPORT}/messages") {
            assert!(request.query().is_none());
            return reply(200, json!([]))
                .header("X-RateLimit-Bucket", "same-hash")
                .header("X-RateLimit-Remaining", "1")
                .header("X-RateLimit-Reset-After", "60.25");
        }
        assert_eq!(
            path,
            format!("/api/v10/channels/{SUPPORT}/messages/{NEWEST}")
        );
        reply(
            429,
            json!({"message":"provider-private-detail", "retry_after":0.125, "global":true}),
        )
        .header("Retry-After", "60.5")
        .header("X-RateLimit-Global", "true")
        .header("X-RateLimit-Scope", "global")
    })
    .await;
    let discord = client(&server);
    let a = discord.channel(ChannelId::new(SALES));
    let b = discord.channel(ChannelId::new(SUPPORT));
    let incomplete = a
        .history(MessageLimit::new(2).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(
        incomplete
            .collected
            .iter()
            .map(|m| m.id.get())
            .collect::<Vec<_>>(),
        [NEWEST, MIDDLE]
    );
    assert_eq!(incomplete.error.outcome(), Outcome::Failed);
    assert_eq!(
        incomplete.error.resume(),
        Some(&Resume::At(MessageId::new(MIDDLE)))
    );
    assert_eq!(incomplete.error.progress().admitted, 2);
    assert_eq!(incomplete.error.progress().succeeded, 1);
    assert_eq!(incomplete.error.progress().failed, 1);
    let error = incomplete.error.provider_error().unwrap();
    assert_safe(error);
    assert_eq!(
        error.resource(),
        Some(Resource::Channel(ChannelId::new(SALES)))
    );
    assert_eq!(error.status().unwrap().as_u16(), 429);
    assert_eq!(
        error.operation().unwrap().name(),
        "Discord.channel.messages"
    );
    let Error::RateLimited(rate) = error else {
        panic!("expected provider 429");
    };
    assert_eq!(rate.scope, RateLimitScope::Shared);
    assert_eq!(rate.retry_after, Some(Duration::from_millis(60_250)));
    assert_eq!(rate.bucket.as_ref().unwrap().as_str(), "same-hash");
    assert_eq!(rate.meta.status().as_u16(), 429);
    assert_eq!(rate.meta.headers()["retry-after"], "0.125");
    assert_eq!(
        rate.meta.operation().unwrap().name(),
        "Discord.channel.messages"
    );
    assert!(rate.body.is_ok());

    assert!(
        b.messages().send().await.unwrap().is_empty(),
        "A's shared hash must not gate B"
    );
    let gated = a.messages().send().await.unwrap_err();
    assert_safe(&gated);
    assert_eq!(
        gated.resource(),
        Some(Resource::Channel(ChannelId::new(SALES)))
    );
    let Error::Gated(gate) = gated else {
        panic!("B's learned window must not unblock A");
    };
    let GateScope::Bucket {
        bucket: Some(bucket),
    } = &gate.scope
    else {
        panic!("expected bucket gate");
    };
    assert_eq!(bucket.as_str(), "same-hash");
    assert!(gate.retry_after > Duration::ZERO);
    assert!(gate.retry_after <= Duration::from_millis(60_250));
    assert_eq!(server.requests().len(), 3);

    let global = b.message(MessageId::new(NEWEST)).send().await.unwrap_err();
    assert_safe(&global);
    assert_eq!(
        global.resource(),
        Some(Resource::Channel(ChannelId::new(SUPPORT)))
    );
    let Error::RateLimited(rate) = global else {
        panic!("expected global 429");
    };
    assert_eq!(rate.scope, RateLimitScope::Global);
    assert_eq!(rate.retry_after, Some(Duration::from_millis(60_500)));
    assert_eq!(rate.meta.headers()["x-ratelimit-global"], "true");
    assert!(rate.bucket.is_none());
    let clone = discord.clone();
    let globally_gated = [
        clone
            .guild(GuildId::new(GUILD))
            .channels()
            .send()
            .await
            .unwrap_err(),
        a.message(MessageId::new(OLDEST)).send().await.unwrap_err(),
        b.create_message(CreateMessage::text("CRM case 427 resolved").unwrap())
            .send()
            .await
            .unwrap_err(),
    ];
    for (error, resource) in globally_gated.into_iter().zip([
        Resource::Guild(GuildId::new(GUILD)),
        Resource::Channel(ChannelId::new(SALES)),
        Resource::Channel(ChannelId::new(SUPPORT)),
    ]) {
        assert_safe(&error);
        assert_eq!(error.resource(), Some(resource));
        assert!(matches!(error, Error::Gated(ref gate) if gate.scope == GateScope::Global));
    }
    assert_eq!(
        server.requests().len(),
        4,
        "global gate does no I/O, including for a POST"
    );
    assert_eq!(server.shutdown().await.open_after, 0);
}

async fn read_errors_and_protocol_failures() {
    for (status, code, class) in [
        (400, 50035, ErrorClass::BadRequest),
        (401, 50014, ErrorClass::Unauthorized),
        (403, 50013, ErrorClass::Forbidden),
        (404, 10008, ErrorClass::NotFound),
        (503, 0, ErrorClass::Server),
    ] {
        let server = TestServer::start(move |_: &Recorded| -> Reply {
            reply(
                status,
                json!({"code":code, "message":"provider-private-detail"}),
            )
            .header("X-Request-Id", "fixture-status")
        })
        .await;
        let error = client(&server)
            .channel(ChannelId::new(SALES))
            .message(MessageId::new(NEWEST))
            .send()
            .await
            .unwrap_err();
        assert_safe(&error);
        assert_eq!(
            error.resource(),
            Some(Resource::Channel(ChannelId::new(SALES)))
        );
        let Error::Api(api) = error else {
            panic!("expected API refusal");
        };
        assert_eq!(api.class(), class);
        assert_eq!(api.status().as_u16(), status);
        assert_eq!(api.code(), Some(JsonErrorCode(code)));
        assert_eq!(api.meta.headers()["x-request-id"], "fixture-status");
        assert!(api.document.is_ok());
        assert_common_wire(&server.single());
        assert_eq!(server.shutdown().await.open_after, 0);
    }
    for (status, media, body, kind) in [
        (200, "application/json", "{broken", ErrorKind::Decode),
        (
            200,
            "text/html",
            "provider-private-detail",
            ErrorKind::Metadata,
        ),
        (
            403,
            "text/html",
            "provider-private-detail",
            ErrorKind::Metadata,
        ),
    ] {
        let server = TestServer::start(move |_: &Recorded| -> Reply {
            Reply::text(status, media, body).header("X-Request-Id", "fixture-malformed")
        })
        .await;
        let error = client(&server)
            .channel(ChannelId::new(SALES))
            .message(MessageId::new(NEWEST))
            .send()
            .await
            .unwrap_err();
        assert_safe(&error);
        assert_eq!(error.status().unwrap().as_u16(), status);
        match error {
            Error::Concord { error, .. } => {
                assert_eq!(error.kind(), kind);
                assert_eq!(error.head().unwrap().status().as_u16(), status);
                assert_eq!(
                    error.head().unwrap().headers()["x-request-id"],
                    "fixture-malformed"
                );
            }
            Error::Api(api) => {
                assert_eq!(api.class(), ErrorClass::Forbidden);
                assert!(api.code().is_none());
                assert_eq!(api.document.unwrap_err().kind(), kind);
            }
            _ => panic!("unexpected error category"),
        }
        assert_common_wire(&server.single());
        assert_eq!(server.shutdown().await.open_after, 0);
    }
    for (ids, problem) in [
        (vec![OLDEST, NEWEST], ProtocolProblem::UnorderedPage),
        (vec![NEWEST, MIDDLE, OLDEST], ProtocolProblem::OversizedPage),
    ] {
        let server = TestServer::start(move |request: &Recorded| -> Reply {
            assert_common_wire(request);
            reply(
                200,
                Value::Array(
                    ids.iter()
                        .map(|&id| message(id, SALES, "history", None))
                        .collect(),
                ),
            )
        })
        .await;
        let incomplete = client(&server)
            .channel(ChannelId::new(SALES))
            .history(MessageLimit::new(2).unwrap())
            .items()
            .collect_complete()
            .await
            .unwrap_err();
        assert!(
            incomplete.collected.is_empty(),
            "an invalid page contributes no items"
        );
        assert_eq!(incomplete.error.resume(), Some(&Resume::Start));
        let error = incomplete.error.provider_error().unwrap();
        assert_safe(error);
        assert!(matches!(error, Error::Protocol(error) if error.problem == problem));
        assert_eq!(
            server.requests().len(),
            1,
            "protocol failure is never retried"
        );
        assert_eq!(server.shutdown().await.open_after, 0);
    }
    let server = TestServer::start(|_: &Recorded| -> Reply { reply(200, json!([])) }).await;
    let history = client(&server)
        .channel(ChannelId::new(SALES))
        .history(MessageLimit::MAX)
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert!(
        history.is_empty(),
        "empty channel or missing READ_MESSAGE_HISTORY is successful exhaustion"
    );
    assert_eq!(server.requests().len(), 1);
    assert_eq!(server.shutdown().await.open_after, 0);
}

async fn reconcile_timed_out_creation() {
    let stored = Arc::new(Mutex::new(None::<Value>));
    let fixture = stored.clone();
    let server = TestServer::start(move |request: &Recorded| -> Reply {
        assert_common_wire(request);
        if request.method == "POST" {
            assert_eq!(
                request.path(),
                format!("/api/v10/channels/{SALES}/messages")
            );
            assert!(request.query().is_none());
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            assert_eq!(body["nonce"], "crm-case-427-timeout");
            assert_eq!(body["enforce_nonce"], true);
            let created = message(
                NEWEST,
                SALES,
                body["content"].as_str().unwrap(),
                body["nonce"].as_str(),
            );
            assert!(
                fixture.lock().unwrap().replace(created.clone()).is_none(),
                "a create must never be resent"
            );
            return reply(200, created).delay(Duration::from_secs(5));
        }
        let created = fixture.lock().unwrap().clone().unwrap();
        if request.path() == format!("/api/v10/channels/{SALES}/messages") {
            assert_eq!(request.query_all("limit"), ["10"]);
            return reply(200, json!([created]));
        }
        assert_eq!(
            request.path(),
            format!("/api/v10/channels/{SALES}/messages/{NEWEST}")
        );
        reply(200, created)
    })
    .await;
    let discord = client(&server);
    let channel = discord.channel(ChannelId::new(SALES));
    // Paused clock: the client's 2 s timeout fires only once the fixture holds the create; the
    // reconciling reads complete while virtual time stands still.
    tokio::time::pause();
    let create = channel
        .create_message(
            CreateMessage::text("CRM case 427 resolved")
                .unwrap()
                .with_nonce(Nonce::new("crm-case-427-timeout").unwrap()),
        )
        .send();
    let late = |r: &Recorded| r.method == "POST";
    let error = paused::time_out(&server, late, Duration::from_secs(2), create)
        .await
        .unwrap_err();
    assert_safe(&error);
    assert!(error.is_ambiguous_write());
    assert_eq!(
        error.resource(),
        Some(Resource::Channel(ChannelId::new(SALES)))
    );
    assert_eq!(
        error.operation().unwrap().name(),
        "Discord.channel.create_message"
    );
    let Error::AmbiguousWrite(write) = error else {
        panic!("timeout after acceptance is ambiguous");
    };
    assert!(write.accepted.is_none());
    assert_eq!(write.error.kind(), ErrorKind::Transport);
    assert!(write.error.transport().unwrap().is_timeout());
    assert!(
        stored.lock().unwrap().is_some(),
        "fixture accepted the message before timing out"
    );
    assert_eq!(server.requests().len(), 1);

    let recent = paused::drive(
        channel
            .messages()
            .set_limit(MessageLimit::new(10).unwrap())
            .send(),
    )
    .await
    .unwrap();
    let found = recent
        .iter()
        .find(|m| m.nonce.as_ref().map(Nonce::as_str) == Some("crm-case-427-timeout"))
        .unwrap();
    let fetched = paused::drive(channel.message(found.id).send())
        .await
        .unwrap();
    tokio::time::resume();
    assert_eq!(fetched.id, MessageId::new(NEWEST));
    assert_eq!(fetched.content, "CRM case 427 resolved");
    assert_eq!(fetched.channel_id, ChannelId::new(SALES));
    assert_eq!(
        server
            .requests()
            .iter()
            .filter(|r| r.method == "POST")
            .count(),
        1
    );
    assert_eq!(server.requests().len(), 3);
    assert_eq!(server.shutdown().await.open_after, 0);
}

async fn unreadable_creation_and_local_validation() {
    let server = TestServer::start(|request: &Recorded| -> Reply {
        assert_common_wire(request);
        assert_eq!(request.method, "POST");
        Reply::json(200, b"{broken").header("X-Request-Id", "created-id-unknown")
    })
    .await;
    let discord = client(&server);
    let error = discord
        .channel(ChannelId::new(SALES))
        .create_message(CreateMessage::text("CRM case 427 resolved").unwrap())
        .send()
        .await
        .unwrap_err();
    assert_safe(&error);
    let Error::AmbiguousWrite(write) = error else {
        panic!("a malformed successful POST is ambiguous");
    };
    assert_eq!(write.accepted.unwrap().as_u16(), 200);
    assert_eq!(write.error.kind(), ErrorKind::Decode);
    assert_eq!(
        write.error.head().unwrap().headers()["x-request-id"],
        "created-id-unknown"
    );
    assert_eq!(server.requests().len(), 1);

    assert!(CreateMessage::text("").is_err());
    assert!(CreateMessage::text("x".repeat(2001)).is_err());
    assert!(Nonce::new("").is_err());
    assert!(Nonce::new("x".repeat(26)).is_err());
    assert!(MessageLimit::new(0).is_err());
    assert!(MessageLimit::new(101).is_err());
    let bad_token = "fixture-private-token\r\ninvalid";
    let error = client_with_token(&server, bad_token)
        .channel(ChannelId::new(SALES))
        .messages()
        .send()
        .await
        .unwrap_err();
    assert_safe(&error);
    assert!(!format!("{error:?}; {error}").contains("fixture-private-token"));
    assert!(matches!(error, Error::Concord { error, .. } if error.kind() == ErrorKind::Build));
    assert_eq!(
        server.requests().len(),
        1,
        "invalid values never reach the wire"
    );
    assert_eq!(server.shutdown().await.open_after, 0);
}

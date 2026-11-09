//! Wire-level checks against local fixtures: request shapes, every documented error class,
//! rate-limit parsing, malformed / truncated / oversized / wrong-media bodies, redaction.
//! No Discord request is made and no real credential is used.

mod support;

use std::time::Duration;

use concord::ErrorKind;
use concord::reqwest::StatusCode;
use concord_example_testserver::{Body, Reply, TestServer};
use concord_sdk_discord_rest::{
    Anchor, ChannelId, ChannelType, CreateMessage, Discord, DiscordConfig, Error, ErrorClass,
    GateScope, JsonErrorCode, MessageId, MessageLimit, Nonce, RateLimitScope, RateLimiter,
    Resource, SUCCESS_BODY_LIMIT, concord,
};
use support::{
    CATEGORY, EXAMPLE_CHANNEL, EXAMPLE_MESSAGE, GENERAL, GUILD, ManualClock, TOKEN, assert_common,
    assert_get, discord, discord_with, documented_429, fixture, http, message_json,
};

#[tokio::test]
async fn native_send_keeps_the_declared_success_body_limit() {
    let server = TestServer::start(|_| Reply::json(200, vec![b' '; SUCCESS_BODY_LIMIT + 1])).await;
    let error = discord(&server, RateLimiter::new())
        .channel(GENERAL)
        .messages()
        .prepare()
        .unwrap()
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Limit);
    assert_eq!(error.head().unwrap().status(), StatusCode::OK);
    server.shutdown().await;
}

#[tokio::test]
async fn guild_channels_request_and_documented_channel_shapes() {
    let server = TestServer::start(|_| Reply::json(200, fixture("guild-channels"))).await;
    let channels = discord(&server, RateLimiter::new())
        .guild(GUILD)
        .channels()
        .send()
        .await
        .unwrap();
    assert_get(
        &server.single(),
        "/api/v10/guilds/290926798629997250/channels",
    );
    assert_eq!(channels.len(), 3);
    let category = &channels[0];
    assert_eq!(category.id, CATEGORY);
    assert_eq!(category.kind, ChannelType::GUILD_CATEGORY);
    assert_eq!(category.parent_id, None, "null parent");
    assert_eq!(channels[1].kind, ChannelType::GUILD_VOICE);
    assert!(!channels[1].kind.is_guild_text());
    let general = &channels[2];
    assert_eq!(general.id, GENERAL);
    assert_eq!(general.guild_id, Some(GUILD));
    assert_eq!(general.name.as_deref(), Some("general"));
    assert_eq!(general.parent_id, Some(CATEGORY));
    assert_eq!(general.rate_limit_per_user, Some(2));
    assert_eq!(
        general.last_message_id,
        Some(MessageId::new(155117677105512449))
    );
    assert!(general.kind.is_guild_text());
    server.shutdown().await;
}

#[tokio::test]
async fn message_listing_query_shapes_and_single_anchor() {
    let server = TestServer::start(|_| Reply::json(200, "[]")).await;
    let channel = discord(&server, RateLimiter::new()).channel(GENERAL);
    let id = MessageId::new(155117677105512449);
    let limit = MessageLimit::new(2).unwrap();
    channel.messages().send().await.unwrap();
    channel
        .messages()
        .anchor(Anchor::Before(id))
        .set_limit(limit)
        .send()
        .await
        .unwrap();
    // A later anchor replaces the earlier one: Discord accepts only one of the three.
    channel
        .messages()
        .anchor(Anchor::Before(id))
        .anchor(Anchor::After(id))
        .send()
        .await
        .unwrap();
    channel
        .messages()
        .anchor(Anchor::Around(id))
        .set_limit(MessageLimit::MAX)
        .send()
        .await
        .unwrap();
    let requests = server.requests();
    let base = "/api/v10/channels/41771983423143937/messages";
    assert_get(&requests[0], base);
    assert_get(
        &requests[1],
        &format!("{base}?before=155117677105512449&limit=2"),
    );
    assert_get(&requests[2], &format!("{base}?after=155117677105512449"));
    assert_get(
        &requests[3],
        &format!("{base}?around=155117677105512449&limit=100"),
    );
    server.shutdown().await;
}

#[tokio::test]
async fn single_message_decodes_the_documented_example() {
    let server = TestServer::start(|_| Reply::json(200, fixture("message"))).await;
    let message = discord(&server, RateLimiter::new())
        .channel(EXAMPLE_CHANNEL)
        .message(EXAMPLE_MESSAGE)
        .send()
        .await
        .unwrap();
    assert_get(
        &server.single(),
        "/api/v10/channels/290926798999357250/messages/334385199974967042",
    );
    assert_eq!(message.id, EXAMPLE_MESSAGE);
    assert_eq!(message.channel_id, EXAMPLE_CHANNEL);
    assert_eq!(message.content, "Supa Hot");
    assert_eq!(message.author.username, "Mason");
    assert_eq!(message.author.id.get(), 53908099506183680);
    assert_eq!(message.timestamp, "2017-07-11T17:27:07.299000+00:00");
    assert_eq!(message.edited_timestamp, None);
    assert_eq!(message.kind, 0);
    assert!(!message.pinned && !message.tts && !message.mention_everyone);
    assert_eq!(message.nonce, None);
    server.shutdown().await;
}

#[tokio::test]
async fn create_message_sends_exact_json_body() {
    let server = TestServer::start(|r| {
        let nonce = serde_json::from_slice::<serde_json::Value>(&r.body).unwrap()["nonce"].clone();
        Reply::json(
            200,
            message_json(1, GENERAL, "hello <@1> & \"you\"", nonce.as_str()),
        )
    })
    .await;
    let channel = discord(&server, RateLimiter::new()).channel(GENERAL);
    let body = CreateMessage::text("hello <@1> & \"you\"")
        .unwrap()
        .with_nonce(Nonce::new("job-7-1").unwrap());
    let created = channel.create_message(body).send().await.unwrap();
    assert_eq!(created.nonce.as_ref().map(Nonce::as_str), Some("job-7-1"));
    channel
        .create_message(CreateMessage::text("plain").unwrap().with_tts(false))
        .send()
        .await
        .unwrap();
    let requests = server.requests();
    let target = "/api/v10/channels/41771983423143937/messages";
    assert_common(&requests[0], "POST", target);
    assert_eq!(requests[0].header_all("content-type"), ["application/json"]);
    assert_eq!(
        requests[0].body_text(),
        r#"{"content":"hello <@1> & \"you\"","nonce":"job-7-1","enforce_nonce":true}"#
    );
    assert_eq!(
        requests[1].body_text(),
        r#"{"content":"plain","tts":false}"#
    );
    server.shutdown().await;
}

#[test]
fn input_rules_refuse_values_before_any_request() {
    assert_eq!(CreateMessage::text("").unwrap_err().field, "content");
    assert!(CreateMessage::text("x".repeat(2001)).is_err());
    assert!(
        CreateMessage::text("é".repeat(2000)).is_ok(),
        "characters, not bytes"
    );
    assert!(Nonce::new("").is_err());
    assert!(Nonce::new("n".repeat(26)).is_err());
    assert!(Nonce::new("n".repeat(25)).is_ok());
    assert!(MessageLimit::new(0).is_err());
    assert!(MessageLimit::new(101).is_err());
    assert_eq!(MessageLimit::new(100).unwrap(), MessageLimit::MAX);
    assert_eq!(MessageLimit::default().get(), 50);
    for refused in ["", "-1", "+1", "0x10", "1 ", "18446744073709551616"] {
        assert!(refused.parse::<ChannelId>().is_err(), "{refused:?}");
    }
    assert_eq!("41771983423143937".parse::<ChannelId>().unwrap(), GENERAL);
    let debug = format!("{:?}", CreateMessage::text("SYNTHETIC-CONTENT").unwrap());
    assert!(!debug.contains("SYNTHETIC-CONTENT"), "{debug}");
}

#[tokio::test]
async fn snowflakes_must_be_strings_and_nonces_may_be_integers() {
    let server = TestServer::start(|r| {
        if r.path().ends_with("/1") {
            Reply::json(200, fixture("message-numeric-id"))
        } else {
            let mut message: serde_json::Value =
                serde_json::from_str(&message_json(2, GENERAL, "n", None)).unwrap();
            message["nonce"] = 1234567890123u64.into();
            Reply::json(200, message.to_string())
        }
    })
    .await;
    let channel = discord(&server, RateLimiter::new()).channel(GENERAL);
    let error = channel.message(MessageId::new(1)).send().await.unwrap_err();
    let Error::Concord { error, resource } = &error else {
        panic!("{error:?}")
    };
    assert_eq!(error.kind(), ErrorKind::Decode);
    assert_eq!(error.head().unwrap().status(), StatusCode::OK);
    assert_eq!(*resource, Some(Resource::Channel(GENERAL)));
    let message = channel.message(MessageId::new(2)).send().await.unwrap();
    assert_eq!(message.nonce.unwrap().as_str(), "1234567890123");
    server.shutdown().await;
}

/// Every documented HTTP error class, with Discord's JSON error codes.
#[tokio::test]
async fn documented_error_classes_keep_status_code_and_resource() {
    let cases: [(u16, &str, ErrorClass, Option<JsonErrorCode>); 10] = [
        (
            400,
            "error-invalid-form-body",
            ErrorClass::BadRequest,
            Some(JsonErrorCode::INVALID_FORM_BODY),
        ),
        (
            400,
            "error-empty-message",
            ErrorClass::BadRequest,
            Some(JsonErrorCode::EMPTY_MESSAGE),
        ),
        (
            401,
            "error-invalid-token",
            ErrorClass::Unauthorized,
            Some(JsonErrorCode::INVALID_TOKEN),
        ),
        (
            403,
            "error-missing-access",
            ErrorClass::Forbidden,
            Some(JsonErrorCode::MISSING_ACCESS),
        ),
        (
            403,
            "error-missing-permissions",
            ErrorClass::Forbidden,
            Some(JsonErrorCode::MISSING_PERMISSIONS),
        ),
        (
            404,
            "error-unknown-channel",
            ErrorClass::NotFound,
            Some(JsonErrorCode::UNKNOWN_CHANNEL),
        ),
        (
            404,
            "error-unknown-message",
            ErrorClass::NotFound,
            Some(JsonErrorCode::UNKNOWN_MESSAGE),
        ),
        (
            405,
            "error-method-not-allowed",
            ErrorClass::MethodNotAllowed,
            Some(JsonErrorCode::GENERAL),
        ),
        (
            500,
            "error-unknown-message",
            ErrorClass::Server,
            Some(JsonErrorCode::UNKNOWN_MESSAGE),
        ),
        (502, "bad-gateway", ErrorClass::Server, None),
    ];
    for (status, name, class, code) in cases {
        let server = TestServer::start(move |_| {
            if name.ends_with("gateway") {
                Reply::text(status, "text/html", fixture(name))
            } else {
                Reply::json(status, fixture(name))
            }
        })
        .await;
        let error = discord(&server, RateLimiter::new())
            .channel(GENERAL)
            .message(MessageId::new(7))
            .send()
            .await
            .unwrap_err();
        assert_eq!(error.status().map(|s| s.as_u16()), Some(status), "{name}");
        assert_eq!(error.resource(), Some(Resource::Channel(GENERAL)));
        assert_eq!(
            error.operation().unwrap().to_string(),
            "GET Discord.channel.message"
        );
        let Error::Api(api) = &error else {
            panic!("{name}: {error:?}")
        };
        assert_eq!(api.class(), class, "{name}");
        assert_eq!(api.code(), code, "{name}");
        if name == "error-invalid-form-body" {
            let errors = api.document.as_ref().unwrap().errors.as_ref().unwrap();
            assert_eq!(
                errors["access_token"]["_errors"][0]["code"],
                "BASE_TYPE_REQUIRED"
            );
        }
        if name == "bad-gateway" {
            // The HTML page is not a document, and the 502 stays the failure.
            let document = api.document.as_ref().unwrap_err();
            assert_eq!(document.kind(), ErrorKind::Metadata);
        }
        server.shutdown().await;
    }
}

#[tokio::test]
async fn error_documents_that_cannot_be_read_keep_the_status() {
    let oversized = format!(
        r#"{{"code":10003,"message":"{}"}}"#,
        "x".repeat(concord_sdk_discord_rest::ERROR_BODY_LIMIT)
    );
    let server = TestServer::start(move |r| match r.path().rsplit('/').next().unwrap() {
        "1" => Reply::json(404, oversized.clone()),
        "2" => Reply::text(403, "text/plain", "forbidden"),
        "3" => Reply::json(403, fixture("error-foreign-shape")),
        "4" => Reply::json(404, "").with_body(Body::FailAfter(vec![b"{\"code\":".to_vec().into()])),
        _ => Reply::json(404, ""),
    })
    .await;
    let channel = discord(&server, RateLimiter::new()).channel(GENERAL);
    for (id, status, kind) in [
        (1, 404, ErrorKind::Limit),
        (2, 403, ErrorKind::Metadata),
        (3, 403, ErrorKind::Decode),
        (4, 404, ErrorKind::Transport),
        (5, 404, ErrorKind::Decode),
    ] {
        let error = channel
            .message(MessageId::new(id))
            .send()
            .await
            .unwrap_err();
        let Error::Api(api) = &error else {
            panic!("{id}: {error:?}")
        };
        assert_eq!(api.status().as_u16(), status, "{id}");
        assert_eq!(api.document.as_ref().unwrap_err().kind(), kind, "{id}");
        assert_eq!(api.code(), None);
    }
    server.shutdown().await;
}

#[tokio::test]
async fn malformed_truncated_oversized_and_mislabeled_successes() {
    let server = TestServer::start(|r| match r.path().rsplit('/').next().unwrap() {
        "1" => Reply::json(200, fixture("message-truncated")),
        "2" => Reply::json(200, "").with_body(Body::FailAfter(vec![
            fixture("message")[..40].to_vec().into(),
        ])),
        "3" => {
            let mut left = SUCCESS_BODY_LIMIT / (64 * 1024) + 2;
            Reply::json(200, "").with_body(Body::Generate(
                Box::new(move || {
                    left = left.checked_sub(1)?;
                    Some(vec![b' '; 64 * 1024].into())
                }),
                None,
            ))
        }
        "4" => Reply::text(200, "text/html", fixture("bad-gateway")),
        _ => Reply::new(200).body(fixture("message")),
    })
    .await;
    let channel = discord(&server, RateLimiter::new()).channel(GENERAL);
    for (id, kind) in [
        (1, ErrorKind::Decode),
        (2, ErrorKind::Transport),
        (3, ErrorKind::Limit),
        (4, ErrorKind::Metadata),
        (5, ErrorKind::Metadata), // no Content-Type at all
    ] {
        let error = channel
            .message(MessageId::new(id))
            .send()
            .await
            .unwrap_err();
        let Error::Concord {
            error: concord,
            resource,
        } = &error
        else {
            panic!("{id}: {error:?}")
        };
        assert_eq!(concord.kind(), kind, "{id}");
        assert_eq!(concord.head().unwrap().status(), StatusCode::OK, "{id}");
        assert_eq!(*resource, Some(Resource::Channel(GENERAL)));
        assert!(!error.is_ambiguous_write(), "a read is never ambiguous");
    }
    server.shutdown().await;
}

#[tokio::test]
async fn documented_429_examples_parse_scope_delay_and_bucket() {
    for (kind, scope, delay, bucket) in [
        ("user", RateLimitScope::User, 65.0, Some("abcd1234")),
        ("shared", RateLimitScope::Shared, 1337.0, Some("abcd1234")),
        ("global", RateLimitScope::Global, 65.0, None),
    ] {
        let server = TestServer::start(move |_| documented_429(kind)).await;
        let error = discord(&server, RateLimiter::new())
            .channel(GENERAL)
            .messages()
            .send()
            .await
            .unwrap_err();
        let Error::RateLimited(limited) = &error else {
            panic!("{kind}: {error:?}")
        };
        assert_eq!(limited.scope, scope, "{kind}");
        // The gate uses the larger of `Retry-After` (whole seconds) and `retry_after` (fractional).
        assert_eq!(limited.retry_after, Some(Duration::from_secs_f64(delay)));
        assert_eq!(limited.bucket.as_ref().map(|b| b.as_str()), bucket);
        assert_eq!(limited.route.resource(), Some(Resource::Channel(GENERAL)));
        let body = limited.body.as_ref().unwrap();
        assert_eq!(body.global, kind == "global");
        assert!(
            body.retry_after.fract() > 0.0,
            "the body delay is fractional"
        );
        assert_eq!(error.retry_after(), limited.retry_after);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn retry_delay_falls_back_through_header_body_and_reset_after() {
    let clock = ManualClock::new();
    let server = TestServer::start(|r| match r.path().rsplit('/').next().unwrap() {
        // A fractional header and a Cloudflare-style HTML body.
        "1" => Reply::text(429, "text/html", fixture("bad-gateway")).header("retry-after", "2.5"),
        // An invalid header; the body's fractional delay is used.
        "2" => Reply::json(429, r#"{"message":"m","retry_after":1.25,"global":false}"#)
            .header("retry-after", "soon"),
        // No delay but the route headers.
        "3" => Reply::json(429, "{}").header("x-ratelimit-reset-after", "0.75"),
        // Nothing usable at all: no delay is invented and nothing is gated.
        _ => Reply::json(429, r#"{"message":"m","retry_after":-1,"global":false}"#),
    })
    .await;
    let channel = discord(&server, clock.limiter()).channel(GENERAL);
    for (id, delay, body_ok) in [
        (1, Some(Duration::from_millis(2500)), false),
        (2, Some(Duration::from_millis(1250)), true),
        (3, Some(Duration::from_millis(750)), false),
        (4, None, true),
    ] {
        let error = channel
            .message(MessageId::new(id))
            .send()
            .await
            .unwrap_err();
        let Error::RateLimited(limited) = &error else {
            panic!("{id}: {error:?}")
        };
        assert_eq!(limited.retry_after, delay, "{id}");
        assert_eq!(limited.scope, RateLimitScope::Unspecified);
        assert_eq!(limited.body.is_ok(), body_ok, "{id}");
        // Each 429 gated only the route it came from (unknown bucket: the route itself), and
        // only for its own delay.
        clock.advance(Duration::from_secs(3));
    }
    assert_eq!(server.requests().len(), 4);
    // The last 429 had no delay: the next request is sent, not gated.
    channel.message(MessageId::new(5)).send().await.unwrap_err();
    assert_eq!(server.requests().len(), 5);
    server.shutdown().await;
}

#[tokio::test]
async fn a_429_gates_its_window_before_any_io() {
    let clock = ManualClock::new();
    let server = TestServer::start(|_| documented_429("user")).await;
    let channel = discord(&server, clock.limiter()).channel(GENERAL);
    channel.messages().send().await.unwrap_err();
    clock.advance(Duration::from_secs(60));
    let error = channel.messages().send().await.unwrap_err();
    let Error::Gated(gated) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(gated_bucket(&error).as_str(), "abcd1234");
    assert_eq!(gated.retry_after, Duration::from_secs(5));
    assert_eq!(server.requests().len(), 1, "refused locally");
    clock.advance(Duration::from_secs(5));
    channel.messages().send().await.unwrap_err();
    assert_eq!(
        server.requests().len(),
        2,
        "admitted once the delay elapsed"
    );
    server.shutdown().await;
}

fn gated_bucket(error: &Error) -> concord_sdk_discord_rest::BucketHash {
    match error {
        Error::Gated(gated) => match &gated.scope {
            GateScope::Bucket { bucket } => bucket.clone().unwrap(),
            GateScope::Global => panic!("global"),
        },
        _ => panic!("{error:?}"),
    }
}

#[tokio::test]
async fn writes_failing_after_the_head_or_without_one_are_ambiguous() {
    let server = TestServer::start(|r| match r.body_text().as_str() {
        r#"{"content":"cut"}"# => Reply::json(200, fixture("message-truncated")),
        _ => Reply::hang_up(),
    })
    .await;
    let channel = discord(&server, RateLimiter::new()).channel(GENERAL);
    let accepted = channel
        .create_message(CreateMessage::text("cut").unwrap())
        .send()
        .await
        .unwrap_err();
    let Error::AmbiguousWrite(write) = &accepted else {
        panic!("{accepted:?}")
    };
    assert_eq!(write.accepted, Some(StatusCode::OK), "created, ID unknown");
    assert_eq!(write.error.kind(), ErrorKind::Decode);
    let dropped = channel
        .create_message(CreateMessage::text("dropped").unwrap())
        .send()
        .await
        .unwrap_err();
    let Error::AmbiguousWrite(write) = &dropped else {
        panic!("{dropped:?}")
    };
    assert_eq!(write.accepted, None, "no response head");
    assert_eq!(write.error.kind(), ErrorKind::Transport);
    assert!(dropped.to_string().contains("not resent"), "{dropped}");
    assert_eq!(server.requests().len(), 2, "each write sent exactly once");
    server.shutdown().await;
}

#[tokio::test]
async fn a_refused_connection_on_a_write_is_ambiguous() {
    // Review b23-review2-sol finding 1: a performed write can be followed by a redirect whose
    // target refuses, so a connect error does not prove "not sent" with the caller's client.
    let port = concord_example_testserver::refused_addr().port();
    let discord = discord_with(
        http(),
        &format!("http://127.0.0.1:{port}"),
        RateLimiter::new(),
    );
    let error = discord
        .channel(GENERAL)
        .create_message(CreateMessage::text("never left").unwrap())
        .send()
        .await
        .unwrap_err();
    let Error::AmbiguousWrite(write) = &error else {
        panic!("{error:?}")
    };
    assert!(write.error.transport().unwrap().is_connect());
    assert_eq!(write.accepted, None);
    assert!(error.is_ambiguous_write());
}

#[tokio::test]
async fn secrets_and_provider_text_are_never_printed() {
    let server = TestServer::start(|r| match r.path().rsplit('/').next().unwrap() {
        "1" => Reply::json(403, fixture("error-marker")),
        "2" => documented_429("global"),
        _ => Reply::json(200, fixture("message")),
    })
    .await;
    let discord = discord(&server, RateLimiter::new());
    let shown = format!("{discord:?}");
    assert!(!shown.contains(TOKEN), "{shown}");
    let channel = discord.channel(GENERAL);
    let prepared = channel.message(MessageId::new(3)).prepare().unwrap();
    assert!(prepared.request().headers()["authorization"].is_sensitive());
    let shown = format!("{prepared:?}");
    assert!(!shown.contains(TOKEN), "{shown}");

    let api = channel.message(MessageId::new(1)).send().await.unwrap_err();
    let limited = channel.message(MessageId::new(2)).send().await.unwrap_err();
    let gated = channel.message(MessageId::new(3)).send().await.unwrap_err();
    assert!(matches!(gated, Error::Gated(_)));
    for error in [&api, &limited, &gated] {
        for shown in [format!("{error}"), format!("{error:?}")] {
            assert!(!shown.contains(TOKEN), "{shown}");
            assert!(!shown.contains("SYNTHETIC-PROVIDER-TEXT"), "{shown}");
            assert!(!shown.contains("rate limited."), "{shown}");
            assert!(!shown.contains("127.0.0.1"), "{shown}");
        }
    }
    assert!(
        api.to_string().contains("on channel 41771983423143937"),
        "{api}"
    );
    let Error::Api(document) = &api else { panic!() };
    assert!(
        document
            .document
            .as_ref()
            .unwrap()
            .message
            .contains("SYNTHETIC-PROVIDER-TEXT"),
        "the message is kept for deliberate reading"
    );

    // A token that cannot be a header value fails before any I/O, without being printed.
    let bad = Discord::new(
        http(),
        DiscordConfig {
            token: "synthetic\nsecret-token".into(),
            limiter: RateLimiter::new(),
        },
    )
    .with_base_url(format!("{}/api/v10", server.url()))
    .unwrap();
    let error = bad.channel(GENERAL).messages().send().await.unwrap_err();
    let Error::Concord { error: concord, .. } = &error else {
        panic!("{error:?}")
    };
    assert_eq!(concord.kind(), ErrorKind::Build);
    for shown in [format!("{error}"), format!("{error:?}")] {
        assert!(!shown.contains("secret-token"), "{shown}");
    }
    assert_eq!(
        server.requests().len(),
        2,
        "gated and refused requests never left"
    );
    server.shutdown().await;
}

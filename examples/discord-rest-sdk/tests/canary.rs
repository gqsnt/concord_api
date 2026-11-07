//! Secret inputs: DiscordConfig.token (src/api.rs:39; Authorization: Bot), and
//! CreateMessage content (src/model.rs:355; Debug promises length only), each distinct.
//! ErrorDocument message/nested errors and headers echo the token; returned Message content
//! stays ordinary because it is explicitly printable. Read messages + write create_message
//! drive seven scenarios; history also exercises PagesError. Excludes native reqwest,
//! explicit clear/unredacted accessors and returned Message/Nonce text. No wire exemptions.

use concord_check_canary::Canaries;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use std::{net::TcpListener, time::Duration};

fn http() -> concord::reqwest::Client {
    concord::reqwest::Client::builder()
        .no_proxy()
        .redirect(concord::reqwest::redirect::Policy::none())
        .retry(concord::reqwest::retry::never())
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap()
}

fn refused() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    url
}

fn reply(case: usize, media: &str, success: &str, problem: &str, echo: &str) -> Reply {
    let response = match case {
        0 | 6 => Reply::text(200, media, success),
        1 => Reply::text(400, media, problem),
        2 => Reply::text(503, media, problem),
        3 => Reply::text(200, media, format!("<{{invalid {echo}")),
        4 => Reply::text(200, "text/html", echo),
        5 => Reply::text(200, media, "").with_body(Body::FailAfter(vec![
            format!("<{{incomplete {echo}").into(),
        ])),
        _ => unreachable!(),
    };
    response
        .header("x-private", echo)
        .header("set-cookie", echo)
}

fn concord_error(c: &mut Canaries, e: &concord::Error) {
    c.assert_debug_absent("Concord error", e);
    c.assert_error_absent("Concord error/source chain", e);
    c.assert_debug_absent("error kind", &e.kind());
    c.assert_debug_absent("error head", &e.head());
    c.assert_debug_absent("error metadata", &e.head_meta());
}

async fn receive<T: std::fmt::Debug>(c: &mut Canaries, prepared: concord::Prepared<T>) {
    c.assert_debug_absent("Prepared", &prepared);
    let result = prepared.receive().await;
    c.assert_debug_absent("Received Result", &result);
    match result {
        Ok(received) => {
            c.assert_debug_absent("response head", &received.head());
            c.assert_debug_absent("response metadata", &received.meta());
            let decoded = received.decode().await;
            c.assert_debug_absent("native decoded Result", &decoded);
            if let Err(e) = decoded {
                concord_error(c, &e);
            }
        }
        Err(e) => concord_error(c, &e),
    }
}

use concord_sdk_discord_rest::{
    ChannelId, CreateMessage, Discord, DiscordConfig, Error, GuildId, MessageLimit, RateLimiter,
};

const MESSAGE: &str = r#"{"id":"10","channel_id":"1","author":{"id":"2","username":"fixture","discriminator":"0","bot":true},"content":"ordinary reply","timestamp":"2026-01-01T00:00:00Z","edited_timestamp":null,"tts":false,"mention_everyone":false,"pinned":false,"type":0}"#;

fn error(c: &mut Canaries, e: &Error) {
    c.assert_debug_absent("SDK error", e);
    c.assert_error_absent("SDK error/source chain", e);
    match e {
        Error::Api(a) => {
            c.assert_debug_absent("API failure", a);
            c.assert_debug_absent("provider document", &a.document);
            c.assert_debug_absent("provider metadata", &a.meta);
            c.assert_debug_absent("route", &a.route);
            c.assert_display_absent("route", &a.route);
            if let Err(e) = &a.document {
                concord_error(c, e);
            }
        }
        Error::AmbiguousWrite(a) => {
            c.assert_debug_absent("ambiguous write", a);
            concord_error(c, &a.error);
        }
        Error::Concord { error, .. } => concord_error(c, error),
        _ => {}
    }
}

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("discord-rest");
    let token = c.issue();
    let content = c.issue();
    let body = CreateMessage::text(&content).unwrap();
    c.assert_debug_absent("message input", &body);
    for case in 0..7 {
        let echo = token.clone();
        let server = TestServer::start(move |r: &Recorded| {
            let success = if r.method == "POST" { MESSAGE.into() } else { format!("[{MESSAGE}]") };
            let problem = format!(r#"{{"code":50035,"message":"{echo}","errors":{{"content":{{"_errors":[{{"code":"BAD","message":"{echo}"}}]}}}}}}"#);
            reply(case, "application/json", &success, &problem, &echo)
        }).await;
        let limiter = RateLimiter::new();
        c.assert_debug_absent("rate limiter", &limiter);
        let config = DiscordConfig {
            token: token.clone(),
            limiter,
        };
        c.assert_debug_absent("config", &config);
        let api = Discord::new(http(), config)
            .with_base_url(if case == 6 { refused() } else { server.url() })
            .unwrap();
        let channel = api.channel(ChannelId::new(1));
        let guild = api.guild(GuildId::new(3));
        let read = channel.messages();
        let write = channel.create_message(body.clone());
        c.assert_debug_absent("client", &api);
        c.assert_debug_absent("channel scope", &channel);
        c.assert_debug_absent("guild scope", &guild);
        c.assert_debug_absent("read endpoint", &read);
        c.assert_debug_absent("write endpoint", &write);
        receive(&mut c, read.clone().prepare().unwrap()).await;
        receive(&mut c, write.clone().prepare().unwrap()).await;
        let r = read.send().await;
        let w = write.send().await;
        c.assert_debug_absent("read Result", &r);
        c.assert_debug_absent("write Result", &w);
        assert_eq!(r.is_ok(), case == 0);
        assert_eq!(w.is_ok(), case == 0);
        if let Err(e) = &r {
            error(&mut c, e);
            if case == 1 {
                assert!(e.to_string().contains("400"));
                assert!(e.to_string().contains("Discord.channel.messages"));
                let Error::Api(a) = e else {
                    panic!("API failure expected")
                };
                assert_eq!(a.document.as_ref().unwrap().message, token);
            }
        }
        if let Err(e) = &w {
            error(&mut c, e);
        }
        let mut history = channel.history(MessageLimit::new(2).unwrap());
        c.assert_debug_absent("history", &history);
        let page = history.next_page().await;
        c.assert_debug_absent("history page Result", &page);
        if let Err(e) = page {
            c.assert_error_absent("PagesError", &e);
        }
        c.assert_debug_absent("traversal checkpoint", &history.checkpoint());
        c.assert_debug_absent("traversal progress", &history.progress());
        c.assert_debug_absent("traversal report", &history.finish());
        if case == 0 {
            c.assert_on_wire(&server.requests());
        }
        if case == 6 {
            assert!(server.requests().is_empty());
        }
        server.shutdown().await;
    }
    assert!(c.checked() >= 250);
    println!(
        "discord-rest: {} renderings; positive control passed",
        c.checked()
    );
}

//! Secrets: AccessToken (src/model.rs:78, lib.rs:169) and sensitive Authorization
//! (src/lib.rs:106); one distinct token canary travels in the bearer header, including a
//! public setter deliberately using an unflagged HeaderValue. Read poll (NDJSON) and write
//! publish cover seven scenarios; SSE also exercises payload/read errors and withheld reader
//! state. Server echoes only into withheld error text, malformed payloads and headers.
//! No wire exemptions. Excludes native reqwest, source_unredacted, raw body/header/field
//! inspection, and clear Event/MessageText/ErrorDocument values returned to callers.
use concord_check_canary::Canaries;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use std::time::Duration;

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
    concord_example_testserver::refused_url()
}

// Echo only into withheld diagnostics, never clear success-model fields.
fn reply(case: usize, status: u16, success: &str, problem: &str, echo: &str) -> Reply {
    let response = match case {
        0 | 6 => Reply::json(status, success),
        1 => Reply::json(401, problem),
        2 => Reply::json(500, problem),
        3 => Reply::json(status, format!("{{invalid {echo}")),
        4 => Reply::text(status, "text/html", echo),
        5 => Reply::json(status, "").with_body(Body::FailAfter(vec![
            format!("{{\"incomplete\":\"{echo}").into(),
        ])),
        _ => unreachable!(),
    };
    response.header("x-canary", echo).header("set-cookie", echo)
}

async fn receive<T>(c: &mut Canaries, prepared: concord::Prepared<T>) -> Result<T, concord::Error> {
    let result = prepared.receive().await;
    c.assert_debug_absent("Received Result", &result);
    match result {
        Ok(received) => {
            c.assert_debug_absent("response head", &received.head());
            c.assert_debug_absent("response metadata", &received.meta());
            received.decode().await
        }
        Err(error) => Err(error),
    }
}

fn concord_error(c: &mut Canaries, error: &concord::Error) {
    c.assert_debug_absent("concord::Error", error);
    c.assert_error_absent("concord::Error and sources", error);
    c.assert_debug_absent("error kind", &error.kind());
    if let Some(meta) = error.head_meta() {
        c.assert_debug_absent("error response metadata", &meta);
    }
}

use concord_sdk_ntfy::{AccessToken, Events, MessageText, Ntfy, NtfyError, Topic};

fn error(c: &mut Canaries, error: &NtfyError) {
    c.assert_debug_absent("SDK error", error);
    c.assert_error_absent("SDK error and sources", error);
    if let Some(e) = error.request() {
        concord_error(c, e);
    }
    if let Some(e) = error.api() {
        c.assert_debug_absent("API error", e);
        c.assert_error_absent("API error", e);
    }
}

async fn drain(c: &mut Canaries, mut events: Events, case: usize) {
    c.assert_debug_absent("open Events", &events);
    let mut failed = false;
    let mut delivered = 0;
    for _ in 0..3 {
        let Some(result) = events.next().await else {
            break;
        };
        c.assert_debug_absent("event Result", &result);
        match result {
            Ok(_) => delivered += 1,
            Err(e) => {
                failed = true;
                c.assert_debug_absent("EventsError", &e);
                c.assert_error_absent("EventsError and sources", &e);
                c.assert_debug_absent("event codec", &e.codec());
                if let Some(codec) = e.codec() {
                    c.assert_error_absent("codec Display and sources", codec);
                }
                if let Some(read) = e.read_error() {
                    c.assert_error_absent("reader Display and sources", read);
                }
                c.assert_debug_absent("event read error", &e.read_error());
                if let Some(e) = e.transport() {
                    concord_error(c, e);
                }
                assert!(e.operation().unwrap().contains("Ntfy.topic"));
            }
        }
    }
    assert_eq!(failed, case != 0);
    assert_eq!(delivered > 0, case == 0);
    c.assert_debug_absent("consumed Events", &events);
    c.assert_debug_absent("completion", &events.completion());
}

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("ntfy");
    let token = c.issue();
    let credential = AccessToken::new(&token).unwrap();
    c.assert_debug_absent("access token", &credential);
    let configured = Ntfy::new(
        http(),
        concord_sdk_ntfy::NtfyConfig {
            origin: concord::BaseUrl::from_static("https://ntfy.sh"),
        },
    )
    .set_authorization(
        concord::reqwest::header::HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
    );
    c.assert_debug_absent("client from unflagged header", &configured);
    for case in 0..7 {
        let echo = token.clone();
        let server = TestServer::start(move |r: &Recorded| {
            let event = r#"{"id":"message001","time":1,"event":"message","topic":"alerts","message":"hello"}"#;
            let problem = format!(r#"{{"code":40101,"http":401,"error":"{echo}"}}"#);
            if r.method == "GET" && !matches!(case, 1 | 2 | 4) {
                let sse = r.path().ends_with("/sse");
                let body = if case == 3 { format!(r#"{{"id":"{echo}"}}"#) } else { event.into() };
                let body = if sse { format!("data: {body}\n\n") } else { format!("{body}\n") };
                let response = Reply::text(200, if sse {"text/event-stream"} else {"application/x-ndjson"}, body);
                let response = if case == 5 {
                    response.with_body(Body::FailAfter(vec![format!("{{\"incomplete\":\"{echo}").into()]))
                } else { response };
                response.header("x-canary", &echo)
            } else {
                reply(case, 200, event, &problem, &echo)
            }
        }).await;
        let api =
            concord::BaseUrl::parse((if case == 6 { refused() } else { server.url() }).as_ref())
                .map(|origin| {
                    Ntfy::new(http(), concord_sdk_ntfy::NtfyConfig { origin })
                        .with_token(&credential)
                })
                .unwrap();
        let topic = api.topic(Topic::new("alerts").unwrap());
        let read = topic.poll();
        let write = topic.publish(MessageText::new("hello").unwrap());
        c.assert_debug_absent("client", &api);
        c.assert_debug_absent("topic scope", &topic);
        c.assert_debug_absent("read endpoint", &read);
        c.assert_debug_absent("write endpoint", &write);
        c.assert_debug_absent("read Prepared", &read.clone().prepare().unwrap());
        c.assert_debug_absent("write Prepared", &write.clone().prepare().unwrap());
        let native = receive(&mut c, write.clone().prepare().unwrap()).await;
        c.assert_debug_absent("native write Result", &native);
        assert_eq!(native.is_ok(), case == 0);
        if let Err(e) = native {
            concord_error(&mut c, &e);
        }
        let result = write.send().await;
        c.assert_debug_absent("write Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        if let Err(e) = &result {
            error(&mut c, e);
            if case == 1 {
                assert!(e.to_string().contains("401"));
                assert_eq!(e.api().unwrap().code(), Some(40101));
            }
        }
        let sse = topic.subscribe_sse();
        c.assert_debug_absent("SSE endpoint", &sse);
        c.assert_debug_absent("SSE Prepared", &sse.clone().prepare().unwrap());
        for result in [read.send().await, sse.send().await] {
            c.assert_debug_absent("read Result", &result);
            match result {
                Ok(events) => {
                    assert!(matches!(case, 0 | 3 | 5));
                    drain(&mut c, events, case).await;
                }
                Err(e) => {
                    assert!(matches!(case, 1 | 2 | 4 | 6));
                    error(&mut c, &e);
                }
            }
        }
        if case == 0 {
            let json = topic.subscribe_json();
            c.assert_debug_absent("JSON subscription endpoint", &json);
            c.assert_debug_absent(
                "JSON subscription Prepared",
                &json.clone().prepare().unwrap(),
            );
            drain(&mut c, json.send().await.unwrap(), case).await;
            c.assert_on_wire(&server.requests());
        }
        server.shutdown().await;
    }
    let e = AccessToken::new(&format!("{token}\n")).unwrap_err();
    c.assert_debug_absent("InvalidToken", &e);
    c.assert_error_absent("InvalidToken", &e);
    assert!(c.checked() >= 150);
    println!("ntfy: {} renderings; positive control passed", c.checked());
}

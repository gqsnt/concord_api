//! Secrets: AccessToken (src/model.rs:108, lib.rs:145) / sensitive Authorization
//! (src/api.rs:19). Its canary travels in the bearer header. Public timeline covers all seven
//! scenarios, including native Prepared receives. Public SSE covers all seven head/body cases,
//! echoing the token only into withheld malformed payloads, stream errors and headers.
//! There are no writes in this SDK. No wire exemptions.
//! Excludes native reqwest, source_unredacted, raw field/header access, and clear returned
//! Status/Account/InstanceInfo/ErrorDocument models. Private Api/config cannot be reached.
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

use concord_sdk_mastodon::{
    AccessToken, Error, InstanceUrl, Mastodon, StreamErrorKind, StreamingHost,
};

fn error(c: &mut Canaries, error: &Error) {
    c.assert_debug_absent("SDK error", error);
    c.assert_error_absent("SDK error and sources", error);
    if let Some(e) = error.request() {
        concord_error(c, e);
    }
    if let Some(e) = error.api() {
        c.assert_debug_absent("API error", e);
        c.assert_error_absent("API error and sources", e);
    }
}

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("mastodon");
    let token = c.issue();
    let credential = AccessToken::new(&token).unwrap();
    c.assert_debug_absent("access token", &credential);
    for case in 0..7 {
        let echo = token.clone();
        let server = TestServer::start(move |r: &Recorded| {
            let problem = format!(r#"{{"error":"{echo}"}}"#);
            if r.path().contains("/streaming/") && !matches!(case, 1 | 2 | 4) {
                let body = if case == 3 {
                    format!("event: update\ndata: {{\"id\":\"{echo}\"}}\n\n")
                } else {
                    "event: delete\ndata: \"one\"\n\n".into()
                };
                let response = Reply::text(200, "text/event-stream", &body);
                let response = if case == 5 {
                    response.with_body(Body::FailAfter(vec![
                        format!("event: update\ndata: {{\"id\":\"{echo}").into(),
                    ]))
                } else {
                    response
                };
                response.header("x-canary", &echo)
            } else {
                reply(case, 200, "[]", &problem, &echo)
            }
        })
        .await;
        let instance =
            InstanceUrl::parse(&if case == 6 { refused() } else { server.url() }).unwrap();
        c.assert_debug_absent("instance config", &instance);
        let api = Mastodon::new(http(), instance).with_token(&credential);
        let read = api.public_timeline();
        c.assert_debug_absent("client", &api);
        c.assert_debug_absent("read endpoint", &read);
        let prepared = read.clone().prepare().unwrap();
        c.assert_debug_absent("read Prepared", &prepared);
        let native = receive(&mut c, prepared).await;
        c.assert_debug_absent("native Result", &native);
        assert_eq!(native.is_ok(), case == 0);
        if let Err(e) = native {
            concord_error(&mut c, &e);
        }
        let result = read.send().await;
        c.assert_debug_absent("SDK Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        if let Err(e) = &result {
            error(&mut c, e);
            if case == 1 {
                assert!(e.to_string().contains("401"));
                assert_eq!(
                    e.api().unwrap().operation().unwrap().name(),
                    "Api.public_timeline"
                );
                assert_eq!(e.api().unwrap().message(), Some(token.as_str()));
            }
        }
        let host = StreamingHost::instance_host(api.instance_url());
        c.assert_debug_absent("streaming host", &host);
        let stream = api.public_stream(&host).unwrap();
        c.assert_debug_absent("stream endpoint", &stream);
        c.assert_debug_absent("stream Prepared", &stream.clone().prepare().unwrap());
        let result = stream.send().await;
        c.assert_debug_absent("stream Result", &result);
        match result {
            Ok(mut stream) => {
                assert!(matches!(case, 0 | 3 | 5));
                c.assert_debug_absent("open stream", &stream);
                let mut terminal = false;
                let mut payload_failure = false;
                for _ in 0..3 {
                    let Some(result) = stream.next().await else {
                        break;
                    };
                    c.assert_debug_absent("stream item Result", &result);
                    if let Err(e) = result {
                        c.assert_debug_absent("StreamError", &e);
                        c.assert_error_absent("StreamError and sources", &e);
                        c.assert_debug_absent("stream codec", &e.codec());
                        if let Some(codec) = e.codec() {
                            c.assert_error_absent("codec Display and sources", codec);
                        }
                        if let Some(read) = e.read_error() {
                            c.assert_error_absent("reader Display and sources", read);
                        }
                        c.assert_debug_absent("stream read error", &e.read_error());
                        if let Some(e) = e.transport() {
                            concord_error(&mut c, e);
                        }
                        assert!(e.operation().unwrap().contains("public_stream"));
                        if case == 3 && !e.is_terminal() {
                            assert_eq!(e.kind(), StreamErrorKind::Payload);
                            payload_failure = true;
                        }
                        if e.is_terminal() {
                            terminal = true;
                            break;
                        }
                    }
                }
                assert!(terminal);
                assert_eq!(payload_failure, case == 3);
                c.assert_debug_absent("finished stream", &stream);
            }
            Err(e) => {
                assert!(matches!(case, 1 | 2 | 4 | 6));
                error(&mut c, &e);
            }
        }
        if case == 0 {
            let status = api.status(concord_sdk_mastodon::StatusId::new("one").unwrap());
            let hashtag = api.hashtag_timeline(concord_sdk_mastodon::Hashtag::new("rust").unwrap());
            let instance = api.instance();
            c.assert_debug_absent("status endpoint", &status);
            c.assert_debug_absent("hashtag timeline endpoint", &hashtag);
            c.assert_debug_absent("instance endpoint", &instance);
            c.assert_debug_absent("status Prepared", &status.prepare().unwrap());
            c.assert_debug_absent("hashtag Prepared", &hashtag.prepare().unwrap());
            c.assert_debug_absent("instance Prepared", &instance.prepare().unwrap());
            let hashtag = api
                .hashtag_stream(&host, concord_sdk_mastodon::Hashtag::new("rust").unwrap())
                .unwrap();
            let local = api
                .local_hashtag_stream(&host, concord_sdk_mastodon::Hashtag::new("rust").unwrap())
                .unwrap();
            c.assert_debug_absent("hashtag stream endpoint", &hashtag);
            c.assert_debug_absent("local stream endpoint", &local);
            c.assert_debug_absent("hashtag stream Prepared", &hashtag.prepare().unwrap());
            c.assert_debug_absent("local stream Prepared", &local.prepare().unwrap());
            c.assert_on_wire(&server.requests());
        }
        server.shutdown().await;
    }
    let e = AccessToken::new(&format!("{token}\n")).unwrap_err();
    c.assert_debug_absent("InvalidToken", &e);
    c.assert_error_absent("InvalidToken", &e);
    assert!(c.checked() >= 150);
    println!(
        "mastodon: {} renderings; positive control passed",
        c.checked()
    );
}

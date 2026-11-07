//! Secrets: BasicAuth username/password (src/auth.rs:19), the sensitive path account SID
//! (src/lib.rs:24-25,75), and every field withheld by CreateMessage Debug (to/from/body,
//! message_intent, status_callback; src/models.rs:64). Each receives a distinct canary.
//! Successful decoded Message fixtures echo these into withheld account_sid, to/from/body
//! and error_message; provider message/more_info and headers echo the password. Fetch and
//! create cover seven scenarios through the SDK and native Prepared receives. Basic wire
//! control decodes the Authorization encoding. No wire exemptions.
//! Excludes native reqwest, raw headers/bodies, public clear field access, and the message
//! SID/status/error_code that Message explicitly prints. The generated wire client is private.
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

use concord_sdk_twilio::{BasicAuth, CreateMessage, Response, Twilio, TwilioError};

fn error(c: &mut Canaries, error: &TwilioError) {
    c.assert_debug_absent("SDK error", error);
    c.assert_error_absent("SDK error and sources", error);
    c.assert_debug_absent("error metadata", &error.meta());
    c.assert_debug_absent("provider error", &error.provider_error());
    c.assert_debug_absent("operation", &error.operation());
    if let Some(e) = error.concord() {
        concord_error(c, e);
    }
}

fn result(c: &mut Canaries, value: &Result<Response<concord_sdk_twilio::Message>, TwilioError>) {
    c.assert_debug_absent("SDK Result", value);
    match value {
        Ok(r) => {
            c.assert_debug_absent("response", r);
            c.assert_debug_absent("decoded Message", &r.value);
            c.assert_debug_absent("response metadata", &r.meta);
        }
        Err(e) => error(c, e),
    }
}

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("twilio");
    let username = c.issue();
    let password = c.issue();
    let account = c.issue();
    let to = c.issue();
    let from = c.issue();
    let body = c.issue();
    let intent = c.issue();
    let callback = format!("https://example.test/{}", c.issue());
    let message = CreateMessage::new(&to, &from, &body)
        .with_message_intent(&intent)
        .with_status_callback(&callback);
    c.assert_debug_absent("CreateMessage", &message);
    let auth = BasicAuth::new(&username, &password).unwrap();
    c.assert_debug_absent("BasicAuth", &auth);
    for case in 0..7 {
        let (echo, response_account, to, from, body) = (
            password.clone(),
            account.clone(),
            to.clone(),
            from.clone(),
            body.clone(),
        );
        let server = TestServer::start(move |r: &Recorded| {
            let success = format!(r#"{{"sid":"SM_one","account_sid":"{response_account}","status":"queued","to":"{to}","from":"{from}","body":"{body}","error_message":"{echo}"}}"#);
            let problem = format!(r#"{{"status":401,"code":20003,"message":"{echo}","more_info":"https://example.test/{echo}"}}"#);
            reply(case, if r.method == "POST" {201} else {200}, &success, &problem, &echo)
        }).await;
        let api = Twilio::new(http(), &account, auth.clone())
            .with_base_url(if case == 6 { refused() } else { server.url() })
            .unwrap();
        c.assert_debug_absent("client", &api);
        let read = api.prepare_get_message("SM_one").unwrap();
        let write = api.prepare_create_message(message.clone()).unwrap();
        c.assert_debug_absent("read Prepared", &read);
        c.assert_debug_absent("write Prepared", &write);
        let native = receive(&mut c, read).await;
        c.assert_debug_absent("native read Result", &native);
        assert_eq!(native.is_ok(), case == 0);
        if let Err(e) = native {
            concord_error(&mut c, &e);
        }
        let native = receive(&mut c, write).await;
        c.assert_debug_absent("native write Result", &native);
        assert_eq!(native.is_ok(), case == 0);
        if let Err(e) = native {
            concord_error(&mut c, &e);
        }
        let read = api.get_message("SM_one").await;
        let write = api.create_message(message.clone()).await;
        assert_eq!(read.is_ok(), case == 0);
        assert_eq!(write.is_ok(), case == 0);
        result(&mut c, &read);
        result(&mut c, &write);
        if case == 1 {
            let e = read.as_ref().unwrap_err();
            assert!(e.to_string().contains("401"));
            assert!(e.to_string().contains("TwilioApi.get_message"));
            assert_eq!(e.provider_error().unwrap().message, password);
        }
        if case == 0 {
            c.assert_on_wire(&server.requests());
        }
        let e = api.prepare_get_message("").unwrap_err();
        error(&mut c, &e);
        server.shutdown().await;
    }
    for e in [
        BasicAuth::new(&format!("{username}:"), &password).unwrap_err(),
        BasicAuth::new(&username, &format!("{password}\n")).unwrap_err(),
    ] {
        c.assert_debug_absent("AuthError", &e);
        c.assert_error_absent("AuthError", &e);
    }
    assert!(c.checked() >= 200);
    println!(
        "twilio: {} renderings; positive control passed",
        c.checked()
    );
}

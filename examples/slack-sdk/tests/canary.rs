//! Secrets: ClientConfig::token (src/wire.rs:35), sent as Bearer Authorization; the
//! sensitive users cursor (src/wire.rs:44, models.rs:10), obtained through a real decoder
//! and sent on the next page. Each has its own canary. Read users_page/conversation_info
//! cover all seven scenarios; users pagination and SDK response metadata are also formatted.
//! No writes in this SDK. No wire exemptions: a successful continuation sends the cursor.
//! Excludes native reqwest, explicit raw body/header access and clear provider/model fields
//! (messages, warnings, error codes, profiles). Private wire endpoints/Prepared are inaccessible.
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

fn concord_error(c: &mut Canaries, error: &concord::Error) {
    c.assert_debug_absent("concord::Error", error);
    c.assert_error_absent("concord::Error and sources", error);
    c.assert_debug_absent("error kind", &error.kind());
    if let Some(meta) = error.head_meta() {
        c.assert_debug_absent("error response metadata", &meta);
    }
}

use concord_sdk_slack::{Client, ClientConfig, ConversationOptions, Error, UsersOptions};

fn error(c: &mut Canaries, error: &Error) {
    c.assert_debug_absent("SDK error", error);
    c.assert_error_absent("SDK error and sources", error);
    c.assert_debug_absent("error metadata", &error.meta());
    match error {
        Error::Concord { error, .. } => concord_error(c, error),
        Error::Api { error, .. } => c.assert_debug_absent("provider error", error),
        Error::Http { body, .. } | Error::RateLimited { body, .. } => {
            c.assert_debug_absent("bounded error body", body);
            c.assert_debug_absent("parsed provider failure", &body.provider_error());
            if let Some(e) = body.read_error() {
                concord_error(c, e);
            }
        }
        _ => {}
    }
}

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("slack");
    let token = c.issue();
    let cursor_text = c.issue();
    let mut cursor = None;
    let options = UsersOptions::default();
    c.assert_debug_absent("users options", &options);
    c.assert_debug_absent("conversation options", &ConversationOptions::default());
    for case in 0..7 {
        let (echo, response_cursor) = (token.clone(), cursor_text.clone());
        let server = TestServer::start(move |r: &Recorded| {
            let success = if r.path() == "/conversations.info" {
                r#"{"ok":true,"channel":{"id":"C_one","name":"general"}}"#.into()
            } else {
                format!(r#"{{"ok":true,"members":[{{"id":"U_one","name":"alice"}}],"response_metadata":{{"next_cursor":"{response_cursor}"}}}}"#)
            };
            // Only the cursor and headers are promised withheld. Provider codes/messages are clear.
            let problem = format!(r#"{{"ok":false,"error":"invalid_auth","response_metadata":{{"next_cursor":"{response_cursor}"}}}}"#);
            reply(case, 200, &success, &problem, &echo)
        }).await;
        let config = ClientConfig {
            token: token.clone(),
        };
        c.assert_debug_absent("config", &config);
        let api = Client::new(http(), config)
            .with_base_url(if case == 6 { refused() } else { server.url() })
            .unwrap();
        c.assert_debug_absent("client", &api);
        let response = api.users_page(&options, cursor.as_ref()).await;
        c.assert_debug_absent("users Result", &response);
        assert_eq!(response.is_ok(), case == 0);
        match response {
            Ok(r) => {
                c.assert_debug_absent("users response", &r);
                c.assert_debug_absent("response metadata", &r.meta);
                c.assert_debug_absent("body metadata", &r.response_metadata);
                cursor = r.response_metadata.unwrap().next_cursor;
                c.assert_debug_absent("decoded cursor", &cursor);
                assert_eq!(cursor.as_ref().unwrap().as_str(), cursor_text);
                let continued = api.users_page(&options, cursor.as_ref()).await.unwrap();
                c.assert_debug_absent("continued users response", &continued);
            }
            Err(e) => {
                error(&mut c, &e);
                if case == 1 {
                    assert_eq!(e.meta().unwrap().status.as_u16(), 401);
                    assert!(e.to_string().contains("401"));
                    assert_eq!(
                        e.meta().unwrap().operation.unwrap().name(),
                        "SlackApi.users_list"
                    );
                }
            }
        }
        let response = api
            .conversation_info("C_one", ConversationOptions::default())
            .await;
        c.assert_debug_absent("conversation Result", &response);
        assert_eq!(response.is_ok(), case == 0);
        match response {
            Ok(r) => {
                c.assert_debug_absent("conversation response", &r);
                c.assert_debug_absent("conversation", &r.value);
                c.assert_debug_absent("conversation metadata", &r.meta);
            }
            Err(e) => error(&mut c, &e),
        }
        let mut pages = api.users(options.clone());
        c.assert_debug_absent("users traversal", &pages);
        let page = pages.next_page().await;
        c.assert_debug_absent("users page Result", &page);
        if let Err(e) = page {
            c.assert_display_absent("PagesError", &e);
            c.assert_error_absent("PagesError and sources", &e);
            error(&mut c, e.provider_error().unwrap());
        }
        if case == 0 {
            c.assert_on_wire(&server.requests());
        }
        server.shutdown().await;
    }
    assert!(c.checked() >= 100);
    println!("slack: {} renderings; positive control passed", c.checked());
}

//! Secrets: PostmarkConfig::token / #[sensitive] token (src/lib.rs:290), sent in
//! X-Postmark-Server-Token. A distinct canary occupies that credential. The server echoes it
//! into withheld error-document Message, response headers, and corrupt bodies. Read list and
//! write send_email cover all seven scenarios, with native Prepared receives alongside them;
//! success also decodes a rejected batch entry. No wire exemptions.
//! Excludes native reqwest values, explicit raw body/header accessors and clear email,
//! Acceptance and Message fields (including an acceptance's Message, printed by design).
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

use concord_sdk_postmark::{Email, Error, Postmark, PostmarkConfig};

fn email() -> Email {
    Email {
        from: "sender@example.test".into(),
        to: "recipient@example.test".into(),
        subject: "Receipt".into(),
        text_body: Some("Thank you".into()),
        html_body: None,
        message_stream: None,
    }
}

fn error(c: &mut Canaries, error: &Error) {
    c.assert_debug_absent("SDK error", error);
    c.assert_error_absent("SDK error and sources", error);
    match error {
        Error::Request(e) | Error::UnknownOutcome(e) => concord_error(c, e),
        Error::Api { document, .. } => c.assert_debug_absent("error document", document),
        Error::UnmatchedBatch { results, .. } => c.assert_debug_absent("batch results", results),
        _ => {}
    }
}

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("postmark");
    let token = c.issue();
    c.assert_debug_absent("email (clear nonsecret data)", &email());
    for case in 0..7 {
        let echo = token.clone();
        let server = TestServer::start(move |r: &Recorded| {
            let problem = format!(r#"{{"ErrorCode":403,"Message":"{echo}"}}"#);
            let success = if r.path() == "/email/batch" {
                format!("[{problem}]")
            } else if r.method == "POST" {
                r#"{"To":"recipient@example.test","SubmittedAt":"2026-11-07T00:00:00Z","MessageID":"one","ErrorCode":0,"Message":"OK"}"#.into()
            } else {
                r#"{"TotalCount":1,"Messages":[{"MessageID":"one","Status":"Sent"}]}"#.into()
            };
            reply(case, 200, &success, &problem, &echo)
        }).await;
        let config = PostmarkConfig {
            token: token.clone(),
        };
        c.assert_debug_absent("config", &config);
        let api = Postmark::new(http(), config)
            .with_base_url(if case == 6 { refused() } else { server.url() })
            .unwrap();
        let outbound = api.outbound();
        let read = outbound.list(1, 0);
        let write = outbound.send_email(email());
        c.assert_debug_absent("client", &api);
        c.assert_debug_absent("outbound scope", &outbound);
        c.assert_debug_absent("read endpoint", &read);
        c.assert_debug_absent("write endpoint", &write);
        for prepared in [
            read.clone().prepare().map(|p| format!("{p:?} {p:#?}")),
            write.clone().prepare().map(|p| format!("{p:?} {p:#?}")),
        ] {
            c.assert_absent("Prepared", &prepared.unwrap());
        }
        let native = receive(&mut c, read.clone().prepare().unwrap()).await;
        c.assert_debug_absent("native read Result", &native);
        assert_eq!(native.is_ok(), case == 0);
        if let Err(e) = native {
            concord_error(&mut c, &e);
        }
        let native = receive(&mut c, write.clone().prepare().unwrap()).await;
        c.assert_debug_absent("native write Result", &native);
        assert_eq!(native.is_ok(), case == 0);
        if let Err(e) = native {
            concord_error(&mut c, &e);
        }
        let result = read.send().await;
        c.assert_debug_absent("read Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        if let Err(e) = &result {
            error(&mut c, e);
            if case == 1 {
                assert!(e.to_string().contains("401"));
            }
        }
        let result = write.send().await;
        c.assert_debug_absent("write Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        if let Err(e) = &result {
            error(&mut c, e);
        }
        if case == 0 {
            let batch = outbound.send_batch(vec![email()]);
            c.assert_debug_absent("batch endpoint", &batch);
            let result = batch.send().await.unwrap();
            c.assert_debug_absent("batch results with withheld rejection Message", &result);
            assert!(
                matches!(&result[0], concord_sdk_postmark::BatchResult::Rejected(d) if d.message == token)
            );
            let unmatched = outbound
                .send_batch(vec![email(), email()])
                .send()
                .await
                .unwrap_err();
            assert!(unmatched.is_unknown_outcome());
            error(&mut c, &unmatched);
            c.assert_on_wire(&server.requests());
        }
        server.shutdown().await;
    }
    assert!(c.checked() >= 200);
    println!(
        "postmark: {} renderings; positive control passed",
        c.checked()
    );
}

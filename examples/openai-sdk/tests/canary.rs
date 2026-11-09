//! Secret inputs, one canary each:
//! - API token: `OpenAi::new(http, token)` (src/lib.rs:321), sent as `Authorization: Bearer`.
//! - The generated client's `#[sensitive] header(AUTHORIZATION)` (src/lib.rs:26), given to
//!   `OpenAiApi::new` as a raw `HeaderValue` not flagged sensitive, so the declaration alone
//!   must redact it.
//!
//! The server echoes the token into response headers (Concord's `ResponseMeta` prints names
//! only) and into malformed, wrong-type and truncated JSON bodies (Concord never prints body
//! content). The README says provider body fields and debug/error output can contain sensitive
//! data (README.md:60), so no canary is echoed into the OpenAI error document, SSE event data
//! or file content, which the SDK returns or prints in clear. Writes (create response, batch
//! upload) and reads (file, content, stream, batch poll, batch results) run all seven
//! scenarios; the raw client runs a native `Prepared` and its generated `send()`. Not covered:
//! native reqwest values, `Received`/`concord::Error` unredacted accessors, decoded models and
//! `ProviderError` fields. No wire exemptions.

use concord::reqwest::header::HeaderValue;
use concord_check_canary::Canaries;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use concord_sdk_openai::{
    Batch, BatchInput, Error, OpenAi, OpenAiApi, OpenAiApiConfig, ResponseRequest, batch_jsonl,
};
use std::time::Duration;

const ERROR: &str = include_str!("../fixtures/error.json");
const RESPONSE: &str = include_str!("../fixtures/response.json");
const FILE: &str = include_str!("../fixtures/file.json");
const BATCH: &str = include_str!("../fixtures/batch-completed.json");
const OUTPUT: &str = include_str!("../fixtures/output.jsonl");
const EVENTS: &str = include_str!("../fixtures/events.sse");

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

/// `in_body`: whether a malformed or wrong-type body may carry the echo (only where Concord,
/// not the SDK or the caller, would read it).
fn reply(case: usize, media: &str, success: &str, echo: &str, in_body: bool) -> Reply {
    let body = if in_body { echo } else { "x" };
    let response = match case {
        0 | 6 => Reply::text(200, media, success),
        1 => Reply::json(401, ERROR.replace("invalid_value", "invalid_api_key")),
        2 => Reply::json(503, ERROR.replace("invalid_value", "server_error")),
        3 => Reply::text(200, media, format!("data: {{invalid {body}\n\n")),
        4 => Reply::text(200, "text/html", body),
        5 => Reply::text(200, media, "")
            .with_body(Body::FailAfter(vec![format!(r#"{{"id":"{echo}"#).into()])),
        _ => unreachable!(),
    };
    response
        .header("x-private", echo)
        .header("set-cookie", echo)
}

fn concord_error<D: 'static>(c: &mut Canaries, e: &concord::Error<D>) {
    c.assert_debug_absent("Concord error", e);
    c.assert_error_absent("Concord error/source chain", e);
    c.assert_debug_absent("error kind", &e.kind());
    c.assert_debug_absent("error head", &e.head());
    c.assert_debug_absent("error metadata", &e.head_meta());
    c.assert_debug_absent("error operation", &e.operation());
}

fn error(c: &mut Canaries, e: &Error) {
    c.assert_debug_absent("SDK error", e);
    c.assert_display_absent("SDK error", e);
    c.assert_error_absent("SDK error/source chain", e);
    match e {
        Error::Concord(e) => concord_error(c, e),
        Error::Provider { meta, error } => {
            c.assert_debug_absent("provider metadata", meta);
            c.assert_debug_absent("provider document", error);
        }
        Error::Decode { meta, error } => {
            c.assert_debug_absent("decode metadata", meta);
            concord_error(c, error);
        }
        _ => {}
    }
}

fn check<T: std::fmt::Debug>(c: &mut Canaries, label: &str, result: &Result<T, Error>) {
    c.assert_debug_absent(label, result);
    if let Err(e) = result {
        error(c, e);
    }
}

async fn receive<T: std::fmt::Debug, D: 'static>(
    c: &mut Canaries,
    prepared: concord::Prepared<T, D>,
) {
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

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("openai");
    let token = c.issue();
    let raw_token = c.issue();
    let jsonl = batch_jsonl(&[BatchInput::response(
        "one",
        ResponseRequest::new("fixture-model", "hello"),
    )])
    .unwrap();
    let batch: Batch = serde_json::from_str(BATCH).unwrap();
    for case in 0..7 {
        let echo = token.clone();
        let server = TestServer::start(move |r: &Recorded| {
            let path = r.path();
            if path == "/v1/responses" && r.body_text().contains(r#""stream":true"#) {
                reply(case, "text/event-stream", EVENTS, &echo, false)
            } else if path.ends_with("/content") {
                reply(case, "application/jsonl", OUTPUT, &echo, false)
            } else if path == "/v1/responses" {
                reply(case, "application/json", RESPONSE, &echo, true)
            } else if path.starts_with("/v1/batches") {
                reply(case, "application/json", BATCH, &echo, true)
            } else {
                reply(case, "application/json", FILE, &echo, true)
            }
        })
        .await;
        let base = if case == 6 { refused() } else { server.url() };
        let client = OpenAi::new(http(), &token)
            .unwrap()
            .with_base_url(&base)
            .unwrap();
        c.assert_debug_absent("client", &client);
        let request = ResponseRequest::new("fixture-model", "hello");

        let created = client.create_response(request.clone()).await;
        check(&mut c, "create response Result", &created);
        assert_eq!(created.is_ok(), case == 0);
        if let Err(e) = &created {
            match (case, e) {
                (1, Error::Provider { meta, error }) => {
                    // Safe diagnosis: status, operation and provider code survive.
                    assert_eq!(meta.status().as_u16(), 401);
                    assert_eq!(meta.operation().unwrap().name(), "OpenAiApi.response");
                    let text = e.to_string();
                    assert!(
                        text.contains("401") && text.contains("invalid_api_key"),
                        "{text}"
                    );
                    assert_eq!(error.as_ref().unwrap().param.as_deref(), Some("model"));
                }
                (2, Error::Provider { meta, .. }) => assert_eq!(meta.status().as_u16(), 503),
                (3..=5, Error::Decode { meta, .. }) => {
                    assert_eq!(meta.status().as_u16(), 200);
                    assert_eq!(meta.headers()["x-private"], token.as_str());
                }
                (6, Error::Concord(e)) => assert_eq!(e.kind(), concord::ErrorKind::Transport),
                _ => panic!("case {case}: {e:?}"),
            }
        }
        let uploaded = client.upload_batch_file("input.jsonl", &jsonl).await;
        check(&mut c, "upload Result", &uploaded);
        assert_eq!(uploaded.is_ok(), case == 0);
        let file = client.file("file_input").await;
        check(&mut c, "file Result", &file);
        assert_eq!(file.is_ok(), case == 0);
        let content = client.content("file_output", 1 << 20).await;
        check(&mut c, "content Result", &content);
        assert_eq!(content.is_ok(), matches!(case, 0 | 3 | 4));
        let polled = client.poll_batch("batch_fixture", 1, Duration::ZERO).await;
        check(&mut c, "poll Result", &polled);
        assert_eq!(polled.is_ok(), case == 0);
        let results = client.batch_results(&batch, 1 << 20).await;
        c.assert_debug_absent("batch results", &results);
        for (_, e) in &results.as_ref().unwrap().file_errors {
            error(&mut c, e);
        }

        let stream = client.stream_response(request.clone()).await;
        match stream {
            Ok(mut events) => {
                c.assert_debug_absent("stream metadata", &events.meta);
                let mut ok = true;
                loop {
                    match events.next().await {
                        Ok(Some(event)) => c.assert_debug_absent("event", &event),
                        Ok(None) => break,
                        Err(e) => {
                            error(&mut c, &e);
                            ok = false;
                            break;
                        }
                    }
                }
                assert_eq!(ok, case == 0);
            }
            Err(e) => {
                error(&mut c, &e);
                assert!(matches!(case, 1 | 2 | 4 | 6), "case {case}: {e:?}");
            }
        }

        let raw = OpenAiApi::new(
            http(),
            OpenAiApiConfig {
                authorization: HeaderValue::from_str(&format!("Bearer {raw_token}")).unwrap(),
            },
        )
        .with_base_url(&base)
        .unwrap();
        c.assert_debug_absent("raw client", &raw);
        let op = raw.file("file_input");
        c.assert_debug_absent("file endpoint", &op);
        let write = raw.response(request.clone());
        c.assert_debug_absent("response endpoint", &write);
        c.assert_debug_absent("response Prepared", &write.clone().prepare().unwrap());
        c.assert_debug_absent("batch endpoint", &raw.batch_status("batch_fixture"));
        receive(&mut c, op.prepare().unwrap()).await;
        let result = raw.batch_status("batch_fixture").send().await;
        check(&mut c, "raw batch status Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        let result = write.send().await;
        check(&mut c, "raw response Result", &result);
        assert_eq!(result.is_ok(), case == 0);

        if case == 0 {
            let requests = server.requests();
            c.assert_on_wire(&requests);
            let bearer = format!("Bearer {token}");
            let raw_bearer = format!("Bearer {raw_token}");
            for r in &requests {
                let sent = r.header("authorization");
                assert!(sent == Some(bearer.as_str()) || sent == Some(raw_bearer.as_str()));
            }
        }
        if case == 6 {
            assert!(server.requests().is_empty());
        }
        server.shutdown().await;
    }
    let e = OpenAi::new(http(), &format!("{token}\n")).unwrap_err();
    error(&mut c, &e);
    assert!(c.checked() >= 400);
    println!(
        "openai: {} renderings; positive control passed",
        c.checked()
    );
}

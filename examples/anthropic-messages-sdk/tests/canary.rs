//! Secrets: the API key, the only secret input (`AnthropicConfig::api_key`, `#[sensitive]
//! header("x-api-key")`, `src/lib.rs:79`; `Anthropic::new` is the only constructor). It has its
//! own canary and travels as `x-api-key`. Response-only values the SDK withholds (README
//! "Redaction", `src/error.rs:4-8`): the error `message` (`ProviderError`, `ApiError`), generated
//! text (`MessageStream`, `PartialMessage`, `Incomplete`), the stream `error` event's message and
//! a serde error quoting event data (`IncompleteCause::InvalidEvent`). The server echoes the key
//! into each of them (so every withheld value reaches the decoder), into headers it does not
//! project (`x-private`, `set-cookie`, an invalid `retry-after`; `request-id` is printed by
//! design) and into every malformed, wrong-media and truncated body. `count_tokens` (read) and
//! `create` (write) cover the seven scenarios through the policy `send()` and the native
//! `Prepared`; `stream` covers them too (malformed = an event whose data quotes the key,
//! truncated = a connection lost after a delta), and the success case adds the stream endings
//! `ErrorEvent`, `MalformedStream`, `EndOfStream` + `AlreadyFailed` and `Protocol`. A key refused
//! at build time gets its own canary in a separate test.
//! Not covered: native reqwest values; explicit accessors (`ApiError::message`, `meta().headers()`,
//! `Incomplete::partial().text()`, `invalid_event_source`); `Message`, `StreamEvent` and
//! `MessageRequest`, whose derived `Debug` shows content by design (README "Redaction"), so a
//! completed stream's final `Message` is checked through `text()` (the canary arrived) and not
//! formatted. No wire exemptions.

use concord_check_canary::Canaries;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use std::time::SystemTime;

use concord_sdk_anthropic_messages::{
    Anthropic, AnthropicConfig, Error, IncompleteCause, MessageRequest, MessageStream, StreamState,
    http_client,
};

fn refused() -> String {
    concord_example_testserver::refused_url()
}

fn fixture(name: &str) -> String {
    let path = format!("{}/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn event(name: &str, data: &str) -> String {
    format!("event: {name}\ndata: {data}\n\n")
}

fn delta(text: &str) -> String {
    let data = format!(
        r#"{{"type":"content_block_delta","index":0,"delta":{{"type":"text_delta","text":"{text}"}}}}"#
    );
    event("content_block_delta", &data)
}

fn sse(status: u16, body: String) -> Reply {
    Reply::text(status, "text/event-stream", body)
}

// Echo the key into headers and into every body whose content the SDK withholds.
fn finite(case: usize, success: &str, problem: &str, echo: &str) -> Reply {
    let response = match case {
        0 | 6 => Reply::json(200, success),
        1 => Reply::json(401, problem),
        2 => Reply::json(529, problem),
        3 => Reply::json(200, format!("{{invalid {echo}")),
        4 => Reply::text(200, "text/html", echo),
        5 => Reply::json(200, "{}").with_body(Body::FailAfter(vec![concord::bytes::Bytes::from(
            format!("{{\"incomplete\":\"{echo}"),
        )])),
        _ => unreachable!(),
    };
    headers(response, echo)
}

fn headers(reply: Reply, echo: &str) -> Reply {
    reply
        .header("request-id", "req_synthetic")
        .header("x-private", echo)
        .header("set-cookie", echo)
        .header("retry-after", echo)
}

// The stream reply: the request's prompt names the ending in the success case.
fn streamed(case: usize, prompt: &str, problem: &str, echo: &str) -> Reply {
    let basic: Vec<String> = fixture("stream-basic.sse")
        .split_inclusive("\n\n")
        .map(str::to_owned)
        .collect();
    let head = basic[..3].concat();
    let quoted = event(
        "content_block_delta",
        &format!(r#"{{"type":"content_block_delta","index":"{echo}"}}"#),
    );
    let response = match (case, prompt) {
        (0 | 6, "complete") => sse(200, format!("{head}{}{}", delta(echo), basic[5..].concat())),
        (0, "error-event") => sse(
            200,
            format!(
                "{head}{}{}",
                delta(echo),
                event(
                    "error",
                    &format!(
                        r#"{{"type":"error","error":{{"type":"overloaded_error","message":"{echo}"}}}}"#
                    )
                )
            ),
        ),
        (0, "cut-line") => sse(200, format!("{head}{}", delta(echo).trim_end())),
        (0, "ends-early") => sse(200, format!("{head}{}", delta(echo))),
        (0, "out-of-order") => sse(200, format!("{}{}", basic[0], delta(echo))),
        (1, _) => Reply::json(401, problem),
        (2, _) => Reply::json(529, problem),
        (3, _) => sse(200, format!("{head}{}{quoted}", delta(echo))),
        (4, _) => Reply::text(200, "text/html", echo),
        (5, _) => Reply::new(200)
            .header("content-type", "text/event-stream")
            .with_body(Body::FailAfter(vec![
                format!("{head}{}", delta(echo)).into(),
            ])),
        other => unreachable!("{other:?}"),
    };
    headers(response, echo)
}

/// The native path; a receive error has no document type, so each error is checked here.
async fn receive<T, D: 'static>(
    c: &mut Canaries,
    prepared: concord::Prepared<T, D>,
) -> Result<T, ()> {
    let result = prepared.receive().await;
    c.assert_debug_absent("Received Result", &result);
    match result {
        Ok(received) => {
            c.assert_debug_absent("response head", &received.head());
            c.assert_debug_absent("response metadata", &received.meta());
            let decoded = received.decode().await;
            if let Err(e) = &decoded {
                concord_error(c, e);
            }
            decoded.map_err(drop)
        }
        Err(error) => {
            concord_error(c, &error);
            Err(())
        }
    }
}

fn concord_error<D: 'static>(c: &mut Canaries, error: &concord::Error<D>) {
    c.assert_debug_absent("concord::Error", error);
    c.assert_error_absent("concord::Error and sources", error);
    c.assert_debug_absent("error kind", &error.kind());
    if let Some(meta) = error.head_meta() {
        c.assert_debug_absent("error response metadata", &meta);
    }
}

fn error(c: &mut Canaries, error: &Error) {
    c.assert_debug_absent("SDK error", error);
    c.assert_error_absent("SDK error and sources", error);
    c.assert_debug_absent("error request id", &error.request_id());
    c.assert_debug_absent("error status", &error.status());
    match error {
        Error::Concord(e) => concord_error(c, e),
        Error::Api(api) => {
            c.assert_debug_absent("API error", api);
            c.assert_display_absent("API error", api);
            c.assert_debug_absent("provider error", &api.provider_error());
            c.assert_debug_absent("error type", &api.error_type());
            c.assert_debug_absent("API error metadata", api.meta());
            c.assert_debug_absent("retry-after", &api.retry_after(SystemTime::UNIX_EPOCH));
            c.assert_debug_absent("API error failure", api.failure());
            if let concord::Document::Broken(e) = api.failure().state() {
                concord_error(c, e);
            }
        }
        Error::Incomplete(incomplete) => {
            c.assert_debug_absent("incomplete", incomplete);
            c.assert_display_absent("incomplete", incomplete);
            c.assert_debug_absent("incomplete cause", incomplete.cause());
            c.assert_debug_absent("partial message", incomplete.partial());
            c.assert_debug_absent("stop reason", &incomplete.partial().stop_reason());
            match incomplete.cause() {
                IncompleteCause::Transport(e) => concord_error(c, e),
                IncompleteCause::ErrorEvent(e) => c.assert_debug_absent("error event", e),
                IncompleteCause::MalformedStream(r) | IncompleteCause::EventTooLarge(r) => {
                    c.assert_debug_absent("read error", r);
                    c.assert_error_absent("read error and sources", r);
                }
                _ => {}
            }
        }
        Error::InvalidRequest(e) => c.assert_debug_absent("invalid request", e),
        _ => {}
    }
}

// Reads a stream to its end, formatting it after each event; returns the failure, if any.
async fn drain(c: &mut Canaries, stream: &mut MessageStream) -> Option<Error> {
    loop {
        let next = stream.next().await;
        c.assert_debug_absent("stream", stream);
        c.assert_debug_absent("stream partial", stream.partial());
        match next {
            Ok(Some(event)) => c.assert_debug_absent("event type", event.event_type()),
            Ok(None) => return None,
            Err(e) => {
                error(c, &e);
                return Some(e);
            }
        }
    }
}

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("anthropic-messages");
    let key = c.issue();
    let config = AnthropicConfig {
        api_key: key.clone(),
    };
    c.assert_debug_absent("config", &config);
    for case in 0..7 {
        let echo = key.clone();
        let server = TestServer::start(move |r: &Recorded| {
            let body = r.body_text();
            let problem = format!(
                r#"{{"type":"error","error":{{"type":"authentication_error","message":"{echo}"}},"request_id":"req_doc"}}"#
            );
            if r.path().ends_with("/count_tokens") {
                finite(case, r#"{"input_tokens":25}"#, &problem, &echo)
            } else if body.contains(r#""stream":true"#) {
                let prompt: serde_json::Value = serde_json::from_str(&body).unwrap();
                let prompt = prompt["messages"][0]["content"].as_str().unwrap().to_owned();
                streamed(case, &prompt, &problem, &echo)
            } else {
                finite(case, &fixture("message.json"), &problem, &echo)
            }
        })
        .await;
        let base = if case == 6 { refused() } else { server.url() };
        let api = Anthropic::new(http_client().unwrap(), config.clone())
            .with_base_url(&base)
            .unwrap();
        let request = MessageRequest::user("claude-opus-5-5", 256, "complete").unwrap();
        let messages = api.messages();
        let read = messages.count_tokens(request.count_tokens());
        let write = messages.create(request.clone());
        let stream = messages.stream(request.clone().streaming());
        c.assert_debug_absent("client", &api);
        c.assert_debug_absent("messages scope", &messages);
        c.assert_debug_absent("read endpoint", &read);
        c.assert_debug_absent("write endpoint", &write);
        c.assert_debug_absent("stream endpoint", &stream);
        c.assert_debug_absent("stream Prepared", &stream.clone().prepare().unwrap());
        c.assert_debug_absent("write Prepared", &write.clone().prepare().unwrap());
        let prepared = read.clone().prepare().unwrap();
        c.assert_debug_absent("read Prepared", &prepared);
        let native = receive(&mut c, prepared).await;
        c.assert_debug_absent("native Result", &native);
        assert_eq!(native.is_ok(), case == 0);

        let result = read.send().await;
        c.assert_debug_absent("read Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        match &result {
            Ok(response) => c.assert_debug_absent("read metadata", &response.meta),
            Err(e) => error(&mut c, e),
        }
        let result = write.send().await;
        c.assert_debug_absent("write Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        match &result {
            Ok(response) => {
                c.assert_debug_absent("write metadata", &response.meta);
                c.assert_debug_absent("write request id", &response.request_id());
            }
            Err(e) => {
                error(&mut c, e);
                // Safe diagnosis survives: the status and error type of the 401.
                if case == 1 {
                    assert_eq!(e.status().map(|s| s.as_u16()), Some(401));
                    let shown = e.to_string();
                    assert!(shown.contains("401"), "{shown}");
                    assert!(shown.contains("authentication_error"), "{shown}");
                    assert!(shown.contains("req_synthetic"), "{shown}");
                    // The withheld message reached the decoder.
                    let Error::Api(api) = e else { panic!("{e:?}") };
                    assert_eq!(api.message(), Some(key.as_str()));
                }
                if case == 3 {
                    let Error::Concord(e) = e else {
                        panic!("{e:?}")
                    };
                    assert_eq!(e.kind(), concord::ErrorKind::Decode);
                }
            }
        }

        let result = stream.send().await;
        c.assert_debug_absent("stream Result", &result);
        assert_eq!(result.is_ok(), matches!(case, 0 | 3 | 5));
        match result {
            Ok(response) => {
                c.assert_debug_absent("stream metadata", &response.meta);
                let mut stream = response.value;
                let failure = drain(&mut c, &mut stream).await;
                assert_eq!(failure.is_none(), case == 0);
                if let Some(Error::Incomplete(incomplete)) = &failure {
                    // The withheld generated text (and quoted data) reached the decoder.
                    assert!(incomplete.partial().text().contains(&key));
                    if case == 3 {
                        let source = incomplete.invalid_event_source().unwrap();
                        assert!(source.to_string().contains(&key));
                    }
                }
                if case == 0 {
                    assert_eq!(stream.state(), StreamState::Complete);
                    // The withheld generated text reached the decoder.
                    assert!(stream.partial().text().contains(&key));
                    let message = stream.final_message().await.unwrap();
                    assert!(message.text().contains(&key));
                }
            }
            Err(e) => error(&mut c, &e),
        }

        if case == 0 {
            for (prompt, expected) in [
                ("error-event", "ErrorEvent"),
                ("cut-line", "MalformedStream"),
                ("ends-early", "EndOfStream"),
                ("out-of-order", "Protocol"),
            ] {
                let request = MessageRequest::user("claude-opus-5-5", 256, prompt).unwrap();
                let mut stream = messages
                    .stream(request.streaming())
                    .send()
                    .await
                    .unwrap()
                    .value;
                let failure = drain(&mut c, &mut stream).await.unwrap();
                let Error::Incomplete(incomplete) = &failure else {
                    panic!("{failure:?}")
                };
                assert!(format!("{:?}", incomplete.cause()).starts_with(expected));
                // A second call reports that the stream had already failed.
                let again = stream.next().await.unwrap_err();
                error(&mut c, &again);
                c.assert_debug_absent("failed stream", &stream);
            }
            c.assert_on_wire(&server.requests());
            for r in server.requests() {
                assert_eq!(r.header("x-api-key"), Some(key.as_str()));
            }
        }
        server.shutdown().await;
    }
    assert!(c.checked() >= 300);
    println!(
        "anthropic-messages: {} renderings; positive control passed",
        c.checked()
    );
}

#[tokio::test]
async fn refused_key_never_prints() {
    let mut c = Canaries::new("anthropic-messages");
    let key = format!("{}\n", c.issue());
    let api = Anthropic::new(http_client().unwrap(), AnthropicConfig { api_key: key });
    c.assert_debug_absent("client with a refused key", &api);
    let request = MessageRequest::user("claude-opus-5-5", 256, "complete").unwrap();
    let e = api.messages().create(request).send().await.unwrap_err();
    error(&mut c, &e);
    let Error::Concord(e) = e else {
        panic!("{e:?}")
    };
    assert_eq!(e.kind(), concord::ErrorKind::Build);
}

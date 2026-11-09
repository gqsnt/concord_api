//! Fresh consumer workflows built from the README and generated public rustdoc.

use std::{
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime},
};

use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use concord_sdk_anthropic_messages::{
    API_VERSION, Anthropic, AnthropicConfig, ApiErrorType, Error, IncompleteCause, MessageRequest,
    Role, StopReason, StreamEvent, StreamState,
    concord::{Delivery, ErrorKind},
    http_client,
};
use serde_json::{Value, json};

const KEY: &str = "sk-ant-fictional-first-use-secret";
const MODEL: &str = "claude-opus-5-5";
const PROMPT: &str = "Describe a quiet café in one sentence.";

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("fixture operation exceeded five seconds")
}

fn request() -> MessageRequest {
    MessageRequest::user(MODEL, 128, PROMPT).unwrap()
}

fn client(server: &TestServer) -> Anthropic {
    Anthropic::new(
        http_client().unwrap(),
        AnthropicConfig {
            api_key: KEY.into(),
        },
    )
    .with_base_url(server.url())
    .unwrap()
}

fn assert_wire(recorded: &Recorded, streaming: bool) {
    assert_eq!(recorded.method, "POST");
    assert_eq!(recorded.target, "/v1/messages");
    assert_eq!(recorded.query(), None);
    assert_eq!(recorded.header_all("x-api-key"), [KEY]);
    assert_eq!(recorded.header_all("anthropic-version"), [API_VERSION]);
    assert_eq!(recorded.header("content-type"), Some("application/json"));
    assert_eq!(
        recorded.header("accept"),
        Some(if streaming {
            "text/event-stream"
        } else {
            "application/json"
        })
    );
    assert_eq!(recorded.header("authorization"), None);
    let mut expected = json!({
        "model": MODEL,
        "max_tokens": 128,
        "messages": [{"role": "user", "content": PROMPT}],
    });
    if streaming {
        expected["stream"] = json!(true);
    }
    assert_eq!(
        serde_json::from_slice::<Value>(&recorded.body).unwrap(),
        expected
    );
}

#[tokio::test]
async fn create_text_message_and_inspect_blocks() {
    let server = TestServer::start(|_: &Recorded| -> Reply {
        Reply::json(
            200,
            json!({
                "id": "msg_first_use_create",
                "type": "message",
                "role": "assistant",
                "model": MODEL,
                "content": [
                    {"type": "text", "text": "A quiet café glows warmly"},
                    {"type": "text", "text": " as rain taps the windows."},
                ],
                "stop_reason": "end_turn",
                "stop_sequence": null,
                "usage": {"input_tokens": 19, "output_tokens": 16},
            })
            .to_string(),
        )
        .header("request-id", "req_first_use_create")
        .header("anthropic-organization-id", "org_fixture")
    })
    .await;
    let api = client(&server);
    assert!(!format!("{api:?}").contains(KEY));
    assert!(!format!("{:?}", api.messages().create(request())).contains(KEY));
    let response = within(api.messages().create(request()).send())
        .await
        .unwrap();
    assert_eq!(response.request_id(), Some("req_first_use_create"));
    assert_eq!(response.meta.status().as_u16(), 200);
    assert_eq!(
        response.meta.headers()["anthropic-organization-id"],
        "org_fixture"
    );
    assert!(response.meta.operation().is_some());
    let message = response.value;
    assert_eq!(message.id, "msg_first_use_create");
    assert_eq!(message.kind, "message");
    assert_eq!(message.role, Role::Assistant);
    assert_eq!(message.model, MODEL);
    assert_eq!(message.stop_reason, Some(StopReason::EndTurn));
    assert_eq!(message.stop_sequence, None);
    assert_eq!(message.content.len(), 2);
    assert_eq!(message.content[0].block_type(), "text");
    assert_eq!(
        message.content[0].as_text(),
        Some("A quiet café glows warmly")
    );
    assert_eq!(
        message.content[1].as_text(),
        Some(" as rain taps the windows.")
    );
    assert_eq!(
        message.text(),
        "A quiet café glows warmly as rain taps the windows."
    );
    let usage = message.usage.as_ref().unwrap();
    assert_eq!(usage.input_tokens, Some(19));
    assert_eq!(usage.output_tokens, Some(16));
    assert_eq!(usage.cache_read_input_tokens, None);
    assert_wire(&server.single(), false);
    server.shutdown().await;

    // Every failed send is a separate loopback fixture, so single() detects retries.
    for case in ["rate_limit", "error_html", "bad_message", "bad_media"] {
        let server = TestServer::start(move |_: &Recorded| -> Reply {
            let reply = match case {
                "rate_limit" => Reply::json(429, json!({
                    "type": "error",
                    "error": {"type": "rate_limit_error", "message": format!("Slow down: {KEY}")},
                    "request_id": "req_body_fallback",
                }).to_string()).header("retry-after", "2"),
                "error_html" => Reply::text(429, "text/html", "<h1>Proxy rate limit</h1>"),
                "bad_message" => Reply::json(200, r#"{"type":"message","id":123}"#),
                "bad_media" => Reply::text(200, "text/plain", "Not a JSON message"),
                _ => unreachable!(),
            };
            reply.header("request-id", "req_first_use_error")
        })
        .await;
        let error = within(client(&server).messages().create(request()).send())
            .await
            .unwrap_err();
        assert_eq!(error.request_id(), Some("req_first_use_error"));
        assert!(!format!("{error:?} {error}").contains(KEY));
        match (&error, case) {
            (Error::Api(api), "rate_limit") => {
                assert_eq!(error.status().unwrap().as_u16(), 429);
                assert_eq!(api.error_type(), Some(&ApiErrorType::RateLimit));
                assert_eq!(api.message(), Some(format!("Slow down: {KEY}").as_str()));
                assert_eq!(api.request_id(), Some("req_first_use_error"));
                assert_eq!(api.meta().status().as_u16(), 429);
                assert_eq!(
                    api.retry_after(SystemTime::now()).unwrap(),
                    Some(Duration::from_secs(2))
                );
                assert!(api.failure().document().is_some());
            }
            (Error::Api(api), "error_html") => {
                assert_eq!(error.status().unwrap().as_u16(), 429);
                assert_eq!(api.error_type(), None);
                assert_eq!(api.message(), None);
                assert!(matches!(
                    api.failure().state(),
                    concord::Document::Unrecognized(_)
                ));
            }
            (Error::Concord(inner), "bad_message" | "bad_media") => {
                assert_eq!(error.status().unwrap().as_u16(), 200);
                assert_eq!(
                    inner.kind(),
                    if case == "bad_message" {
                        ErrorKind::Decode
                    } else {
                        ErrorKind::Metadata
                    }
                );
                assert_eq!(inner.delivery(), Delivery::Answered);
                assert!(inner.operation().is_some());
            }
            _ => panic!("unexpected result for {case}: {error:?}"),
        }
        assert_wire(&server.single(), false);
        server.shutdown().await;
    }

    let server = TestServer::start(|_: &Recorded| -> Reply {
        panic!("invalid input must never reach the server")
    })
    .await;
    assert!(MessageRequest::user("", 128, PROMPT).is_err());
    assert!(MessageRequest::user(MODEL, 0, PROMPT).is_err());
    assert!(MessageRequest::new(MODEL, 128, vec![]).is_err());
    let error = within(
        client(&server)
            .set_api_key(format!("{KEY}\n"))
            .messages()
            .create(request())
            .send(),
    )
    .await
    .unwrap_err();
    assert!(!format!("{error:?} {error}").contains(KEY));
    assert_eq!(error.request_id(), None);
    assert_eq!(error.status(), None);
    match error {
        Error::Concord(inner) => {
            assert_eq!(inner.kind(), ErrorKind::Build);
            assert_eq!(inner.delivery(), Delivery::NotSent);
        }
        other => panic!("unexpected invalid-header error: {other:?}"),
    }
    assert!(server.requests().is_empty());
    server.shutdown().await;
}

fn sse(value: Value) -> Vec<u8> {
    format!(
        "event: {}\ndata: {value}\n\n",
        value["type"].as_str().unwrap()
    )
    .into_bytes()
}

// Separate event and transport chunks; even UTF-8 characters may cross chunks.
fn prefix() -> Vec<u8> {
    [
        json!({"type":"message_start", "message":{
            "id":"msg_first_use_stream", "type":"message", "role":"assistant",
            "model":MODEL, "content":[], "stop_reason":null, "stop_sequence":null,
            "usage":{"input_tokens":19,"output_tokens":1}
        }}),
        json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
        json!({"type":"ping"}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"A quiet café"}}),
    ].into_iter().flat_map(sse).collect()
}

fn ending() -> Vec<u8> {
    [
        json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" glows warmly."}}),
        json!({"type":"content_block_stop","index":0}),
        json!({"type":"message_delta","delta":{"stop_reason":null,"stop_sequence":null},"usage":{"output_tokens":4}}),
        json!({"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null},"usage":{"output_tokens":9}}),
    ].into_iter().flat_map(sse).collect()
}

#[tokio::test]
async fn consume_stream_incrementally_and_reject_incomplete_results() {
    // The server cannot send the ending until the caller has consumed the first
    // text delta. This proves incremental consumption without a timing assertion.
    let release = Arc::new(AtomicBool::new(false));
    let server_release = Arc::clone(&release);
    let server = TestServer::start(move |_: &Recorded| -> Reply {
        let mut first = prefix().into_iter();
        let mut last = [ending(), sse(json!({"type":"message_stop"}))]
            .concat()
            .into_iter();
        let release = Arc::clone(&server_release);
        Reply::new(200)
            .header("content-type", "text/event-stream; charset=utf-8")
            .header("request-id", "req_first_use_stream")
            .with_body(Body::Generate(
                Box::new(move || {
                    if let Some(byte) = first.next() {
                        return Some(vec![byte].into());
                    }
                    if !release.load(Ordering::SeqCst) {
                        return Some(sse(json!({"type":"ping"})).into());
                    }
                    last.next().map(|byte| vec![byte].into())
                }),
                Some(Duration::from_millis(1)),
            ))
    })
    .await;
    let response = within(
        client(&server)
            .messages()
            .stream(request().streaming())
            .send(),
    )
    .await
    .unwrap();
    assert_eq!(response.meta.status().as_u16(), 200);
    assert_eq!(response.request_id(), Some("req_first_use_stream"));
    assert!(response.meta.operation().is_some());
    let mut stream = response.value;
    assert_eq!(stream.state(), StreamState::Reading);
    assert_eq!(stream.events_received(), 0);
    assert!(!stream.partial().started());
    assert_eq!(stream.request_id(), Some("req_first_use_stream"));
    let mut text = String::new();
    let mut events = Vec::new();
    let mut stopped = false;
    while let Some(event) = within(stream.next()).await.unwrap() {
        events.push(event.event_type().to_owned());
        if let Some(delta) = event.text_delta() {
            text.push_str(delta);
            assert_eq!(stream.state(), StreamState::Reading);
            assert_eq!(stream.partial().open_blocks(), [0]);
            assert_eq!(stream.partial().text(), text);
            if !release.swap(true, Ordering::SeqCst) {
                assert_eq!(text, "A quiet café");
                assert_eq!(stream.partial().usage().unwrap().output_tokens, Some(1));
            }
        }
        if matches!(
            event,
            StreamEvent::MessageDelta {
                stop_reason: Some(StopReason::EndTurn),
                ..
            }
        ) {
            assert_eq!(stream.state(), StreamState::Reading);
            assert!(stream.partial().open_blocks().is_empty());
            assert_eq!(stream.partial().usage().unwrap().output_tokens, Some(9));
        }
        if matches!(event, StreamEvent::MessageStop) {
            stopped = true;
            assert_eq!(stream.state(), StreamState::Complete);
        }
    }
    assert!(stopped);
    assert!(release.load(Ordering::SeqCst));
    assert_eq!(stream.events_received(), events.len() as u64);
    assert_eq!(
        events
            .iter()
            .filter(|event| event.as_str() != "ping")
            .map(String::as_str)
            .collect::<Vec<_>>(),
        [
            "message_start",
            "content_block_start",
            "content_block_delta",
            "content_block_delta",
            "content_block_stop",
            "message_delta",
            "message_delta",
            "message_stop",
        ]
    );
    assert!(events.iter().any(|event| event == "ping"));
    assert!(within(stream.next()).await.unwrap().is_none());
    let message = within(stream.final_message()).await.unwrap();
    assert_eq!(message.id, "msg_first_use_stream");
    assert_eq!(message.model, MODEL);
    assert_eq!(message.content.len(), 1);
    assert_eq!(message.content[0].as_text(), Some(text.as_str()));
    assert_eq!(text, "A quiet café glows warmly.");
    assert_eq!(message.stop_reason, Some(StopReason::EndTurn));
    let usage = message.usage.unwrap();
    assert_eq!(usage.input_tokens, Some(19));
    assert_eq!(usage.output_tokens, Some(9)); // cumulative, not 1 + 4 + 9
    assert_wire(&server.single(), true);
    server.shutdown().await;

    for case in ["provider", "eof", "invalid_event"] {
        let server = TestServer::start(move |_: &Recorded| -> Reply {
            let tail = match case {
                "provider" => sse(json!({"type":"error","error":{
                    "type":"overloaded_error","message":format!("Overloaded: {KEY}")
                }})),
                "eof" => ending(), // end_turn arrived, but message_stop never does
                "invalid_event" => b"event: content_block_delta\ndata: not-json\n\n".to_vec(),
                _ => unreachable!(),
            };
            let body = [prefix(), tail].concat();
            Reply::new(200)
                .header("content-type", "text/event-stream")
                .header("request-id", "req_first_use_incomplete")
                .with_body(Body::Chunks(
                    body.chunks(7)
                        .map(|chunk| (Duration::ZERO, chunk.to_vec().into()))
                        .collect(),
                ))
        })
        .await;
        let response = within(
            client(&server)
                .messages()
                .stream(request().streaming())
                .send(),
        )
        .await
        .unwrap();
        assert_eq!(response.meta.status().as_u16(), 200);
        let mut stream = response.value;
        let mut text = String::new();
        let mut accepted = 0;
        let error = loop {
            match within(stream.next()).await {
                Ok(Some(event)) => {
                    accepted += 1;
                    if let Some(delta) = event.text_delta() {
                        text.push_str(delta);
                    }
                }
                Ok(None) => panic!("{case} was incorrectly treated as a complete message"),
                Err(error) => break error,
            }
        };
        assert_eq!(stream.state(), StreamState::Failed);
        assert_eq!(error.status(), None); // HTTP 200 does not describe this body failure
        assert_eq!(error.request_id(), Some("req_first_use_incomplete"));
        assert!(!format!("{error:?} {error} {stream:?}").contains(KEY));
        assert!(!format!("{error:?} {error} {stream:?}").contains("A quiet café"));
        let Error::Incomplete(incomplete) = error else {
            panic!("expected incomplete")
        };
        assert_eq!(incomplete.events_received(), accepted);
        assert_eq!(incomplete.partial(), stream.partial());
        assert!(incomplete.partial().started());
        assert_eq!(incomplete.partial().id(), Some("msg_first_use_stream"));
        assert_eq!(incomplete.partial().model(), Some(MODEL));
        assert_eq!(incomplete.partial().text(), text);
        assert_eq!(incomplete.partial().usage().unwrap().input_tokens, Some(19));
        if case == "eof" {
            assert!(matches!(incomplete.cause(), IncompleteCause::EndOfStream));
            assert_eq!(text, "A quiet café glows warmly.");
            assert!(incomplete.partial().open_blocks().is_empty());
            assert_eq!(
                incomplete.partial().stop_reason(),
                Some(&StopReason::EndTurn)
            );
            assert_eq!(incomplete.partial().usage().unwrap().output_tokens, Some(9));
        } else {
            assert_eq!(text, "A quiet café");
            assert_eq!(incomplete.partial().open_blocks(), [0]);
            assert_eq!(incomplete.partial().stop_reason(), None);
            match incomplete.cause() {
                IncompleteCause::ErrorEvent(provider) if case == "provider" => {
                    assert_eq!(provider.error_type(), &ApiErrorType::Overloaded);
                    assert_eq!(provider.message(), format!("Overloaded: {KEY}"));
                }
                IncompleteCause::InvalidEvent { event, .. } if case == "invalid_event" => {
                    assert_eq!(event, "content_block_delta");
                    assert!(incomplete.invalid_event_source().is_some());
                }
                cause => panic!("unexpected cause for {case}: {cause:?}"),
            }
        }
        for error in [
            within(stream.next()).await.unwrap_err(),
            within(stream.final_message()).await.unwrap_err(),
        ] {
            let Error::Incomplete(later) = error else {
                panic!("expected sticky failure")
            };
            assert!(matches!(later.cause(), IncompleteCause::AlreadyFailed));
            assert_eq!(later.partial(), incomplete.partial());
            assert_eq!(later.request_id(), Some("req_first_use_incomplete"));
        }
        assert_wire(&server.single(), true); // no automatic retry or restart
        server.shutdown().await;
    }

    for case in ["status", "media"] {
        let server = TestServer::start(move |_: &Recorded| -> Reply {
            if case == "status" {
                Reply::json(
                    529,
                    json!({"type":"error","error":{
                    "type":"overloaded_error","message":"Temporarily overloaded"
                },"request_id":"req_stream_setup_body"})
                    .to_string(),
                )
            } else {
                Reply::text(200, "application/json", "{}")
                    .header("request-id", "req_stream_wrong_media")
            }
        })
        .await;
        let error = within(
            client(&server)
                .messages()
                .stream(request().streaming())
                .send(),
        )
        .await
        .unwrap_err();
        match error {
            Error::Api(api) if case == "status" => {
                assert_eq!(api.status().as_u16(), 529);
                assert_eq!(api.error_type(), Some(&ApiErrorType::Overloaded));
                assert_eq!(api.request_id(), Some("req_stream_setup_body"));
                assert_eq!(api.retry_after(SystemTime::now()).unwrap(), None);
            }
            Error::Concord(inner) if case == "media" => {
                assert_eq!(inner.kind(), ErrorKind::Metadata);
                assert_eq!(inner.delivery(), Delivery::Answered);
                assert_eq!(inner.response().unwrap().status().as_u16(), 200);
            }
            other => panic!("unexpected setup error: {other:?}"),
        }
        assert_wire(&server.single(), true);
        server.shutdown().await;
    }
}

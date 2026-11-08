//! The two connected workflows end to end against a local server, and the required failure: a
//! midstream error or an end before `message_stop` is incomplete — not a final message, and not
//! re-sent.
//!
//! Expectations come from the cited documentation (see `PROVENANCE.md`): the documented event
//! order, the concatenated deltas, the cumulative `usage` rule, and the hand-assembled
//! non-streaming message `fixtures/message.json`.

mod support;

use std::time::Duration;

use concord::ErrorKind;
use concord::bytes::Bytes;
use concord_example_testserver::{Body, Reply, TestServer, paused};
use concord_sdk_anthropic_messages::{
    ApiErrorType, ContentBlock, Error, IncompleteCause, Message, Role, StopReason, StreamEvent,
    StreamState,
};
use support::{
    api, assert_no_second_request, events, fixture, hello, sse, sse_broken, sse_chunked,
};

fn message_fixture(name: &str) -> Message {
    serde_json::from_str(&fixture(name)).unwrap()
}

fn incomplete(error: Error) -> Box<concord_sdk_anthropic_messages::Incomplete> {
    match error {
        Error::Incomplete(incomplete) => incomplete,
        other => panic!("expected an incomplete stream, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------------------------
// Workflow 1: a non-streaming message with text input; inspect the returned blocks.
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn workflow_1_create_a_message_and_inspect_its_blocks() {
    let server = TestServer::start(|_| {
        Reply::json(200, fixture("message.json")).header("request-id", "req_create_0001")
    })
    .await;

    let created = api(&server)
        .messages()
        .create(hello())
        .send()
        .await
        .unwrap();
    assert_eq!(created.request_id(), Some("req_create_0001"));
    assert_eq!(created.meta.status(), 200);

    let message = created.into_value();
    assert_eq!(message.id, "msg_1nZdL29xx5MUA1yADyHTEsnR8uuvGzszyY");
    assert_eq!(message.kind, "message");
    assert_eq!(message.role, Role::Assistant);
    assert_eq!(message.model, "claude-opus-5-5");
    assert_eq!(message.stop_reason, Some(StopReason::EndTurn));
    assert_eq!(message.stop_sequence, None);
    let usage = message.usage.clone().unwrap();
    assert_eq!(
        (usage.input_tokens, usage.output_tokens),
        (Some(25), Some(15))
    );

    // The blocks, one by one.
    let [block] = message.content.as_slice() else {
        panic!("one block expected: {:?}", message.content)
    };
    assert_eq!(block.block_type(), "text");
    assert_eq!(block.as_text(), Some("Hello!"));
    assert_eq!(message.text(), "Hello!");

    let request = server.single();
    assert_eq!(
        (request.method.as_str(), request.path()),
        ("POST", "/v1/messages")
    );
}

#[tokio::test]
async fn workflow_1_keeps_blocks_it_does_not_interpret() {
    let server = TestServer::start(|_| Reply::json(200, fixture("message-tool-use.json"))).await;
    let message = api(&server)
        .messages()
        .create(hello())
        .send()
        .await
        .unwrap()
        .value;

    assert_eq!(message.stop_reason, Some(StopReason::ToolUse));
    let [text, tool] = message.content.as_slice() else {
        panic!("two blocks expected")
    };
    assert_eq!(
        text.as_text(),
        Some("Okay, let's check the weather for San Francisco, CA:")
    );
    assert_eq!(tool.block_type(), "tool_use");
    assert_eq!(tool.as_text(), None);
    let ContentBlock::Other(tool) = tool else {
        panic!("tool_use is not interpreted")
    };
    assert_eq!(tool.raw()["name"], "get_weather");
    assert_eq!(tool.raw()["input"]["location"], "San Francisco, CA");
    assert!(tool.deltas().is_empty());
}

// ---------------------------------------------------------------------------------------------
// Workflow 2: a streaming message, consumed incrementally through message_stop.
// ---------------------------------------------------------------------------------------------

/// The documented order of the basic stream (streaming page, "Basic streaming request").
const BASIC_ORDER: [&str; 8] = [
    "message_start",
    "content_block_start",
    "ping",
    "content_block_delta",
    "content_block_delta",
    "content_block_stop",
    "message_delta",
    "message_stop",
];

#[tokio::test]
async fn workflow_2_stream_a_message_through_message_stop() {
    let body = fixture("stream-basic.sse");
    // 5-byte chunks: every line and most events are split across reads.
    let server = TestServer::start(move |_| sse_chunked(body.as_bytes(), 5)).await;

    let streamed = api(&server)
        .messages()
        .stream(hello().streaming())
        .send()
        .await
        .unwrap();
    assert_eq!(streamed.request_id(), Some("req_stream_0001"));
    let mut stream = streamed.into_value();
    assert_eq!(stream.request_id(), Some("req_stream_0001"));
    assert_eq!(stream.state(), StreamState::Reading);

    let mut order = Vec::new();
    let mut text = String::new();
    while let Some(event) = stream.next().await.unwrap() {
        order.push(event.event_type().to_owned());
        if let Some(delta) = event.text_delta() {
            text.push_str(delta);
            // Incremental: the partial message already holds what arrived.
            assert_eq!(stream.partial().text(), text);
        }
        if let StreamEvent::MessageDelta { usage, .. } = &event {
            assert_eq!(usage.as_ref().unwrap().output_tokens, Some(15));
        }
    }
    assert_eq!(order, BASIC_ORDER);
    assert_eq!(text, "Hello!");
    assert_eq!(stream.state(), StreamState::Complete);
    assert_eq!(stream.events_received(), 8);
    // `Ok(None)` stays the answer once complete.
    assert!(stream.next().await.unwrap().is_none());

    // The assembled message is the non-streaming one assembled by hand from the same events
    // (input 25 from message_start, output 15 from the cumulative message_delta).
    let message = stream.final_message().await.unwrap();
    assert_eq!(message, message_fixture("message.json"));

    let request = server.single();
    let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
    assert_eq!(body["stream"], true);
}

#[tokio::test]
async fn workflow_2_final_message_reads_the_remaining_events() {
    let body = fixture("stream-basic.sse");
    let server = TestServer::start(move |_| sse(&body)).await;
    let mut stream = api(&server)
        .messages()
        .stream(hello().streaming())
        .send()
        .await
        .unwrap()
        .value;
    // Two events by hand, the rest through `final_message`.
    assert!(matches!(
        stream.next().await.unwrap(),
        Some(StreamEvent::MessageStart(_))
    ));
    assert!(matches!(
        stream.next().await.unwrap(),
        Some(StreamEvent::ContentBlockStart { index: 0, .. })
    ));
    let message = stream.final_message().await.unwrap();
    assert_eq!(message.text(), "Hello!");
}

#[tokio::test]
async fn workflow_2_tool_use_blocks_pass_through_uninterpreted() {
    let body = fixture("stream-tool-use.sse");
    let server = TestServer::start(move |_| sse_chunked(body.as_bytes(), 64)).await;
    let message = api(&server)
        .messages()
        .stream(hello().streaming())
        .send()
        .await
        .unwrap()
        .value
        .final_message()
        .await
        .unwrap();

    assert_eq!(message.id, "msg_014p7gG3wDgGV9EUtLvnow3U");
    assert_eq!(message.stop_reason, Some(StopReason::ToolUse));
    let usage = message.usage.clone().unwrap();
    assert_eq!(
        (usage.input_tokens, usage.output_tokens),
        (Some(472), Some(89))
    );
    assert_eq!(
        message.text(),
        "Okay, let's check the weather for San Francisco, CA:"
    );
    let ContentBlock::Other(tool) = &message.content[1] else {
        panic!("block 1 is the tool_use block")
    };
    assert_eq!(tool.block_type(), "tool_use");
    assert_eq!(tool.raw()["input"], serde_json::json!({})); // as content_block_start sent it
    let partial_json: String = tool
        .deltas()
        .iter()
        .map(|delta| delta["partial_json"].as_str().unwrap())
        .collect();
    assert_eq!(partial_json, r#"{"location": "San Francisco, CA"}"#);
}

#[tokio::test]
async fn workflow_2_thinking_blocks_pass_through_and_usage_may_be_absent() {
    let body = fixture("stream-thinking.sse");
    // 3-byte chunks cut inside the multi-byte `×` of the thinking text.
    let server = TestServer::start(move |_| sse_chunked(body.as_bytes(), 3)).await;
    let message = api(&server)
        .messages()
        .stream(hello().streaming())
        .send()
        .await
        .unwrap()
        .value
        .final_message()
        .await
        .unwrap();

    assert_eq!(message.usage, None); // this documented stream carries no usage
    assert_eq!(message.stop_reason, Some(StopReason::EndTurn));
    assert_eq!(message.content[0].block_type(), "thinking");
    let ContentBlock::Other(thinking) = &message.content[0] else {
        panic!("thinking is not interpreted")
    };
    assert_eq!(thinking.deltas().len(), 5); // four thinking_delta, one signature_delta
    assert_eq!(
        message.text(),
        "The greatest common divisor of 1071 and 462 is **21**."
    );
}

// Paused clock: the response and the reads before the deadline complete in frozen time, then the
// 50 ms deadline and the server's 300 ms pause run on one virtual clock, so the deadline always
// passes first, however slowly the earlier reads were processed.
#[tokio::test(start_paused = true)]
async fn workflow_2_a_per_event_timeout_loses_no_event() {
    // The basic stream with a pause in the middle of the first text delta's data line.
    let body = fixture("stream-basic.sse");
    let cut = body.find("\"Hello\"").unwrap();
    let (head, tail) = body.split_at(cut);
    let chunks = vec![
        (Duration::ZERO, Bytes::copy_from_slice(head.as_bytes())),
        (
            Duration::from_millis(300),
            Bytes::copy_from_slice(tail.as_bytes()),
        ),
    ];
    let server = TestServer::start(move |_| {
        Reply::new(200)
            .header("content-type", "text/event-stream")
            .with_body(Body::Chunks(chunks.clone()))
    })
    .await;
    let messages = api(&server).messages();
    let mut stream = paused::drive(messages.stream(hello().streaming()).send())
        .await
        .unwrap()
        .value;
    for _ in 0..3 {
        paused::drive(stream.next()).await.unwrap().unwrap(); // message_start, content_block_start, ping
    }
    // The delta is half received: the deadline expires and the pending call is dropped.
    let expired = tokio::time::timeout(Duration::from_millis(50), stream.next()).await;
    assert!(expired.is_err());
    assert_eq!(stream.state(), StreamState::Reading);
    // The next call delivers that delta whole, then the rest.
    assert_eq!(
        stream.next().await.unwrap().unwrap().text_delta(),
        Some("Hello")
    );
    assert_eq!(stream.final_message().await.unwrap().text(), "Hello!");
}

// ---------------------------------------------------------------------------------------------
// The required failure: incomplete, not final, not re-sent.
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_midstream_error_event_is_incomplete_and_never_restarted() {
    // The basic stream up to its first text delta, then the documented `error` event.
    let basic = events(&fixture("stream-basic.sse"));
    let body = format!("{}{}", basic[..4].concat(), fixture("error-event.sse"));
    let server = TestServer::start(move |_| sse_chunked(body.as_bytes(), 11)).await;

    let mut stream = api(&server)
        .messages()
        .stream(hello().streaming())
        .send()
        .await
        .unwrap() // the HTTP setup succeeded: 200, text/event-stream
        .value;
    let mut seen = Vec::new();
    let error = loop {
        match stream.next().await {
            Ok(Some(event)) => seen.push(event.event_type().to_owned()),
            Ok(None) => panic!("an error event must not end the stream cleanly"),
            Err(error) => break error,
        }
    };
    assert_eq!(seen, BASIC_ORDER[..4]);
    assert!(error.to_string().contains("incomplete"), "{error}");
    assert_eq!(error.request_id(), Some("req_stream_0001"));

    let incomplete = incomplete(error);
    let IncompleteCause::ErrorEvent(provider) = incomplete.cause() else {
        panic!("{:?}", incomplete.cause())
    };
    assert_eq!(provider.error_type(), &ApiErrorType::Overloaded);
    assert_eq!(provider.message(), "Overloaded");
    assert_eq!(incomplete.events_received(), 4);
    // What arrived is kept, and is visibly not final.
    let partial = incomplete.partial();
    assert_eq!(partial.text(), "Hello");
    assert_eq!(partial.open_blocks(), [0]);
    assert_eq!(partial.stop_reason(), None);

    // The stream stays failed: never `Ok(None)`, which would read as completion.
    assert_eq!(stream.state(), StreamState::Failed);
    let again = incomplete_cause(stream.next().await);
    assert!(matches!(again, IncompleteCause::AlreadyFailed));
    assert!(stream.final_message().await.is_err());

    // Nothing was re-sent.
    assert_no_second_request(&server).await;
}

fn incomplete_cause(
    result: Result<Option<StreamEvent>, Error>,
) -> concord_sdk_anthropic_messages::IncompleteCause {
    incomplete(result.unwrap_err()).into_cause()
}

#[tokio::test]
async fn eof_before_message_stop_is_incomplete_even_with_a_stop_reason() {
    // Every documented event but the final `message_stop`, ending at an event boundary.
    let basic = events(&fixture("stream-basic.sse"));
    let body = basic[..7].concat();
    let server = TestServer::start(move |_| sse(&body)).await;

    let stream = api(&server)
        .messages()
        .stream(hello().streaming())
        .send()
        .await
        .unwrap()
        .value;
    let incomplete = incomplete(stream.final_message().await.unwrap_err());
    assert!(matches!(incomplete.cause(), IncompleteCause::EndOfStream));
    // `message_delta` said `end_turn`, but without `message_stop` the message is not final.
    assert_eq!(
        incomplete.partial().stop_reason(),
        Some(&StopReason::EndTurn)
    );
    assert_eq!(incomplete.partial().text(), "Hello!");
    assert_eq!(incomplete.events_received(), 7);
    assert_no_second_request(&server).await;
}

#[tokio::test]
async fn a_connection_lost_midstream_is_incomplete_with_concords_transport_error() {
    let basic = events(&fixture("stream-basic.sse"));
    let prefix = basic[..5].concat();
    let server = TestServer::start(move |_| sse_broken(prefix.as_bytes())).await;

    let mut stream = api(&server)
        .messages()
        .stream(hello().streaming())
        .send()
        .await
        .unwrap()
        .value;
    for _ in 0..5 {
        stream.next().await.unwrap().unwrap();
    }
    let incomplete = incomplete(stream.next().await.unwrap_err());
    assert_eq!(incomplete.partial().text(), "Hello!");
    let IncompleteCause::Transport(error) = incomplete.into_cause() else {
        panic!("a broken body is a transport failure")
    };
    assert_eq!(error.kind(), ErrorKind::Transport);
    assert_eq!(error.endpoint(), Some("Anthropic.messages.stream"));
    assert_no_second_request(&server).await;
}

#[tokio::test]
async fn a_body_ending_inside_an_event_is_incomplete() {
    let basic = fixture("stream-basic.sse");
    // Cut inside the `message_delta` data line.
    let cut = basic.find("\"end_turn\"").unwrap();
    let body = basic[..cut].to_owned();
    let server = TestServer::start(move |_| sse(&body)).await;
    let stream = api(&server)
        .messages()
        .stream(hello().streaming())
        .send()
        .await
        .unwrap()
        .value;
    let incomplete = incomplete(stream.final_message().await.unwrap_err());
    assert!(
        matches!(incomplete.cause(), IncompleteCause::MalformedStream(_)),
        "{:?}",
        incomplete.cause()
    );
    assert_no_second_request(&server).await;
}

#[tokio::test]
async fn an_error_before_the_stream_starts_is_an_api_error_not_a_stream() {
    let document = support::error_document("overloaded_error", "Overloaded", "req_529");
    let server = TestServer::start(move |_| Reply::json(529, &document)).await;
    let error = api(&server)
        .messages()
        .stream(hello().streaming())
        .send()
        .await
        .unwrap_err();
    let Error::Api(api_error) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(api_error.status(), 529);
    assert_eq!(api_error.error_type(), Some(&ApiErrorType::Overloaded));
    assert_no_second_request(&server).await;
}

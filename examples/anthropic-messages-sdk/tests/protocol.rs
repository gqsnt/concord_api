//! Wire-level checks: request shapes, every documented error class, malformed / truncated /
//! oversized / wrong-media bodies (finite and streamed), the event-flow rules, and redaction.
//!
//! Fixtures and derivations are listed in `PROVENANCE.md`; bodies built here from a fixture say
//! how in a comment.

mod support;

use std::time::{Duration, SystemTime};

use concord::reqwest::{self, StatusCode};
use concord::{Delivery, ErrorKind};
use concord_example_testserver::{Body, Reply, Script, TestServer};
use concord_retry::http::RetryAfterError;
use concord_sdk_anthropic_messages::{
    API_VERSION, Anthropic, AnthropicConfig, ApiError, ApiErrorType, ERROR_BODY_LIMIT, Error,
    IncompleteCause, InputMessage, InvalidRequest, MAX_EVENT_BYTES, MAX_MESSAGE_BYTES,
    MessageRequest, StreamEvent, http_client,
};
use serde_json::json;
use support::{KEY, api, error_document, events, fixture, hello, sse};

fn api_error(error: Error) -> Box<ApiError> {
    match error {
        Error::Api(api) => api,
        other => panic!("expected an API error, got {other:?}"),
    }
}

fn concord_error(error: Error) -> concord::Error {
    match error {
        Error::Concord(error) => error,
        other => panic!("expected a Concord error, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------------------------
// Request shapes
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn create_sends_the_documented_request() {
    let server = TestServer::start(|_| Reply::json(200, fixture("message.json"))).await;
    api(&server)
        .messages()
        .create(hello())
        .send()
        .await
        .unwrap();

    let request = server.single();
    assert_eq!(request.method, "POST");
    assert_eq!(request.target, "/v1/messages");
    assert_eq!(request.header("x-api-key"), Some(KEY));
    assert_eq!(request.header("anthropic-version"), Some("2026-10-21"));
    assert_eq!(request.header("content-type"), Some("application/json"));
    assert_eq!(request.header("accept"), Some("application/json"));
    assert!(!request.chunked);
    let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
    // The cited curl example's fields, and no `stream` field for a non-streaming request.
    assert_eq!(
        body,
        json!({
            "model": "claude-opus-5-5",
            "max_tokens": 256,
            "messages": [{"role": "user", "content": "Hello"}],
        })
    );
}

#[tokio::test]
async fn stream_sends_stream_true_and_asks_for_an_event_stream() {
    let body = fixture("stream-basic.sse");
    let server = TestServer::start(move |_| sse(&body)).await;
    let request = MessageRequest::new(
        "claude-opus-5-5",
        256,
        vec![
            InputMessage::user("Hello"),
            InputMessage::assistant("Hi!"),
            InputMessage::user("Again"),
        ],
    )
    .unwrap();
    let stream = api(&server)
        .messages()
        .stream(request.streaming())
        .send()
        .await
        .unwrap();
    stream.value.final_message().await.unwrap();

    let request = server.single();
    assert_eq!(request.target, "/v1/messages");
    assert_eq!(request.header("accept"), Some("text/event-stream"));
    assert_eq!(request.header("content-type"), Some("application/json"));
    let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
    assert_eq!(
        body,
        json!({
            "model": "claude-opus-5-5",
            "max_tokens": 256,
            "messages": [
                {"role": "user", "content": "Hello"},
                {"role": "assistant", "content": "Hi!"},
                {"role": "user", "content": "Again"},
            ],
            "stream": true,
        })
    );
}

#[tokio::test]
async fn the_version_header_can_be_replaced() {
    let server = TestServer::start(|_| Reply::json(200, fixture("message.json"))).await;
    api(&server)
        .set_anthropic_version("2023-06-01")
        .messages()
        .create(hello())
        .send()
        .await
        .unwrap();
    assert_eq!(
        server.single().header("anthropic-version"),
        Some("2023-06-01")
    );
    assert_eq!(API_VERSION, "2026-10-21");
}

#[test]
fn header_names_are_valid() {
    Anthropic::validate_header_names().unwrap();
}

#[tokio::test]
async fn local_preconditions_refuse_invalid_requests() {
    assert_eq!(
        MessageRequest::new("", 256, vec![InputMessage::user("Hello")]),
        Err(InvalidRequest::EmptyModel)
    );
    assert_eq!(
        MessageRequest::user("claude-opus-5-5", 0, "Hello"),
        Err(InvalidRequest::ZeroMaxTokens)
    );
    let refused = MessageRequest::new("claude-opus-5-5", 256, Vec::new()).unwrap_err();
    assert_eq!(refused, InvalidRequest::NoMessages);
    let error = Error::from(refused);
    assert_eq!(
        error.to_string(),
        "invalid message request: a message request needs at least one input message"
    );
    assert_eq!(error.request_id(), None);
    assert_eq!(error.status(), None);
}

// ---------------------------------------------------------------------------------------------
// Every documented error class (errors page, "HTTP errors"), on both endpoints
// ---------------------------------------------------------------------------------------------

/// The errors page's list: status, `error.type`, and the SDK's variant.
fn documented_errors() -> Vec<(u16, &'static str, ApiErrorType)> {
    vec![
        (400, "invalid_request_error", ApiErrorType::InvalidRequest),
        (401, "authentication_error", ApiErrorType::Authentication),
        (402, "billing_error", ApiErrorType::Billing),
        (403, "permission_error", ApiErrorType::Permission),
        (404, "not_found_error", ApiErrorType::NotFound),
        (409, "conflict_error", ApiErrorType::Conflict),
        (413, "request_too_large", ApiErrorType::RequestTooLarge),
        (429, "rate_limit_error", ApiErrorType::RateLimit),
        (500, "api_error", ApiErrorType::Api),
        // "The API also returns timeout_error with other status codes, such as a 500".
        (500, "timeout_error", ApiErrorType::Timeout),
        (504, "timeout_error", ApiErrorType::Timeout),
        (529, "overloaded_error", ApiErrorType::Overloaded),
        // A type added later is kept, not refused.
        (
            400,
            "a_future_error",
            ApiErrorType::Other("a_future_error".into()),
        ),
    ]
}

fn error_replies() -> Script {
    Script::new(
        documented_errors()
            .into_iter()
            .map(|(status, error_type, _)| {
                // The errors page's document, with this class's type (synthetic message and id).
                let body = error_document(
                    error_type,
                    &format!("synthetic {error_type}"),
                    &format!("req_body_{status}"),
                );
                Reply::json(status, body).header("request-id", format!("req_head_{status}"))
            })
            .collect::<Vec<_>>(),
    )
}

fn check_documented(errors: Vec<Error>) {
    let expected = documented_errors();
    assert_eq!(errors.len(), expected.len());
    for (error, (status, error_type, variant)) in errors.into_iter().zip(expected) {
        assert_eq!(error.status(), Some(StatusCode::from_u16(status).unwrap()));
        assert_eq!(
            error.request_id(),
            Some(format!("req_head_{status}").as_str())
        );
        let shown = error.to_string();
        assert!(
            shown.contains(&status.to_string()) && shown.contains(error_type),
            "{shown}"
        );
        let api = api_error(error);
        assert_eq!(api.error_type(), Some(&variant));
        assert_eq!(api.error_type().unwrap().as_str(), error_type);
        assert_eq!(
            api.message(),
            Some(format!("synthetic {error_type}").as_str())
        );
        assert!(api.document_error().is_none());
    }
}

#[tokio::test]
async fn every_documented_error_class_on_create() {
    let script = error_replies();
    let server = TestServer::start(move |_| script.next()).await;
    let api = api(&server);
    let mut errors = Vec::new();
    for _ in documented_errors() {
        errors.push(api.messages().create(hello()).send().await.unwrap_err());
    }
    check_documented(errors);
    assert_eq!(server.requests().len(), documented_errors().len()); // one request each
}

#[tokio::test]
async fn every_documented_error_class_before_a_stream_starts() {
    let script = error_replies();
    let server = TestServer::start(move |_| script.next()).await;
    let api = api(&server);
    let mut errors = Vec::new();
    for _ in documented_errors() {
        errors.push(
            api.messages()
                .stream(hello().streaming())
                .send()
                .await
                .unwrap_err(),
        );
    }
    check_documented(errors);
}

#[tokio::test]
async fn the_documented_error_shape_verbatim() {
    // `fixtures/error-not-found.json` as published, without a `request-id` header: the
    // document's own `request_id` is used.
    let server = TestServer::start(|_| Reply::json(404, fixture("error-not-found.json"))).await;
    let error = api(&server)
        .messages()
        .create(hello())
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.request_id(), Some("req_011CSHoEeqs5C35K2UUqR7Fy"));
    let api = api_error(error);
    assert_eq!(api.error_type(), Some(&ApiErrorType::NotFound));
    assert_eq!(
        api.message(),
        Some("The requested resource could not be found.")
    );
}

#[tokio::test]
async fn retry_after_is_read_but_nothing_is_retried() {
    // 2027-01-15T08:00:00Z, the wall clock the HTTP date below is judged against.
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000);
    let body = error_document("rate_limit_error", "synthetic", "req_1");
    let script = Script::new([
        Reply::json(429, &body).header("retry-after", "30"),
        // "A tier spend-cap 429 has no retry-after header".
        Reply::json(429, &body),
        Reply::json(429, &body).header("retry-after", "soon"),
        Reply::json(429, &body)
            .header("retry-after", "1")
            .header("retry-after", "2"),
        Reply::json(
            529,
            error_document("overloaded_error", "Overloaded", "req_2"),
        )
        .header("retry-after", "Fri, 15 Jan 2027 08:02:00 GMT"),
    ]);
    let server = TestServer::start(move |_| script.next()).await;
    let api = api(&server);
    let mut delays = Vec::new();
    for _ in 0..5 {
        let error = api_error(api.messages().create(hello()).send().await.unwrap_err());
        delays.push(error.retry_after(now));
    }
    assert_eq!(delays[0], Ok(Some(Duration::from_secs(30))));
    assert_eq!(delays[1], Ok(None));
    assert_eq!(delays[2], Err(RetryAfterError::Invalid));
    assert_eq!(delays[3], Err(RetryAfterError::Repeated));
    assert_eq!(delays[4], Ok(Some(Duration::from_secs(120))));
    assert_eq!(server.requests().len(), 5); // five calls, five requests: no automatic retry
}

// ---------------------------------------------------------------------------------------------
// Error documents that cannot be read: the status stays authoritative
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn unreadable_error_documents_keep_the_status() {
    let oversized = format!(
        r#"{{"type":"error","error":{{"type":"invalid_request_error","message":"{}"}}}}"#,
        "x".repeat(ERROR_BODY_LIMIT)
    );
    let script = Script::new([
        // A proxy's HTML page (wrong media type).
        Reply::text(502, "text/html", "<html>Bad gateway</html>"),
        // No Content-Type at all.
        Reply::new(500).body(r#"{"type":"error"}"#),
        // Labelled JSON, not JSON.
        Reply::json(400, "{oops"),
        // JSON without the documented `error` object.
        Reply::json(400, r#"{"type":"error","request_id":"req_x"}"#),
        // Longer than the bound.
        Reply::json(400, oversized),
        // Cut off mid-document (the connection drops).
        Reply::new(500)
            .header("content-type", "application/json")
            .with_body(Body::FailAfter(vec![r#"{"type":"error","err"#.into()])),
    ]);
    let server = TestServer::start(move |_| script.next()).await;
    let api = api(&server);
    let expected = [
        (502, ErrorKind::Metadata),
        (500, ErrorKind::Metadata),
        (400, ErrorKind::Decode),
        (400, ErrorKind::Decode),
        (400, ErrorKind::Limit),
        (500, ErrorKind::Transport),
    ];
    for (status, kind) in expected {
        let error = api_error(api.messages().create(hello()).send().await.unwrap_err());
        assert_eq!(error.status(), status);
        assert_eq!(error.error_type(), None);
        assert_eq!(error.document_error().map(concord::Error::kind), Some(kind));
        assert!(error.to_string().contains("unreadable"), "{error}");
    }
}

// ---------------------------------------------------------------------------------------------
// 2xx bodies that are not a message
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn success_bodies_that_are_not_messages() {
    let oversized = format!(
        r#"{{"id":"m","type":"message","role":"assistant","model":"m","content":[{{"type":"text","text":"{}"}}]}}"#,
        "x".repeat(MAX_MESSAGE_BYTES)
    );
    let script = Script::new([
        Reply::text(200, "text/html", "<html>login</html>").header("request-id", "req_html"),
        Reply::new(200).body(fixture("message.json")),
        Reply::json(200, "{\"id\":"),
        Reply::json(
            200,
            r#"{"id":"m","type":"message","role":"assistant","model":"m"}"#,
        ),
        Reply::json(
            200,
            r#"{"id":"m","type":"message","role":"assistant","model":"m","content":[{"type":"text"}]}"#,
        ),
        Reply::json(200, oversized),
        Reply::new(200)
            .header("content-type", "application/json")
            .with_body(Body::FailAfter(vec![r#"{"id":"msg_"#.into()])),
    ]);
    let server = TestServer::start(move |_| script.next()).await;
    let api = api(&server);
    let expected = [
        ErrorKind::Metadata,  // wrong media type, refused before the body is read
        ErrorKind::Metadata,  // no Content-Type
        ErrorKind::Decode,    // truncated JSON
        ErrorKind::Decode,    // no `content`
        ErrorKind::Decode,    // a text block without text
        ErrorKind::Limit,     // longer than MAX_MESSAGE_BYTES
        ErrorKind::Transport, // the connection drops mid-body
    ];
    let mut errors = Vec::new();
    for kind in expected {
        let error = api.messages().create(hello()).send().await.unwrap_err();
        assert_eq!(error.status(), Some(StatusCode::OK), "{error:?}");
        let error = concord_error(error);
        assert_eq!(error.kind(), kind, "{error:?}");
        assert_eq!(error.delivery(), Delivery::Answered);
        assert_eq!(error.endpoint(), Some("Anthropic.messages.create"));
        errors.push(error);
    }
    // A refused head keeps its request id.
    assert_eq!(
        Error::Concord(errors.remove(0)).request_id(),
        Some("req_html")
    );
}

#[tokio::test]
async fn the_stream_endpoint_requires_an_event_stream_head() {
    let script = Script::new([
        Reply::json(200, fixture("message.json")),
        Reply::new(200).body(fixture("stream-basic.sse")),
        Reply::text(
            200,
            "text/event-stream; charset=iso-8859-1",
            "event: ping\n\n",
        ),
    ]);
    let server = TestServer::start(move |_| script.next()).await;
    let api = api(&server);
    for _ in 0..3 {
        let error = api
            .messages()
            .stream(hello().streaming())
            .send()
            .await
            .unwrap_err();
        let error = concord_error(error);
        assert_eq!(error.kind(), ErrorKind::Metadata);
        assert!(error.response().is_some(), "the response is kept unread");
    }
    // Accepted: the media type with a UTF-8 charset.
    let accepted = TestServer::start(|_| {
        Reply::text(
            200,
            "text/event-stream; charset=utf-8",
            fixture("stream-basic.sse"),
        )
    })
    .await;
    let stream = support::api(&accepted)
        .messages()
        .stream(hello().streaming())
        .send()
        .await
        .unwrap();
    stream.value.final_message().await.unwrap();
}

// ---------------------------------------------------------------------------------------------
// Streamed bodies: malformed, truncated, oversized, and the documented flow
// ---------------------------------------------------------------------------------------------

/// Streams `body` and returns how it ended before `message_stop`.
async fn stream_failure(body: impl Into<Vec<u8>>) -> IncompleteCause {
    let body = body.into();
    let server = TestServer::start(move |_| sse(&body)).await;
    let stream = api(&server)
        .messages()
        .stream(hello().streaming())
        .send()
        .await
        .unwrap()
        .value;
    match stream.final_message().await {
        Err(Error::Incomplete(incomplete)) => incomplete.into_cause(),
        other => panic!("expected an incomplete stream, got {other:?}"),
    }
}

/// The basic stream's events with event `index` replaced by `replacement` (synthetic).
fn basic_with(index: usize, replacement: &str) -> String {
    let mut basic = events(&fixture("stream-basic.sse"));
    basic[index] = replacement.to_owned();
    basic.concat()
}

fn protocol(cause: IncompleteCause) -> &'static str {
    match cause {
        IncompleteCause::Protocol(rule) => rule,
        other => panic!("expected a protocol violation, got {other:?}"),
    }
}

#[tokio::test]
async fn the_documented_flow_is_enforced() {
    let basic = events(&fixture("stream-basic.sse"));
    let cases: Vec<(String, &str)> = vec![
        // A delta before any message_start.
        (basic[3..].concat(), "an event before message_start"),
        // The SSE name and the data's type differ.
        (
            basic_with(2, "event: pong\ndata: {\"type\": \"ping\"}\n\n"),
            "the SSE event name differs from the data's type",
        ),
        // An event without a name is a `message` event: also a mismatch.
        (
            basic_with(2, "data: {\"type\": \"ping\"}\n\n"),
            "the SSE event name differs from the data's type",
        ),
        // A second message_start.
        (basic_with(2, &basic[0]), "a second message_start"),
        // A block that skips an index.
        (
            basic_with(
                1,
                "event: content_block_start\ndata: {\"type\": \"content_block_start\", \"index\": 1, \"content_block\": {\"type\": \"text\", \"text\": \"\"}}\n\n",
            ),
            "content_block_start out of order",
        ),
        // message_delta while block 0 is still open (its stop removed).
        (
            [&basic[..5], &basic[6..]].concat().concat(),
            "message_delta while a content block is open",
        ),
        // message_stop while block 0 is still open (its stop and the message_delta removed).
        (
            [&basic[..5], &basic[7..]].concat().concat(),
            "message_stop while a content block is open",
        ),
        // message_stop without any message_delta.
        (
            [&basic[..6], &basic[7..]].concat().concat(),
            "message_stop before any message_delta",
        ),
        // A text delta for a tool_use block.
        (
            basic_with(
                1,
                "event: content_block_start\ndata: {\"type\": \"content_block_start\", \"index\": 0, \"content_block\": {\"type\": \"tool_use\", \"id\": \"t\", \"name\": \"n\", \"input\": {}}}\n\n",
            ),
            "text_delta for a block that is not text",
        ),
        // Data without a type.
        (
            basic_with(2, "event: ping\ndata: {}\n\n"),
            "event data without a string type",
        ),
        // A delta after its block stopped (the first delta repeated after the stop).
        (
            [&basic[..6], &basic[3..4], &basic[6..]].concat().concat(),
            "content_block_delta for a block that is not open",
        ),
        // A block stopped twice.
        (
            [&basic[..6], &basic[5..]].concat().concat(),
            "content_block_stop for a block that is not open",
        ),
        // message_start must carry empty content.
        (
            basic_with(
                0,
                &basic[0].replace(
                    r#""content": []"#,
                    r#""content": [{"type": "text", "text": "x"}]"#,
                ),
            ),
            "message_start carries content",
        ),
    ];
    for (body, rule) in cases {
        assert_eq!(protocol(stream_failure(body).await), rule);
    }
}

#[tokio::test]
async fn malformed_and_oversized_events_end_the_stream() {
    // Data that is not JSON.
    let not_json = basic_with(3, "event: content_block_delta\ndata: {\"text\": \"Hel\n\n");
    let cause = stream_failure(not_json).await;
    let IncompleteCause::InvalidEvent { event, .. } = &cause else {
        panic!("{cause:?}")
    };
    assert_eq!(event, "content_block_delta");

    // A known event missing a required field.
    let no_index = basic_with(
        5,
        "event: content_block_stop\ndata: {\"type\": \"content_block_stop\"}\n\n",
    );
    assert!(matches!(
        stream_failure(no_index).await,
        IncompleteCause::InvalidEvent { .. }
    ));

    // A field of the wrong type: serde's error quotes the data, so it is withheld from
    // Display and Debug and reachable only on purpose.
    let quoted = basic_with(
        5,
        "event: content_block_stop\ndata: {\"type\": \"content_block_stop\", \"index\": \"zz-quoted\"}\n\n",
    );
    let server = TestServer::start(move |_| sse(&quoted)).await;
    let stream = api(&server)
        .messages()
        .stream(hello().streaming())
        .send()
        .await
        .unwrap()
        .value;
    let error = stream.final_message().await.unwrap_err();
    assert!(
        !error.to_string().contains("zz-quoted") && !format!("{error:?}").contains("zz-quoted")
    );
    let Error::Incomplete(incomplete) = error else {
        panic!("incomplete expected")
    };
    let source = incomplete.invalid_event_source().unwrap().to_string();
    assert!(source.contains("zz-quoted"), "{source}");

    // Invalid UTF-8 inside an event.
    let mut invalid = basic_with(2, "event: ping\ndata: \u{1}\n\n").into_bytes();
    let at = invalid.iter().position(|b| *b == 1).unwrap();
    invalid[at] = 0xff;
    assert!(matches!(
        stream_failure(invalid).await,
        IncompleteCause::MalformedStream(_)
    ));

    // One event larger than MAX_EVENT_BYTES: refused, never truncated.
    let huge = format!(
        "event: content_block_delta\ndata: {{\"type\": \"content_block_delta\", \"index\": 0, \"delta\": {{\"type\": \"text_delta\", \"text\": \"{}\"}}}}\n\n",
        "x".repeat(MAX_EVENT_BYTES)
    );
    assert!(matches!(
        stream_failure(basic_with(3, &huge)).await,
        IncompleteCause::EventTooLarge(_)
    ));

    // An empty 200 body: no message_start, no message_stop.
    assert!(matches!(
        stream_failure("").await,
        IncompleteCause::EndOfStream
    ));
}

#[tokio::test]
async fn a_documented_stream_without_its_final_blank_line_is_incomplete() {
    // The tool-use stream exactly as the page's code block shows it: it ends at the last `data`
    // line, without the blank line that dispatches an event. Concord's reader refuses to guess.
    let fixture = fixture("stream-tool-use.sse");
    let verbatim = fixture.strip_suffix('\n').unwrap().to_owned();
    assert!(matches!(
        stream_failure(verbatim).await,
        IncompleteCause::MalformedStream(_)
    ));
}

#[tokio::test]
async fn unknown_event_types_pass_through() {
    // The basic stream with an event of a type this SDK does not know (synthetic) after
    // content_block_start.
    let mut basic = events(&fixture("stream-basic.sse"));
    let unknown = "event: future_event\ndata: {\"type\": \"future_event\", \"x\": 1}\n\n";
    basic.insert(2, unknown.to_owned());
    let body = basic.concat();
    let server = TestServer::start(move |_| sse(&body)).await;
    let mut stream = api(&server)
        .messages()
        .stream(hello().streaming())
        .send()
        .await
        .unwrap()
        .value;
    let mut seen = Vec::new();
    while let Some(event) = stream.next().await.unwrap() {
        if let StreamEvent::Unknown { event_type, data } = &event {
            assert_eq!(
                (event_type.as_str(), &data["x"]),
                ("future_event", &json!(1))
            );
        }
        seen.push(event.event_type().to_owned());
    }
    assert_eq!(seen[2], "future_event");
    assert_eq!(stream.final_message().await.unwrap().text(), "Hello!");
}

// ---------------------------------------------------------------------------------------------
// Transport policy and redaction
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn no_answer_is_a_transport_error_whose_delivery_is_unknown() {
    // A refused connection: a port nobody listens on any more.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let closed = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let refused = Anthropic::new(
        http_client().unwrap(),
        AnthropicConfig {
            api_key: KEY.into(),
        },
    )
    .with_base_url(closed)
    .unwrap();
    // A server that reads the request and hangs up without answering.
    let server = TestServer::start(|_| Reply::hang_up()).await;
    for error in [
        refused.messages().create(hello()).send().await.unwrap_err(),
        api(&server)
            .messages()
            .stream(hello().streaming())
            .send()
            .await
            .unwrap_err(),
    ] {
        assert_eq!((error.status(), error.request_id()), (None, None));
        assert!(!error.to_string().contains(KEY));
        let error = concord_error(error);
        assert_eq!(error.kind(), ErrorKind::Transport);
        // The request may have reached the server: the SDK reports it and does not resend.
        assert_eq!(error.delivery(), Delivery::Unknown);
    }
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn the_documented_transport_does_not_carry_the_key_through_a_redirect() {
    let elsewhere = TestServer::start(|_| Reply::json(200, fixture("message.json"))).await;
    // Another origin (another port).
    let target = format!("{}/v1/messages", elsewhere.url());
    let redirecting =
        TestServer::start(move |_| Reply::new(307).header("location", target.clone())).await;

    // `http_client()`: the 307 is the answer; the other host receives nothing.
    let error = api(&redirecting)
        .messages()
        .create(hello())
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.status(), Some(StatusCode::TEMPORARY_REDIRECT));
    assert!(elsewhere.requests().is_empty());

    // The control: reqwest's default client follows it and sends `x-api-key` to the other host.
    let default = Anthropic::new(
        reqwest::Client::new(),
        AnthropicConfig {
            api_key: KEY.into(),
        },
    )
    .with_base_url(redirecting.url())
    .unwrap();
    default.messages().create(hello()).send().await.unwrap();
    assert_eq!(elsewhere.single().header("x-api-key"), Some(KEY));
}

#[tokio::test]
async fn the_key_and_provider_text_are_never_printed() {
    let api_client = Anthropic::new(
        http_client().unwrap(),
        AnthropicConfig {
            api_key: KEY.into(),
        },
    );
    assert!(!format!("{api_client:?}").contains(KEY));
    let prepared = api_client.messages().create(hello()).prepare().unwrap();
    assert!(prepared.request().headers()["x-api-key"].is_sensitive());
    let shown = format!("{prepared:?}");
    assert!(
        shown.contains("x-api-key: <redacted>") && !shown.contains(KEY),
        "{shown}"
    );

    // A key that cannot be a header value: refused before sending, and not echoed.
    let broken = format!("{KEY}\n");
    let refused = Anthropic::new(
        http_client().unwrap(),
        AnthropicConfig {
            api_key: broken.clone(),
        },
    )
    .messages()
    .create(hello())
    .send()
    .await
    .unwrap_err();
    assert!(!refused.to_string().contains(KEY) && !format!("{refused:?}").contains(KEY));
    let refused = concord_error(refused);
    assert_eq!(refused.kind(), ErrorKind::Build);
    assert_eq!(refused.delivery(), Delivery::NotSent);

    // An error document that echoes the key: the message is reachable, never printed.
    let echo = error_document(
        "authentication_error",
        &format!("invalid x-api-key {KEY}"),
        "req_401",
    );
    let server = TestServer::start(move |_| Reply::json(401, &echo)).await;
    let error = api(&server)
        .messages()
        .create(hello())
        .send()
        .await
        .unwrap_err();
    assert!(!error.to_string().contains(KEY), "{error}");
    assert!(!format!("{error:?}").contains(KEY), "{error:?}");
    assert!(api_error(error).message().unwrap().contains(KEY));

    // An incomplete stream never prints generated text or the error event's message.
    let basic = events(&fixture("stream-basic.sse"));
    let error_event = fixture("error-event.sse").replace("\"Overloaded\"", "\"zz-provider-text\"");
    let body = format!("{}{}", basic[..4].concat(), error_event);
    let server = TestServer::start(move |_| sse(&body)).await;
    let mut stream = api(&server)
        .messages()
        .stream(hello().streaming())
        .send()
        .await
        .unwrap()
        .value;
    let error = loop {
        if let Err(error) = stream.next().await {
            break error;
        }
    };
    for shown in [
        error.to_string(),
        format!("{error:?}"),
        format!("{stream:?}"),
    ] {
        assert!(
            !shown.contains("Hello") && !shown.contains("zz-provider-text"),
            "{shown}"
        );
    }
}

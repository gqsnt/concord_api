//! Server-sent events through real generated and runtime response terminals.

use std::time::Duration;

use concord::ApiError;
use concord::api;
use concord::endpoint::{EndpointId, RuntimeEndpoint};
use concord::error::{DispatchKnowledge, ErrorCategory};
use concord::http::{Method, StatusCode};
use concord::records::{EventStream, ItemStream, ServerEvent};
use concord::response::{
    BodyDisposition, Buffered, ResponseRepresentation, StreamErrorKind, StreamState, Streaming,
};
use concord_test_support::server::{ResponseScript, ScriptedServer};

use crate::support::loopback_proxy;

api! {
    client SseResponseApi {
        base "http://sse-response.example.test/v1/";
        timeout 30s;

        GET sse_events() -> Vec<ServerEvent> via EventStream { expose accepted_bytes; }
        GET sse_stream() -> ItemStream<ServerEvent> via EventStream;
    }
}

fn client(server: &ScriptedServer) -> SseResponseApi {
    let proxy = loopback_proxy(server);
    SseResponseApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .expect("SSE response client")
}

fn response(body: impl AsRef<[u8]>) -> ResponseScript {
    ResponseScript::ok(body.as_ref().to_vec()).with_header("content-type", "text/event-stream")
}

fn one_byte(body: &[u8]) -> ResponseScript {
    ResponseScript::Chunked {
        status: 200,
        headers: vec![("content-type".into(), "text/event-stream".into())],
        chunks: body.iter().map(|byte| vec![*byte]).collect(),
    }
}

async fn collect(mut stream: ItemStream<ServerEvent>) -> Vec<ServerEvent> {
    assert_eq!(stream.summary().state(), StreamState::Active);
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event.expect("complete SSE event"));
    }
    let summary = stream.summary();
    assert_eq!(summary.state(), StreamState::Eof);
    assert_eq!(summary.body_disposition(), BodyDisposition::Complete);
    assert_eq!(summary.delivered_records(), events.len());
    assert!(stream.next().await.is_none());
    assert_eq!(stream.finish(), summary);
    events
}

fn assert_event(
    event: &ServerEvent,
    event_type: &str,
    data: &str,
    last_event_id: &str,
    retry_ms: Option<u64>,
) {
    assert_eq!(event.event(), event_type);
    assert_eq!(event.data(), data);
    assert_eq!(event.last_event_id(), last_event_id);
    assert_eq!(event.retry(), retry_ms.map(Duration::from_millis));
}

fn assert_redacted(error: &ApiError, forbidden: &[&str]) {
    let mut rendered = format!("{error} {error:?}");
    let mut source = std::error::Error::source(error);
    while let Some(error) = source {
        rendered.push_str(&format!(" {error} {error:?}"));
        source = error.source();
    }
    for value in forbidden {
        assert!(!rendered.contains(value), "error exposed a response value");
    }
}

async fn assert_terminal_frame_error(mut stream: ItemStream<ServerEvent>) -> ApiError {
    let opened = stream.summary();
    let first = stream
        .next()
        .await
        .expect("first event is dispatched")
        .expect("first event is valid");
    assert_eq!(first.data(), "before");
    let error = stream
        .next()
        .await
        .expect("frame failure is yielded")
        .expect_err("oversized frame is withheld");
    assert_eq!(error.category(), ErrorCategory::ResponseBody);
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    let failed = error.stream_summary().expect("correlated failure summary");
    assert_eq!(failed.state(), StreamState::Failed);
    assert_eq!(failed.error_kind(), Some(StreamErrorKind::FrameLimit));
    assert_eq!(failed.status(), StatusCode::OK);
    assert_eq!(failed.operation_id(), opened.operation_id());
    assert_eq!(failed.attempt(), opened.attempt());
    assert_eq!(failed.endpoint_id(), opened.endpoint_id());
    assert_eq!(failed.delivered_records(), 1);
    assert_eq!(&stream.summary(), failed);
    assert!(stream.next().await.is_none());
    assert!(stream.next().await.is_none());
    assert_eq!(&stream.finish(), failed);
    error
}

#[test]
fn sse_supports_buffered_and_managed_response_pairs() {
    fn buffered<S: ResponseRepresentation<Vec<ServerEvent>, Driver = Buffered>>() {}
    fn streamed<S: ResponseRepresentation<ItemStream<ServerEvent>, Driver = Streaming>>() {}

    buffered::<EventStream>();
    streamed::<EventStream>();
    assert_eq!(
        <EventStream as ResponseRepresentation<Vec<ServerEvent>>>::ACCEPT,
        Some("text/event-stream")
    );
    assert_eq!(
        <EventStream as ResponseRepresentation<ItemStream<ServerEvent>>>::ACCEPT,
        Some("text/event-stream")
    );
    let contract = SseResponseApi::contract();
    let endpoint = contract
        .endpoint("sse_response_api.sse_stream")
        .expect("SSE stream endpoint");
    assert_eq!(
        endpoint.result().driver(),
        concord::endpoint::ResponseAcquisition::Streaming
    );
    assert!(!endpoint.terminals().pagination());
}

#[tokio::test]
async fn buffered_and_one_byte_sse_preserve_event_state_and_line_semantics() {
    const BODY: &str = concat!(
        "\u{feff}: keepalive\r\n",
        "id: initial\r\nretry: 00150\r\nunknown: ignored\r\n\r\n",
        "event: ignored-without-data\r\n\r\n",
        "event: stale\r\nevent: update\r\ndata: café ☃\r\n",
        "data:  keeps:colon\r\nid: current\r\n\r\n",
        "data\r\n\r\n",
        "event:\ndata: after empty type\nid: bad\0id\n",
        "retry: -3\nretry: 1.5\nretry: 18446744073709551616\nretry: １２\n\n",
        "id\nretry: 0\ndata: zero\n\n",
        "event: lone-cr\rdata: final complete\r\r",
        "data: unfinished event\nlast partial line"
    );
    let server = ScriptedServer::start([
        response(BODY.as_bytes()),
        one_byte(BODY.as_bytes()),
        one_byte(BODY.as_bytes()),
        response(BODY.as_bytes()),
    ])
    .expect("SSE delivery server");
    let client = client(&server);

    let direct = client.sse_events().await.expect("buffered SSE");
    let direct_split = client.sse_events().await.expect("one-byte buffered SSE");
    let streamed = collect(client.sse_stream().await.expect("one-byte SSE stream")).await;
    let whole = collect(client.sse_stream().await.expect("whole-body SSE stream")).await;
    assert_eq!(direct, direct_split);
    assert_eq!(direct, streamed);
    assert_eq!(direct, whole);
    assert_eq!(direct.len(), 5);
    assert_event(
        &direct[0],
        "update",
        "café ☃\n keeps:colon",
        "current",
        Some(150),
    );
    assert_event(&direct[1], "message", "", "current", Some(150));
    assert_event(
        &direct[2],
        "message",
        "after empty type",
        "current",
        Some(150),
    );
    assert_event(&direct[3], "message", "zero", "", Some(0));
    assert_event(&direct[4], "lone-cr", "final complete", "", Some(0));

    let requests = server.requests();
    assert_eq!(requests.len(), 4);
    for request in requests {
        assert_eq!(request.header("accept"), Some("text/event-stream"));
        assert_eq!(request.header("last-event-id"), None);
    }
}

#[tokio::test]
async fn sse_replaces_invalid_utf8_and_ignores_only_the_initial_bom() {
    const BODY: &[u8] = b"\xef\xbb\xbfdata: initial\n\n\xef\xbb\xbfdata: ignored\n\ndata: bad-\xff-\xe2\x82\n\ndata: \xef\xbb\xbfretained\n\n";
    let server = ScriptedServer::start([response(BODY), one_byte(BODY)]).expect("SSE UTF-8 server");
    let client = client(&server);
    let direct = client.sse_events().await.expect("lossy UTF-8 buffered SSE");
    let streamed = collect(client.sse_stream().await.expect("lossy UTF-8 SSE stream")).await;
    assert_eq!(direct, streamed);
    assert_eq!(direct.len(), 3);
    assert_event(&direct[0], "message", "initial", "", None);
    assert_event(&direct[1], "message", "bad-�-�", "", None);
    assert_event(&direct[2], "message", "\u{feff}retained", "", None);
}

#[tokio::test]
async fn sse_discards_every_unfinished_event_at_eof() {
    let tails: &[&[u8]] = &[
        b"",
        b": keepalive\n\n",
        b"id: metadata-only\nretry: 5\nevent: ignored\n\n",
        b"data: no line ending",
        b"data: no blank line\n",
        b"data: no blank line\r\n",
        b"data:\n",
        b"data: first line\ndata: partial second line",
    ];
    let prefix = b"data: complete\n\n";
    let scripts = tails.iter().flat_map(|tail| {
        let body = [prefix.as_slice(), *tail].concat();
        [response(body.clone()), one_byte(&body)]
    });
    let server = ScriptedServer::start(scripts).expect("SSE EOF server");
    let client = client(&server);
    for tail in tails {
        let direct = client.sse_events().await.expect("buffered EOF policy");
        let streamed = collect(client.sse_stream().await.expect("EOF policy stream")).await;
        assert_eq!(direct, streamed, "EOF tail: {tail:?}");
        assert_eq!(direct.len(), 1, "EOF tail: {tail:?}");
        assert_event(&direct[0], "message", "complete", "", None);
    }
    assert_eq!(server.request_count(), tails.len() * 2);
}

#[tokio::test]
async fn sse_validates_response_media_before_stream_handoff() {
    let valid = [
        "text/event-stream",
        "TeXt/EvEnT-StReAm; ChArSeT=\"UTF-8\"; version=1",
        "text/event-stream; charset=iso-8859-1",
    ];
    let invalid = [
        None,
        Some("text/plain"),
        Some("application/json"),
        Some("text/event-streaming"),
        Some("text/event-stream+json"),
        Some("text/event-stream; charset"),
        Some("text/event-stream; value=\"unterminated"),
    ];
    let mut scripts = Vec::new();
    for media in valid {
        let script =
            ResponseScript::ok(b"data: accepted\n\n".to_vec()).with_header("content-type", media);
        scripts.extend([script.clone(), script]);
    }
    for media in invalid {
        let script = ResponseScript::ok(b"media-sse-secret".to_vec());
        let script = match media {
            Some(media) => script.with_header("content-type", media),
            None => script,
        };
        scripts.extend([script.clone(), script]);
    }
    let server = ScriptedServer::start(scripts).expect("SSE response media server");
    let client = client(&server);
    for media in valid {
        let direct = client.sse_events().await.expect(media);
        let streamed = collect(client.sse_stream().await.expect(media)).await;
        assert_eq!(direct, streamed);
        assert_eq!(direct.len(), 1);
    }
    for media in invalid {
        let direct = client.sse_events().await.expect_err("SSE media mismatch");
        let streamed = client.sse_stream().await.expect_err("SSE media mismatch");
        for error in [direct, streamed] {
            assert_eq!(
                error.category(),
                ErrorCategory::ResponseContract,
                "{media:?}"
            );
            assert_eq!(
                error.dispatch_knowledge(),
                DispatchKnowledge::ResponseObserved
            );
            assert!(error.stream_summary().is_none());
            assert_redacted(&error, &["media-sse-secret"]);
        }
    }
    assert_eq!(server.request_count(), 2 * (valid.len() + invalid.len()));
    assert!(
        server
            .requests()
            .iter()
            .all(|request| request.header("accept") == Some("text/event-stream"))
    );
}

#[tokio::test]
async fn sse_line_and_accumulated_data_limits_are_terminal_and_redacted() {
    let prefix = "data: before\n\n";
    let oversized_line = format!(
        "{prefix}data: line-sse-secret{}\n\ndata: after-failure\n\n",
        "x".repeat(1024 * 1024)
    );
    let oversized_event = format!(
        "{prefix}data: cumulative-sse-secret\n{}\ndata: after-failure\n\n",
        format!("data: {}\n", "x".repeat(1024)).repeat(1024)
    );
    let server = ScriptedServer::start([
        response(oversized_line.as_bytes()),
        response(oversized_line.as_bytes()),
        response(oversized_event.as_bytes()),
        response(oversized_event.as_bytes()),
    ])
    .expect("SSE frame failure server");
    let client = client(&server);
    let repeated_payload = "x".repeat(64);
    let forbidden = [
        "line-sse-secret",
        "cumulative-sse-secret",
        "after-failure",
        repeated_payload.as_str(),
    ];
    for _ in 0..2 {
        let direct = client
            .sse_events()
            .await
            .expect_err("buffered SSE frame limit");
        assert_eq!(direct.category(), ErrorCategory::ResponseDecoding);
        assert_redacted(&direct, &forbidden);
        let stream = client
            .sse_stream()
            .await
            .expect("SSE stream opens before oversized event");
        assert!(!format!("{stream:?}").contains("sse-secret"));
        let error = assert_terminal_frame_error(stream).await;
        assert_redacted(&error, &forbidden);
    }
    assert_eq!(server.request_count(), 4);
}

#[tokio::test]
async fn generated_and_runtime_sse_consume_keepalives_under_a_small_staging_bound() {
    let body = format!(
        "{}data: first\n\n{}data: second\n\n",
        ": keepalive\n".repeat(256),
        "ignored: x\n".repeat(256)
    );
    let overflow =
        b"data: before\n\ndata: runtime-staging-secret-too-long\n\ndata: after-failure\n\n";
    let server = ScriptedServer::start([
        response(body.as_bytes()),
        response(body.as_bytes()),
        response(body.as_bytes()),
        response(overflow),
        ResponseScript::ok(b"raw-sse-decoder-bypass".to_vec())
            .with_header("content-type", "application/octet-stream"),
    ])
    .expect("SSE bounded staging server");
    let proxy = loopback_proxy(&server);
    let client = SseResponseApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(|runtime| runtime.stream_frame_limit(16).stream_max_records(2))
        .build()
        .expect("SSE client with small staging bound");
    let direct = client.sse_events().await.expect("complete SSE keepalives");
    let streamed = collect(client.sse_stream().await.expect("bounded SSE stream")).await;
    assert_eq!(direct, streamed);
    assert_eq!(direct.len(), 2);
    assert_event(&direct[0], "message", "first", "", None);
    assert_event(&direct[1], "message", "second", "", None);

    let endpoint = RuntimeEndpoint::builder(
        EndpointId::try_from("runtime.sse_stream").expect("runtime SSE endpoint ID"),
        Method::GET,
    )
    .response::<EventStream, ItemStream<ServerEvent>>()
    .build()
    .expect("runtime SSE response");
    let runtime = collect(
        client
            .bind(endpoint)
            .expect("runtime SSE binding")
            .await
            .expect("runtime SSE stream"),
    )
    .await;
    assert_eq!(runtime, direct);

    let error = assert_terminal_frame_error(
        client
            .sse_stream()
            .await
            .expect("SSE overflow stream opens"),
    )
    .await;
    assert_redacted(&error, &["runtime-staging-secret", "after-failure"]);
    let raw = client
        .sse_events()
        .accepted_bytes()
        .await
        .expect("SSE accepted bytes bypass");
    assert_eq!(raw.value(), b"raw-sse-decoder-bypass".as_slice());
    assert_eq!(server.request_count(), 5);
    assert!(
        server
            .requests()
            .iter()
            .all(|request| request.header("accept") == Some("text/event-stream"))
    );
}

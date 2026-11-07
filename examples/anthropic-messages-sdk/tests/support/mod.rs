//! Shared test support: fixtures, a client pointed at a local server, SSE replies.
#![allow(dead_code, missing_docs)]

use std::time::Duration;

use concord::bytes::Bytes;
use concord_example_testserver::{Body, Reply, TestServer};
use concord_sdk_anthropic_messages::{Anthropic, AnthropicConfig, MessageRequest, http_client};

/// A synthetic key: never a real credential.
pub const KEY: &str = "sk-ant-synthetic-0000-key";

pub fn fixture(name: &str) -> String {
    let path = format!("{}/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// The client of the documented transport, pointed at `server`.
pub fn api(server: &TestServer) -> Anthropic {
    Anthropic::new(
        http_client().unwrap(),
        AnthropicConfig {
            api_key: KEY.into(),
        },
    )
    .with_base_url(server.url())
    .unwrap()
}

/// The request of the cited curl examples: `claude-opus-5-5`, 256 tokens, one user turn "Hello".
pub fn hello() -> MessageRequest {
    MessageRequest::user("claude-opus-5-5", 256, "Hello").unwrap()
}

/// A `text/event-stream` 200 with a request id, the whole body at once.
pub fn sse(body: impl AsRef<[u8]>) -> Reply {
    Reply::text(200, "text/event-stream", body).header("request-id", "req_stream_0001")
}

/// The same, delivered in chunks of `size` bytes (cuts fall anywhere: inside lines, inside
/// multi-byte characters, between CR and LF).
pub fn sse_chunked(body: &[u8], size: usize) -> Reply {
    let chunks = body
        .chunks(size)
        .map(|chunk| (Duration::ZERO, Bytes::copy_from_slice(chunk)))
        .collect();
    Reply::new(200)
        .header("content-type", "text/event-stream")
        .header("request-id", "req_stream_0001")
        .with_body(Body::Chunks(chunks))
}

/// A 200 event stream whose connection is dropped after `prefix`.
pub fn sse_broken(prefix: &[u8]) -> Reply {
    Reply::new(200)
        .header("content-type", "text/event-stream")
        .header("request-id", "req_stream_0001")
        .with_body(Body::FailAfter(vec![Bytes::copy_from_slice(prefix)]))
}

/// The events of an SSE fixture, each with its terminating blank line.
pub fn events(stream: &str) -> Vec<String> {
    stream.split_inclusive("\n\n").map(str::to_owned).collect()
}

/// An error document of the documented shape (the errors page's example) with another type,
/// message and request id.
pub fn error_document(error_type: &str, message: &str, request_id: &str) -> String {
    let mut document: serde_json::Value =
        serde_json::from_str(&fixture("error-not-found.json")).unwrap();
    document["error"]["type"] = error_type.into();
    document["error"]["message"] = message.into();
    document["request_id"] = request_id.into();
    document.to_string()
}

/// Asserts that no further connection reaches `server` within a short bound (nothing re-sent).
pub async fn assert_no_second_request(server: &TestServer) {
    let resent = server
        .wait_until(Duration::from_millis(150), |s| s.requests().len() > 1)
        .await;
    assert!(!resent, "the SDK sent the request again");
    assert_eq!(server.requests().len(), 1);
}

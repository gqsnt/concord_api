//! Shared test support: fixtures, clients pointed at loopback servers, SSE replies.

#![allow(dead_code)]

use std::time::Duration;

use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use concord_sdk_mastodon::concord::bytes::Bytes;
use concord_sdk_mastodon::concord::reqwest;
use concord_sdk_mastodon::{AccessToken, InstanceUrl, Mastodon};

/// A synthetic token: it must never appear in any `Debug` or `Display` output.
pub const TOKEN: &str = "SYNTHETICTOKEN-not-real-0123";

/// The `Authorization` value the token produces.
pub const BEARER: &str = "Bearer SYNTHETICTOKEN-not-real-0123";

/// A fixture file's bytes.
pub fn fixture(name: &str) -> Vec<u8> {
    let path = format!("{}/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// A fixture file's text.
pub fn fixture_text(name: &str) -> String {
    String::from_utf8(fixture(name)).expect("fixtures are UTF-8")
}

/// A reqwest client that never consults a system proxy (the servers are on loopback).
pub fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("a client")
}

/// The instance a test server stands in for.
pub fn instance(server: &TestServer) -> InstanceUrl {
    InstanceUrl::parse(&server.url()).expect("a loopback instance URL")
}

/// An anonymous client for the server.
pub fn anonymous(server: &TestServer) -> Mastodon {
    Mastodon::new(http(), instance(server))
}

/// A client with the synthetic token.
pub fn authorized(server: &TestServer) -> Mastodon {
    anonymous(server).with_token(&AccessToken::new(TOKEN).expect("a valid token"))
}

/// A JSON reply as Mastodon labels it.
pub fn json(status: u16, body: impl AsRef<[u8]>) -> Reply {
    Reply::json(status, body)
}

/// A `text/event-stream` reply delivering `chunks` in order (chunked encoding, one frame each),
/// then the end of the body.
pub fn sse(chunks: Vec<Vec<u8>>) -> Reply {
    sse_with(Body::Chunks(
        chunks
            .into_iter()
            .map(|chunk| (Duration::ZERO, Bytes::from(chunk)))
            .collect(),
    ))
}

/// A `text/event-stream` reply with any body.
pub fn sse_with(body: Body) -> Reply {
    Reply::new(200)
        .header("content-type", "text/event-stream")
        .with_body(body)
}

/// `bytes` cut at each offset in `cuts` (ascending).
pub fn cut(bytes: &[u8], cuts: &[usize]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut start = 0;
    for &at in cuts {
        out.push(bytes[start..at].to_vec());
        start = at;
    }
    out.push(bytes[start..].to_vec());
    out
}

/// The `Authorization` values a request carried.
pub fn authorization(request: &Recorded) -> Vec<&str> {
    request.header_all("authorization")
}

/// A Status JSON document with this id (synthetic, the shape of the documented examples).
pub fn status_json(id: &str, content: &str) -> String {
    serde_json::json!({
        "id": id,
        "created_at": "2026-11-07T00:00:00.000Z",
        "in_reply_to_id": null,
        "in_reply_to_account_id": null,
        "sensitive": false,
        "spoiler_text": "",
        "visibility": "public",
        "language": null,
        "uri": format!("https://mastodon.example/users/synthetic/statuses/{id}"),
        "url": null,
        "edited_at": null,
        "content": content,
        "reblog": null,
        "account": {
            "id": "900003",
            "username": "synthetic",
            "acct": "synthetic",
            "display_name": "",
            "url": "https://mastodon.example/@synthetic"
        },
        "media_attachments": []
    })
    .to_string()
}

/// An SSE frame: `event: <name>`, `data: <data>`, a blank line.
pub fn frame(event: &str, data: &str) -> Vec<u8> {
    format!("event: {event}\ndata: {data}\n\n").into_bytes()
}

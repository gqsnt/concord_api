//! Shared test helpers: a loopback server answering a script of replies, fixtures, and NDJSON
//! framing checks.
#![allow(dead_code)]

use concord_example_testserver::{Recorded, Reply, Script, TestServer};
use concord_sdk_elasticsearch::concord::{self, reqwest};
use concord_sdk_elasticsearch::{ApiKey, Elasticsearch};
use serde_json::Value;

/// A synthetic encoded API key (Base64 of `synthetic-id:synthetic-key`).
pub const KEY: &str = "c3ludGhldGljLWlkOnN5bnRoZXRpYy1rZXk=";

/// The PIT id of the pagination guide's examples.
pub const GUIDE_PIT_ID: &str = "46ToAwMDaWR5BXV1aWQyKwZub2RlXzMAAAAAAAAAACoBYwADaWR4BXV1aWQxAgZub2RlXzEAAAAAAAAAAAEBYQADaWR5BXV1aWQyKgZub2RlXzIAAAAAAAAAAAwBYgACBXV1aWQyAAAFdXVpZDEAAQltYXRjaF9hbGw_gAAAAA==";

/// A transport for writes: no automatic retry, no redirects, no proxy.
pub fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .build()
        .unwrap()
}

/// A server answering `replies` in order, and a client pointed at it.
pub async fn serve(replies: Vec<Reply>) -> (TestServer, Elasticsearch) {
    let script = Script::new(replies);
    let server = TestServer::start(move |_: &Recorded| script.next()).await;
    let es = Elasticsearch::new(http(), &server.url(), ApiKey::new(KEY).unwrap()).unwrap();
    (server, es)
}

pub fn fixture(name: &str) -> Vec<u8> {
    let path = format!("{}/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

pub fn fixture_json(name: &str) -> Value {
    serde_json::from_slice(&fixture(name)).unwrap()
}

/// A JSON reply (`application/json; charset=utf-8`) with a fixture body.
pub fn json(status: u16, name: &str) -> Reply {
    Reply::json(status, fixture(name))
}

/// The NDJSON framing the bulk reference requires, checked on the received bytes: every line
/// (the last included) ends with `\n`, no line is empty or pretty printed, each line is one JSON
/// value. Returns the parsed lines.
pub fn ndjson_lines(body: &[u8]) -> Vec<Value> {
    assert!(
        body.ends_with(b"\n"),
        "the final line must end with a newline"
    );
    let text = std::str::from_utf8(body).unwrap();
    let lines: Vec<&str> = text[..text.len() - 1].split('\n').collect();
    lines
        .iter()
        .map(|line| {
            assert!(!line.is_empty(), "empty NDJSON line");
            assert!(!line.contains('\r'), "carriage return in {line}");
            assert!(!line.starts_with(' '), "indented line {line}");
            serde_json::from_str(line).unwrap()
        })
        .collect()
}

/// A JSON request body, parsed.
pub fn body_json(request: &Recorded) -> Value {
    serde_json::from_slice(&request.body).unwrap()
}

/// The guide's next-page search request (`fixtures/pit-search-request-next.json`) with the PIT id
/// and `search_after` the documented rules require for this page (`None`: the first page).
pub fn guide_search_request(pit_id: &str, search_after: Option<Value>) -> Value {
    let mut request = fixture_json("pit-search-request-next.json");
    request["pit"]["id"] = Value::from(pit_id);
    match search_after {
        Some(after) => request["search_after"] = after,
        None => {
            request.as_object_mut().unwrap().remove("search_after");
        }
    }
    request
}

/// The kind of a Concord error inside the SDK error.
pub fn concord_kind(error: &concord_sdk_elasticsearch::Error) -> Option<concord::ErrorKind> {
    error.concord().map(concord::Error::kind)
}

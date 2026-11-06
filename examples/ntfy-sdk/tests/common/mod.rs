//! Test support shared by `protocol.rs` and `workflows.rs`: fixtures, a client pointed at the
//! loopback server, chunking helpers and a stateful in-memory fake ntfy server.
//!
//! The fake is a deterministic model written for these tests, not the real server: its clock,
//! ids, auth rule and replay rules are synthetic choices stated below.

#![allow(dead_code)]

use std::collections::VecDeque;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use concord::bytes::Bytes;
use concord::reqwest;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use concord_sdk_ntfy::{AccessToken, Ntfy, NtfyTopic, Topic};
use serde_json::{Value, json};

pub const SSE_DOCUMENTED: &[u8] = include_bytes!("../../fixtures/sse_documented.txt");
pub const SSE_PERTURBED: &[u8] = include_bytes!("../../fixtures/sse_perturbed.txt");
pub const SSE_CRLF_BOM: &[u8] = include_bytes!("../../fixtures/sse_crlf_bom.txt");
pub const JSON_DOCUMENTED: &[u8] = include_bytes!("../../fixtures/json_documented.ndjson");
pub const PUBLISH_RESPONSE: &[u8] = include_bytes!("../../fixtures/publish_response.json");
pub const ERROR_FORBIDDEN: &[u8] = include_bytes!("../../fixtures/error_forbidden.json");
pub const ERROR_UNAUTHORIZED: &[u8] = include_bytes!("../../fixtures/error_unauthorized.json");
pub const ERROR_RATE_LIMITED: &[u8] = include_bytes!("../../fixtures/error_rate_limited.json");

/// What ntfy sends with each mode (`Content-Type` plus `; charset=utf-8`, from the server source).
pub const SSE_TYPE: &str = "text/event-stream; charset=utf-8";
pub const NDJSON_TYPE: &str = "application/x-ndjson; charset=utf-8";

/// A synthetic token: not a real credential and not ntfy's `tk_` shape.
pub const TOKEN: &str = "synthetic-token-not-real";

pub fn token() -> AccessToken {
    AccessToken::new(TOKEN).unwrap()
}

pub fn client(server: &TestServer) -> Ntfy {
    Ntfy::new(reqwest::Client::new())
        .with_base_url(server.url())
        .unwrap()
}

pub fn alerts(server: &TestServer) -> NtfyTopic {
    client(server).topic(Topic::new("alerts").unwrap())
}

pub fn alerts_with_token(server: &TestServer) -> NtfyTopic {
    client(server)
        .with_token(&token())
        .topic(Topic::new("alerts").unwrap())
}

/// `bytes` cut at the given offsets, delivered as separate chunks.
pub fn cut(bytes: &[u8], at: &[usize]) -> Body {
    let mut chunks = Vec::new();
    let mut start = 0;
    for &end in at {
        chunks.push((Duration::ZERO, Bytes::copy_from_slice(&bytes[start..end])));
        start = end;
    }
    chunks.push((Duration::ZERO, Bytes::copy_from_slice(&bytes[start..])));
    Body::Chunks(chunks)
}

/// One byte per chunk.
pub fn bytewise(bytes: &[u8]) -> Body {
    Body::Chunks(
        bytes
            .iter()
            .map(|b| (Duration::ZERO, Bytes::copy_from_slice(&[*b])))
            .collect(),
    )
}

/// The offset just after the first occurrence of `needle`.
pub fn after(bytes: &[u8], needle: &[u8]) -> usize {
    bytes
        .windows(needle.len())
        .position(|w| w == needle)
        .expect("needle in fixture")
        + needle.len()
}

/// A body that sends `first` (as separate chunks), then a `keepalive` event every `every`, until
/// the client goes away (a write then fails and the server's connection task ends).
pub fn then_keepalives(first: Vec<Bytes>, every: Duration) -> Body {
    let mut first: VecDeque<Bytes> = first.into();
    let mut n = 0u64;
    Body::Generate(
        Box::new(move || {
            Some(first.pop_front().unwrap_or_else(|| {
                n += 1;
                sse_bytes(&keepalive(n))
            }))
        }),
        Some(every),
    )
}

pub fn keepalive(n: u64) -> Value {
    json!({"id": format!("synKa{n:07}"), "time": 1_767_300_000 + n, "event": "keepalive", "topic": "alerts"})
}

/// One SSE event as ntfy writes it: `event:` for kinds other than `message`, then the JSON line.
pub fn sse_bytes(message: &Value) -> Bytes {
    let line = serde_json::to_string(message).unwrap();
    let event = message["event"].as_str().unwrap_or("message");
    if event == "message" {
        Bytes::from(format!("data: {line}\n\n"))
    } else {
        Bytes::from(format!("event: {event}\ndata: {line}\n\n"))
    }
}

/// The state of the fake server.
#[derive(Default)]
struct State {
    /// Synthetic clock, Unix seconds; every event advances it by one.
    clock: u64,
    next_id: u64,
    /// Cached messages, oldest first (`Cache: no` messages are never stored).
    cache: Vec<Value>,
    /// Live SSE subscribers; a send fails once the connection is gone.
    subscribers: Vec<Sender<Value>>,
}

impl State {
    fn tick(&mut self) -> u64 {
        self.clock += 1;
        1_767_225_600 + self.clock
    }

    fn id(&mut self, prefix: &str) -> String {
        self.next_id += 1;
        format!("{prefix}{:07}", self.next_id)
    }
}

/// A stateful fake of one protected topic (`alerts`), for the workflow tests.
///
/// - Auth: every request needs `Authorization: Bearer <TOKEN>`; none is `401` (`40101`), another
///   value `403` (`40301`), with ntfy's error documents.
/// - `POST /alerts`: stores the body as a `message` (title, priority, tags from `X-*`), unless
///   `X-Cache: no`; pushes it to live subscribers; answers the message as JSON.
/// - `GET /alerts/sse`: `open`, then live messages as SSE, with a `keepalive` every 5 idle polls
///   (polls every 10 ms) so a closed connection is noticed.
/// - `GET /alerts/json?poll=1&since=..`: the cached messages selected by `since`, as NDJSON cut
///   mid-line into chunks. `all`/absent: all; `latest`: the last; an id: those **after** it; a
///   number: those whose `time` is **at or after** it (inclusive: a message of that second comes
///   again — the duplicate the workflow must deduplicate itself).
#[derive(Clone, Default)]
pub struct FakeNtfy(Arc<Mutex<State>>);

impl FakeNtfy {
    pub async fn start(&self) -> TestServer {
        let fake = self.clone();
        TestServer::start(move |request| fake.respond(request)).await
    }

    /// The number of cached messages.
    pub fn cached(&self) -> usize {
        self.0.lock().unwrap().cache.len()
    }

    fn respond(&self, request: &Recorded) -> Reply {
        match request.header("authorization") {
            None => return Reply::json(401, ERROR_UNAUTHORIZED),
            Some(value) if value != format!("Bearer {TOKEN}") => {
                return Reply::json(403, ERROR_FORBIDDEN);
            }
            Some(_) => {}
        }
        match (request.method.as_str(), request.path()) {
            ("POST", "/alerts") => self.publish(request),
            ("GET", "/alerts/sse") => self.subscribe(),
            ("GET", "/alerts/json") if request.query_all("poll") == ["1"] => {
                self.poll(request.query_all("since").first().map(String::as_str))
            }
            _ => Reply::json(404, r#"{"code":40401,"http":404,"error":"page not found"}"#),
        }
    }

    fn publish(&self, request: &Recorded) -> Reply {
        let mut state = self.0.lock().unwrap();
        let time = state.tick();
        let id = state.id("synPub");
        let cache = request.header("x-cache") != Some("no");
        let mut message = json!({"id": id, "time": time, "event": "message", "topic": "alerts"});
        if cache {
            message["expires"] = json!(time + 43_200);
        }
        if let Some(title) = request.header("x-title") {
            message["title"] = json!(title);
        }
        message["message"] = json!(request.body_text());
        if let Some(tags) = request.header("x-tags") {
            message["tags"] = json!(tags.split(',').collect::<Vec<_>>());
        }
        if let Some(priority) = request.header("x-priority") {
            message["priority"] = json!(priority.parse::<u8>().unwrap());
        }
        if cache {
            state.cache.push(message.clone());
        }
        state
            .subscribers
            .retain(|subscriber| subscriber.send(message.clone()).is_ok());
        let mut body = serde_json::to_vec(&message).unwrap();
        body.push(b'\n');
        Reply::json(200, body)
    }

    fn subscribe(&self) -> Reply {
        let (sender, receiver): (Sender<Value>, Receiver<Value>) = mpsc::channel();
        let open = {
            let mut state = self.0.lock().unwrap();
            state.subscribers.push(sender);
            let time = state.tick();
            let id = state.id("synOpen");
            json!({"id": id, "time": time, "event": "open", "topic": "alerts"})
        };
        let mut open = Some(open);
        let mut idle = 0u64;
        let state = self.0.clone();
        let generator = Box::new(move || {
            if let Some(open) = open.take() {
                return Some(sse_bytes(&open));
            }
            match receiver.try_recv() {
                Ok(message) => Some(sse_bytes(&message)),
                Err(TryRecvError::Empty) => {
                    idle += 1;
                    if !idle.is_multiple_of(5) {
                        return Some(Bytes::new()); // nothing written
                    }
                    let mut state = state.lock().unwrap();
                    let time = state.tick();
                    let id = state.id("synKeep");
                    Some(sse_bytes(
                        &json!({"id": id, "time": time, "event": "keepalive", "topic": "alerts"}),
                    ))
                }
                Err(TryRecvError::Disconnected) => None,
            }
        });
        Reply::new(200)
            .header("content-type", SSE_TYPE)
            .with_body(Body::Generate(generator, Some(Duration::from_millis(10))))
    }

    fn poll(&self, since: Option<&str>) -> Reply {
        let state = self.0.lock().unwrap();
        let selected: Vec<&Value> = match since {
            None | Some("all") => state.cache.iter().collect(),
            Some("latest") => state.cache.last().into_iter().collect(),
            Some(text) if text.bytes().all(|b| b.is_ascii_digit()) => {
                let from: u64 = text.parse().unwrap();
                state
                    .cache
                    .iter()
                    .filter(|m| m["time"].as_u64().unwrap() >= from)
                    .collect()
            }
            Some(id) => match state.cache.iter().position(|m| m["id"] == id) {
                Some(at) => state.cache[at + 1..].iter().collect(),
                None => Vec::new(),
            },
        };
        let mut body = Vec::new();
        for message in selected {
            serde_json::to_writer(&mut body, message).unwrap();
            body.push(b'\n');
        }
        // Cut every 37 bytes, so lines and the multi-byte characters inside them are split.
        let chunks = body
            .chunks(37)
            .map(|c| (Duration::ZERO, Bytes::copy_from_slice(c)))
            .collect();
        Reply::new(200)
            .header("content-type", NDJSON_TYPE)
            .with_body(Body::Chunks(chunks))
    }
}

//! Independent application consumer. Protocol provenance and first-use trace: FIRST_USE.md.

use std::collections::HashSet;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use concord::reqwest;
use concord::stream::{Completion, ReadErrorKind};
use concord_example_testserver::{Body, Recorded, Reply, Script, TestServer};
use concord_sdk_ntfy::{
    AccessToken, Cache, Event, EventId, EventKind, Events, MessageText, Mode, Ntfy, NtfyTopic,
    Priority, ReplayCursor, Since, Tags, Topic,
};
use serde_json::{Value, json};

const DEADLINE: Duration = Duration::from_secs(3);
const TOKEN: &str = "synthetic-first-use-not-a-real-token";
const LIVE: &[u8] = include_bytes!("../fixtures/first-use/live.sse");
const FORBIDDEN: &[u8] = include_bytes!("../fixtures/first-use/forbidden.json");
const BANDWIDTH: &[u8] = include_bytes!("../fixtures/first-use/bandwidth.json");
const PUBLISH_TEXT: &str = include_str!("../fixtures/first-use/publish.txt");
const REPLAY: &[u8] = include_bytes!("../fixtures/first-use/replay.jsonl");

fn alerts(server: &TestServer) -> NtfyTopic {
    let http = reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(DEADLINE)
        .build()
        .unwrap();
    concord::BaseUrl::parse((server.url()).as_ref())
        .map(|origin| Ntfy::new(http, concord_sdk_ntfy::NtfyConfig { origin }))
        .unwrap()
        .with_token(&AccessToken::new(TOKEN).unwrap())
        .topic(Topic::new("first-use").unwrap())
}

async fn next_event(events: &mut Events) -> Event {
    tokio::time::timeout(DEADLINE, events.next())
        .await
        .expect("event deadline")
        .expect("expected an event")
        .expect("valid event")
}

fn check_wire(request: &Recorded, method: &str, path: &str) {
    assert_eq!(request.method, method);
    assert_eq!(request.path(), path);
    assert_eq!(
        request.header("authorization"),
        Some(format!("Bearer {TOKEN}").as_str())
    );
    if method == "GET" {
        assert!(request.body.is_empty());
    }
}

fn message_frame() -> &'static [u8] {
    let start = LIVE
        .windows(b"data: {\"id\":\"message001\"".len())
        .position(|b| b == b"data: {\"id\":\"message001\"")
        .unwrap();
    let end = start + LIVE[start..].windows(2).position(|b| b == b"\n\n").unwrap() + 2;
    &LIVE[start..end]
}

#[tokio::test]
async fn sse_cancellation_preserves_partial_event_and_drop_closes_connection() {
    // Deliberately split a UTF-8 code point. The fake holds the tail until the caller resumes.
    let split = LIVE
        .windows("🦀".len())
        .position(|b| b == "🦀".as_bytes())
        .unwrap()
        + 1;
    let release = Arc::new(AtomicBool::new(false));
    let gate = Arc::clone(&release);
    let server = TestServer::start(move |_| {
        let gate = Arc::clone(&gate);
        let mut stage = 0;
        Reply::new(200)
            .header("content-type", "text/event-stream; charset=utf-8")
            .with_body(Body::Generate(
                Box::new(move || {
                    let bytes = match stage {
                        0 => {
                            stage = 1;
                            LIVE[..split].to_vec()
                        }
                        1 if gate.load(Ordering::SeqCst) => {
                            stage = 2;
                            LIVE[split..].to_vec()
                        }
                        1 => Vec::new(), // a waiting provider must not insert bytes inside the payload
                        _ => b": synthetic heartbeat\n\n".to_vec(),
                    };
                    Some(bytes.into())
                }),
                Some(Duration::from_millis(2)),
            ))
    })
    .await;
    let mut events = tokio::time::timeout(DEADLINE, alerts(&server).subscribe_sse().send())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(events.mode(), Mode::Sse);
    let mut cursor = ReplayCursor::new();
    assert!(cursor.since().is_none());

    let open = next_event(&mut events).await;
    assert_eq!(open.kind, EventKind::Open);
    cursor.observe(&open);
    assert!(matches!(cursor.since(), Some(Since::Time(1_700_000_000))));
    let message = next_event(&mut events).await;
    assert_eq!(message.message.as_deref(), Some("Backup ready"));
    assert_eq!(message.kind, EventKind::Message); // no SSE event: field
    cursor.observe(&message);
    let keepalive = next_event(&mut events).await;
    assert_eq!(keepalive.kind, EventKind::Keepalive);
    cursor.observe(&keepalive);
    assert_eq!(cursor.last_message().unwrap().as_str(), "message001");

    let (cancel, stop) = tokio::sync::oneshot::channel();
    let cancellation = async {
        assert!(
            server
                .wait_until(DEADLINE, |s| s.body_bytes_sent() >= split as u64)
                .await
        );
        cancel.send(()).unwrap();
    };
    let pending_read = async {
        tokio::select! {
            biased;
            unexpected = events.next() => panic!("partial event should wait: {unexpected:?}"),
            result = stop => result.unwrap(),
        }
    };
    tokio::time::timeout(DEADLINE, async {
        tokio::join!(pending_read, cancellation);
    })
    .await
    .unwrap();
    assert_eq!(events.completion(), Completion::Reading);
    assert_eq!(events.position().records, 3);
    assert!(events.position().bytes >= split as u64);
    release.store(true, Ordering::SeqCst);
    let last = next_event(&mut events).await;
    assert_eq!(last.id.as_str(), "message002");
    assert_eq!(last.message.as_deref(), Some("Backup done 🦀"));
    cursor.observe(&last);
    assert_eq!(events.position().records, 4);
    assert_eq!(events.completion(), Completion::Reading);
    drop(events);
    assert!(
        server
            .wait_until(DEADLINE, |s| s.live_connections() == 0)
            .await,
        "reader drop must disconnect the live writer"
    );
    let request = server.single();
    check_wire(&request, "GET", "/first-use/sse");
    assert!(request.query_pairs().is_empty());
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn sse_early_clean_end_reconnects_with_message_cursor_under_application_budget() {
    let script = Script::new([
        Reply::text(200, "text/event-stream", message_frame()),
        Reply::text(
            200,
            "text/event-stream",
            &LIVE[LIVE
                .windows(b"data: {\"id\":\"message002\"".len())
                .position(|b| b == b"data: {\"id\":\"message002\"")
                .unwrap()..],
        ),
    ]);
    let replies = script.clone();
    let server = TestServer::start(move |_| replies.next()).await;
    let topic = alerts(&server);
    let mut cursor = ReplayCursor::new();
    let mut delivered = Vec::new();
    // A clean EOF is a lost live subscription; this caller permits exactly one reconnect.
    tokio::time::timeout(DEADLINE, async {
        for _ in 0..2 {
            let mut events = topic
                .subscribe_sse()
                .maybe_since(cursor.since())
                .send()
                .await
                .unwrap();
            while let Some(event) = events.next().await {
                let event = event.unwrap();
                cursor.observe(&event);
                if event.is_message() {
                    delivered.push(event.id);
                }
            }
            assert_eq!(events.completion(), Completion::EndOfInput);
        }
    })
    .await
    .unwrap();
    assert_eq!(
        delivered.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
        ["message001", "message002"]
    );
    assert_eq!(cursor.last_message().unwrap().as_str(), "message002");
    assert_eq!(script.remaining(), 0);
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    for request in &requests {
        check_wire(request, "GET", "/first-use/sse");
    }
    assert!(requests[0].query_pairs().is_empty());
    assert_eq!(
        requests[1].query_pairs(),
        [("since".into(), "message001".into())]
    );
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn sse_terminal_failures_keep_delivered_prefix_and_resumable_cursor() {
    let mut unfinished = message_frame().to_vec();
    unfinished.pop(); // a clean HTTP EOF before the SSE blank-line delimiter
    let cases = [
        (ReadErrorKind::Format, Some(unfinished)),
        (
            ReadErrorKind::Format,
            Some(include_bytes!("../fixtures/first-use/malformed.sse").to_vec()),
        ),
        (
            ReadErrorKind::Decode,
            Some(include_bytes!("../fixtures/first-use/missing-fields.sse").to_vec()),
        ),
        (ReadErrorKind::Transport, None),
    ];
    for (kind, tail) in cases {
        let body = if let Some(tail) = tail {
            Body::Full([message_frame(), tail.as_slice()].concat().into())
        } else {
            Body::FailAfter(vec![message_frame().to_vec().into()])
        };
        let script = Script::new([Reply::new(200)
            .header("content-type", "text/event-stream")
            .with_body(body)]);
        let server = TestServer::start(move |_| script.next()).await;
        let mut events = alerts(&server).subscribe_sse().send().await.unwrap();
        let mut cursor = ReplayCursor::new();
        cursor.observe(&next_event(&mut events).await);
        let failure = tokio::time::timeout(DEADLINE, events.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap_err();
        assert_eq!(failure.kind(), kind);
        assert_eq!(failure.position().records, 1);
        assert!(failure.position().bytes >= message_frame().len() as u64);
        assert!(failure.operation().is_some());
        assert_eq!(cursor.last_message().unwrap().as_str(), "message001");
        assert_eq!(events.completion(), Completion::Failed);
        assert!(events.next().await.is_none());
        assert!(events.next().await.is_none());
        assert_eq!(events.completion(), Completion::Failed);
        let display = format!("{failure} {failure:?}");
        assert!(!display.contains("Backup ready"));
        assert!(!display.contains(TOKEN));
        assert!(!display.contains(&server.url()));
        check_wire(&server.single(), "GET", "/first-use/sse");
        assert_eq!(server.shutdown().await.open_after, 0);
    }
}

#[tokio::test]
async fn sse_rejects_wrong_media_type_before_waiting_for_body() {
    let server = TestServer::start(|_| {
        Reply::new(200)
            .header("content-type", "application/json")
            .header("x-request-id", "synthetic-request-id")
            .with_body(Body::Stall)
    })
    .await;
    let failure = tokio::time::timeout(DEADLINE, alerts(&server).subscribe_sse().send())
        .await
        .unwrap()
        .unwrap_err();
    let error = failure.request().expect("Concord metadata error");
    assert_eq!(error.kind(), concord::ErrorKind::Metadata);
    assert!(error.operation().is_some());
    let response = error.response().expect("unread rejected response");
    assert_eq!(response.status().as_u16(), 200);
    assert_eq!(response.headers()["x-request-id"], "synthetic-request-id");
    assert_eq!(server.body_bytes_sent(), 0);
    check_wire(&server.single(), "GET", "/first-use/sse");
    drop(failure);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn sse_provider_forbidden_is_typed_and_does_not_retry() {
    let server = TestServer::start(|_| Reply::json(403, FORBIDDEN)).await;
    let error = alerts(&server).subscribe_sse().send().await.unwrap_err();
    let api = error.api().expect("provider error");
    assert_eq!(api.status().as_u16(), 403);
    assert_eq!(api.code(), Some(40301));
    assert!(api.is_auth());
    assert!(!api.is_rate_limited());
    assert!(api.operation().is_some());
    let doc = api.document().unwrap();
    assert_eq!(doc.http, 403);
    assert_eq!(doc.error, "synthetic forbidden detail");
    assert_eq!(
        doc.link.as_deref(),
        Some("https://docs.ntfy.sh/publish/#authentication")
    );
    assert!(!format!("{error} {error:?}").contains(&doc.error));
    check_wire(&server.single(), "GET", "/first-use/sse");
    assert_eq!(server.shutdown().await.open_after, 0);
}

/// Independent minimal provider model: cached text publishing and finite JSON polling only.
/// Equal timestamps deliberately let an inclusive timestamp replay repeat messages.
#[derive(Default)]
struct FakeCache {
    published: u64,
    cached: Vec<Value>,
}

impl FakeCache {
    fn respond(&mut self, request: &Recorded) -> Reply {
        match (request.method.as_str(), request.path()) {
            ("POST", "/first-use") => {
                self.published += 1;
                let mut event = json!({
                    "id": format!("offline{:03}", self.published),
                    "time": 1_700_000_100u64,
                    "event": "message",
                    "topic": "first-use",
                    "message": std::str::from_utf8(&request.body).unwrap(),
                });
                if let Some(title) = request.header("x-title") {
                    event["title"] = json!(title);
                }
                if let Some(priority) = request.header("x-priority") {
                    event["priority"] = json!(priority.parse::<u8>().unwrap());
                }
                if let Some(tags) = request.header("x-tags") {
                    event["tags"] = json!(tags.split(',').collect::<Vec<_>>());
                }
                if request.header("x-cache") != Some("no") {
                    event["expires"] = json!(1_700_043_300u64);
                    self.cached.push(event.clone());
                }
                Reply::json(200, serde_json::to_vec(&event).unwrap())
            }
            ("GET", "/first-use/json") => {
                assert_eq!(request.query_all("poll"), ["1"]);
                let since = request.query_all("since");
                let selected: Vec<&Value> = match since.first().map(String::as_str) {
                    None | Some("all") => self.cached.iter().collect(),
                    Some("latest") => self.cached.last().into_iter().collect(),
                    Some(value) if value.parse::<u64>().is_ok() => {
                        let time = value.parse::<u64>().unwrap();
                        self.cached
                            .iter()
                            .filter(|event| event["time"].as_u64().unwrap() >= time)
                            .collect()
                    }
                    Some(id) => {
                        let index = self
                            .cached
                            .iter()
                            .position(|event| event["id"] == id)
                            .expect("this fake requires a known message cursor");
                        self.cached[index + 1..].iter().collect()
                    }
                };
                let mut body = Vec::new();
                for event in selected {
                    serde_json::to_writer(&mut body, event).unwrap();
                    body.push(b'\n');
                }
                Reply::text(200, "application/x-ndjson; charset=utf-8", body)
            }
            _ => panic!("unexpected fake provider request: {request:?}"),
        }
    }
}

async fn cached_provider() -> TestServer {
    let cache = Mutex::new(FakeCache::default());
    TestServer::start(move |request| cache.lock().unwrap().respond(request)).await
}

fn check_poll(request: &Recorded, since: Option<&str>) {
    check_wire(request, "GET", "/first-use/json");
    let mut actual = request.query_pairs();
    actual.sort();
    let mut expected = vec![("poll".to_owned(), "1".to_owned())];
    if let Some(since) = since {
        expected.push(("since".to_owned(), since.to_owned()));
    }
    expected.sort();
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn offline_publish_then_json_replay_excludes_uncached_and_resumes_after_last_seen() {
    let server = cached_provider().await;
    let topic = alerts(&server);
    // No subscriber exists during these publishes; the first returned ID was persisted earlier.
    let previous = topic
        .publish(MessageText::new("Previously seen").unwrap())
        .send()
        .await
        .unwrap();
    let kept = topic
        .publish(MessageText::new(PUBLISH_TEXT).unwrap())
        .set_title("Backup")
        .set_priority(Priority::High)
        .set_tags(Tags::new(["white_check_mark", "backup"]).unwrap())
        .send()
        .await
        .unwrap();
    let live_only = topic
        .publish(MessageText::new("synthetic transient ping").unwrap())
        .set_cache(Cache::No)
        .send()
        .await
        .unwrap();
    assert_eq!(kept.id.as_str(), "offline002");
    assert_eq!(kept.message.as_deref(), Some(PUBLISH_TEXT));
    assert_eq!(kept.title.as_deref(), Some("Backup"));
    assert_eq!(kept.priority, Some(Priority::High));
    assert_eq!(kept.tags, ["white_check_mark", "backup"]);
    assert_eq!(kept.expires, Some(1_700_043_300));
    assert!(live_only.expires.is_none());

    let mut replay = topic
        .poll()
        .set_since(Since::Id(previous.id.clone()))
        .send()
        .await
        .unwrap();
    assert_eq!(replay.mode(), Mode::Json);
    assert_eq!(replay.completion(), Completion::Reading);
    assert!(!replay.truncated());
    let mut seen = HashSet::from([previous.id.clone()]);
    let mut cursor = ReplayCursor::after(previous.id);
    let mut received = Vec::new();
    tokio::time::timeout(DEADLINE, async {
        while let Some(event) = replay.next().await {
            let event = event.unwrap();
            assert!(event.is_message()); // a poll has no open or keepalive
            cursor.observe(&event);
            if seen.insert(event.id.clone()) {
                received.push(event);
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(replay.completion(), Completion::EndOfInput);
    assert_eq!(replay.position().records, 1);
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].id, kept.id);
    assert_eq!(received[0].message, kept.message);
    assert!(!seen.contains(&live_only.id));
    assert_eq!(cursor.last_message(), Some(&kept.id));

    // Catching up from that persisted ID now produces an empty, successfully ended poll.
    let mut empty = topic
        .poll()
        .maybe_since(cursor.since())
        .send()
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(DEADLINE, empty.next())
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(empty.completion(), Completion::EndOfInput);
    assert_eq!(empty.position().records, 0);

    let requests = server.requests();
    assert_eq!(requests.len(), 5);
    for request in &requests[..3] {
        check_wire(request, "POST", "/first-use");
        assert!(request.query_pairs().is_empty());
    }
    assert_eq!(requests[0].body, b"Previously seen");
    assert_eq!(requests[1].body, PUBLISH_TEXT.as_bytes());
    assert_eq!(
        requests[1].header("content-type"),
        Some("text/plain; charset=utf-8")
    );
    assert_eq!(requests[1].header("x-title"), Some("Backup"));
    assert_eq!(requests[1].header("x-priority"), Some("4"));
    assert_eq!(
        requests[1].header("x-tags"),
        Some("white_check_mark,backup")
    );
    assert_eq!(requests[1].header("x-cache"), None);
    assert_eq!(requests[2].body, b"synthetic transient ping");
    assert_eq!(requests[2].header("x-cache"), Some("no"));
    check_poll(&requests[3], Some("offline001"));
    check_poll(&requests[4], Some("offline002"));
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn timestamp_replay_exposes_duplicates_for_the_application_to_skip() {
    let server = cached_provider().await;
    let topic = alerts(&server);
    let first = topic
        .publish(MessageText::new("first").unwrap())
        .send()
        .await
        .unwrap();
    let second = topic
        .publish(MessageText::new("second").unwrap())
        .send()
        .await
        .unwrap();
    let mut seen = HashSet::from([first.id]);
    let mut replay = topic
        .poll()
        .set_since(Since::Time(first.time))
        .send()
        .await
        .unwrap();
    let mut delivered = Vec::new();
    tokio::time::timeout(DEADLINE, async {
        while let Some(event) = replay.next().await {
            let event = event.unwrap();
            if seen.insert(event.id.clone()) {
                delivered.push(event.id);
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(replay.position().records, 2); // the SDK returns both; the application skips one
    assert_eq!(delivered, [second.id]);
    assert_eq!(replay.completion(), Completion::EndOfInput);
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    check_poll(&requests[2], Some("1700000100"));
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn truncated_chunked_json_poll_reaches_eof_without_claiming_complete_history() {
    let server = TestServer::start(|_| {
        Reply::new(200)
            .header("content-type", "application/x-ndjson; charset=utf-8")
            .header("x-messages-truncated", "1")
            .with_body(Body::Chunks(
                REPLAY
                    .chunks(7)
                    .map(|chunk| (Duration::ZERO, chunk.to_vec().into()))
                    .collect(),
            ))
    })
    .await;
    let mut replay = alerts(&server)
        .poll()
        .set_since(Since::All)
        .send()
        .await
        .unwrap();
    assert!(replay.truncated());
    let mut messages = Vec::new();
    tokio::time::timeout(DEADLINE, async {
        while let Some(event) = replay.next().await {
            messages.push(event.unwrap());
        }
    })
    .await
    .unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].id.as_str(), "replayed01");
    assert_eq!(messages[1].message.as_deref(), Some("Restored 🦀"));
    assert_eq!(messages[1].priority, Some(Priority::High));
    assert_eq!(replay.position().records, 2);
    assert_eq!(replay.completion(), Completion::EndOfInput);
    assert!(replay.truncated());
    assert!(replay.next().await.is_none());
    check_poll(&server.single(), Some("all"));
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn failed_json_replays_retain_prefix_and_never_report_success() {
    let first_end = REPLAY.iter().position(|b| *b == b'\n').unwrap() + 1;
    let prefix = &REPLAY[..first_end];
    let tails = [
        (
            ReadErrorKind::Format,
            Some(include_bytes!("../fixtures/first-use/malformed.jsonl").to_vec()),
        ),
        (
            ReadErrorKind::Decode,
            Some(include_bytes!("../fixtures/first-use/missing-fields.jsonl").to_vec()),
        ),
        (
            ReadErrorKind::Format,
            Some(prefix[..prefix.len() - 2].to_vec()),
        ), // missing closing brace and newline
        (ReadErrorKind::Transport, None),
    ];
    for (kind, tail) in tails {
        let body = if let Some(tail) = tail {
            Body::Full([prefix, tail.as_slice()].concat().into())
        } else {
            Body::FailAfter(vec![prefix.to_vec().into()])
        };
        let script = Script::new([Reply::new(200)
            .header("content-type", "application/x-ndjson")
            .with_body(body)]);
        let server = TestServer::start(move |_| script.next()).await;
        let mut replay = alerts(&server).poll().send().await.unwrap();
        let mut cursor = ReplayCursor::new();
        let received = next_event(&mut replay).await;
        cursor.observe(&received);
        let failure = tokio::time::timeout(DEADLINE, replay.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap_err();
        assert_eq!(failure.kind(), kind);
        assert_eq!(failure.position().records, 1);
        assert!(failure.position().bytes >= prefix.len() as u64);
        assert!(failure.operation().is_some());
        assert!(failure.read_error().is_some());
        assert_eq!(cursor.last_message().unwrap().as_str(), "replayed01");
        assert_eq!(replay.completion(), Completion::Failed);
        assert!(replay.next().await.is_none());
        assert_eq!(replay.completion(), Completion::Failed);
        let display = format!("{failure} {failure:?}");
        assert!(!display.contains("synthetic invalid JSON"));
        assert!(!display.contains(TOKEN));
        assert!(!display.contains(&server.url()));
        check_poll(&server.single(), None);
        assert_eq!(server.shutdown().await.open_after, 0);
    }
}

#[tokio::test]
async fn replay_bandwidth_error_retains_document_and_retry_after_without_retrying() {
    let server =
        TestServer::start(|_| Reply::json(429, BANDWIDTH).header("retry-after", "7")).await;
    let error = alerts(&server)
        .poll()
        .set_since(Since::Id(EventId::new("lastseen01").unwrap()))
        .send()
        .await
        .unwrap_err();
    let api = error.api().unwrap();
    assert_eq!(api.status().as_u16(), 429);
    assert_eq!(api.code(), Some(42905));
    assert!(api.is_rate_limited());
    assert!(!api.is_auth());
    assert_eq!(api.retry_after(), Some(Duration::from_secs(7)));
    assert!(api.operation().is_some());
    let document = api.document().unwrap();
    assert_eq!(document.http, 429);
    assert_eq!(document.error, "synthetic bandwidth detail");
    assert!(!format!("{error} {error:?}").contains(&document.error));
    check_poll(&server.single(), Some("lastseen01"));
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn json_poll_accepts_complete_final_value_without_newline_despite_readme_prediction() {
    let end = REPLAY.iter().position(|b| *b == b'\n').unwrap();
    let server =
        TestServer::start(move |_| Reply::text(200, "application/x-ndjson", &REPLAY[..end])).await;
    let mut replay = alerts(&server).poll().send().await.unwrap();
    let event = next_event(&mut replay).await;
    assert_eq!(event.id.as_str(), "replayed01");
    assert!(
        tokio::time::timeout(DEADLINE, replay.next())
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(replay.completion(), Completion::EndOfInput);
    assert_eq!(replay.position().records, 1);
    check_poll(&server.single(), None);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn rejected_publish_returns_provider_error_and_preserves_wire_body() {
    let server = TestServer::start(|_| Reply::json(403, FORBIDDEN)).await;
    let error = alerts(&server)
        .publish(MessageText::new(PUBLISH_TEXT).unwrap())
        .send()
        .await
        .unwrap_err();
    let api = error.api().unwrap();
    assert!(api.is_auth());
    assert_eq!(api.code(), Some(40301));
    assert_eq!(api.status().as_u16(), 403);
    assert!(api.operation().is_some());
    let request = server.single();
    check_wire(&request, "POST", "/first-use");
    assert_eq!(request.body, PUBLISH_TEXT.as_bytes());
    assert!(!format!("{error} {error:?}").contains(PUBLISH_TEXT));
    assert_eq!(server.shutdown().await.open_after, 0);
}

//! The two workflows of the brief, end to end through the public API against loopback fixtures.
//!
//! - **A**: subscribe to a topic's SSE feed and consume `open`/`message`/`keepalive` events until
//!   the caller cancels; a connection lost on the way is resumed with the replay cursor.
//! - **B**: publish to the (fake, in-memory) server while the subscriber is offline, then request
//!   the cached messages with documented `since` values and read the JSON poll response; a
//!   duplicate replay item is the caller's to deduplicate.

mod common;

use std::collections::HashSet;
use std::time::Duration;

use common::*;
use concord::bytes::Bytes;
use concord::stream::{Completion, ReadErrorKind};
use concord_example_testserver::{Body, Reply, TestServer};
use concord_sdk_ntfy::{
    Cache, Event, EventId, EventKind, Events, EventsError, MessageText, NtfyError, NtfyTopic,
    Priority, ReplayCursor, Since, Tags,
};
use serde_json::json;
use tokio::sync::{mpsc, oneshot};

// ---------------------------------------------------------------------------------------------
// Workflow A
// ---------------------------------------------------------------------------------------------

/// How the application's subscription loop ended.
#[derive(Debug)]
enum Ended {
    /// The caller cancelled it.
    Cancelled,
    /// The server ended the body cleanly (not expected of a live subscription).
    ServerClosed,
    /// A non-transport stream failure, or too many reconnects.
    Failed(EventsError),
}

/// What the application loop hands back.
struct Outcome {
    ended: Ended,
    cursor: ReplayCursor,
    reconnects: u32,
    /// The completion of the last connection when the loop stopped.
    completion: Completion,
}

/// The application's subscriber: follows the topic over SSE, hands every event to `deliver`,
/// reconnects after a lost connection from the replay cursor (at most `max_reconnects` times),
/// and stops when `stop` fires. Reconnecting and the reconnect budget are this application's
/// policy, not the SDK's.
async fn follow(
    topic: NtfyTopic,
    stop: oneshot::Receiver<()>,
    deliver: mpsc::UnboundedSender<Event>,
    max_reconnects: u32,
) -> Result<Outcome, NtfyError> {
    let mut cursor = ReplayCursor::new();
    let mut reconnects = 0;
    tokio::pin!(stop);
    loop {
        let mut events: Events = topic
            .subscribe_sse()
            .maybe_since(cursor.since())
            .send()
            .await?;
        let ended = loop {
            tokio::select! {
                biased;
                _ = &mut stop => break Some(Ended::Cancelled),
                item = events.next() => match item {
                    Some(Ok(event)) => {
                        cursor.observe(&event);
                        let _ = deliver.send(event);
                    }
                    Some(Err(lost)) if lost.kind() == ReadErrorKind::Transport
                        && reconnects < max_reconnects => break None,
                    Some(Err(error)) => break Some(Ended::Failed(error)),
                    None => break Some(Ended::ServerClosed),
                },
            }
        };
        let completion = events.completion();
        drop(events); // closes this connection
        match ended {
            Some(ended) => {
                return Ok(Outcome {
                    ended,
                    cursor,
                    reconnects,
                    completion,
                });
            }
            None => reconnects += 1,
        }
    }
}

/// Receives delivered events until `messages` messages arrived; returns all of them.
async fn until_messages(
    received: &mut mpsc::UnboundedReceiver<Event>,
    messages: usize,
) -> Vec<Event> {
    let mut events = Vec::new();
    while events.iter().filter(|e: &&Event| e.is_message()).count() < messages {
        events.push(received.recv().await.expect("the subscriber is running"));
    }
    events
}

#[tokio::test]
async fn workflow_a_subscribe_until_cancelled_resuming_a_lost_connection() {
    // First connection: the perturbed fixture cut inside a message and inside a delimiter, and
    // dropped (no terminating chunk) in the middle of the second message. Second connection,
    // which must ask `since=<first message id>`: the second message replayed, then a keepalive
    // every 20 ms for as long as the client stays.
    let f = SSE_PERTURBED;
    let mid_json = after(f, b"\"title\":\"Ba");
    let delimiter = after(f, b"[1,2]}}\n");
    let broken = after(f, b"\"id\":\"synMsg000002\",\"time\":17672");
    let second = after(f, b"\"keepalive\",\"topic\":\"alerts\"}\n\n");
    let server = TestServer::start(move |request| {
        let since = request.query_all("since");
        let body = match since.as_slice() {
            [] => Body::FailAfter(vec![
                Bytes::copy_from_slice(&f[..mid_json]),
                Bytes::copy_from_slice(&f[mid_json..delimiter]),
                Bytes::copy_from_slice(&f[delimiter..broken]),
            ]),
            [id] if id == "synMsg000001" => then_keepalives(
                vec![
                    sse_bytes(&json!({"id": "synOpen00009", "time": 1_767_225_900, "event": "open", "topic": "alerts"})),
                    Bytes::copy_from_slice(&f[second..]),
                ],
                Duration::from_millis(20),
            ),
            other => panic!("unexpected since {other:?}"),
        };
        Reply::new(200)
            .header("content-type", SSE_TYPE)
            .with_body(body)
    })
    .await;

    let (stop, stop_rx) = oneshot::channel();
    let (deliver, mut delivered) = mpsc::unbounded_channel();
    let subscriber = tokio::spawn(follow(alerts(&server), stop_rx, deliver, 3));

    // The caller decides when to stop: here, once both messages arrived and one more event
    // proves the subscription is still live.
    let mut events = until_messages(&mut delivered, 2).await;
    events.push(delivered.recv().await.unwrap());
    stop.send(()).unwrap();
    let outcome = subscriber.await.unwrap().unwrap();

    assert!(
        matches!(outcome.ended, Ended::Cancelled),
        "{:?}",
        outcome.ended
    );
    assert_eq!(outcome.reconnects, 1);
    assert_eq!(
        outcome.completion,
        Completion::Reading,
        "a cancelled subscription is not an end of input"
    );
    let kinds: Vec<_> = events.iter().map(|e| e.kind.clone()).collect();
    assert_eq!(
        kinds[..6],
        [
            EventKind::Open,
            EventKind::Message,
            EventKind::Keepalive,
            EventKind::Open, // the reconnect
            EventKind::Message,
            EventKind::Keepalive,
        ]
    );
    let messages: Vec<_> = events.iter().filter(|e| e.is_message()).collect();
    assert_eq!(
        messages[0].message.as_deref(),
        Some("Backup successful \u{1F600}")
    );
    assert_eq!(messages[1].id.as_str(), "synMsg000002");
    assert_eq!(
        outcome.cursor.since(),
        Some(Since::Id(EventId::new("synMsg000002").unwrap()))
    );

    // On the wire: the reconnect asked for the messages after the last one received.
    let targets: Vec<_> = server.requests().into_iter().map(|r| r.target).collect();
    assert_eq!(targets, ["/alerts/sse", "/alerts/sse?since=synMsg000001"]);
    // Cancelling dropped the reader, which closed the connection: the server's next keepalive
    // write fails and its connection task ends.
    assert!(
        server
            .wait_until(Duration::from_secs(5), |s| s.live_connections() == 0)
            .await,
        "the cancelled subscription's connection was closed"
    );
}

#[tokio::test]
async fn workflow_a_a_connection_lost_before_any_message_resumes_from_its_open_time() {
    let open = sse_bytes(
        &json!({"id": "synOpen00001", "time": 1_767_225_600, "event": "open", "topic": "alerts"}),
    );
    let server = TestServer::start(move |request| {
        let body = if request.query_all("since").is_empty() {
            Body::FailAfter(vec![open.clone()])
        } else {
            then_keepalives(
                vec![sse_bytes(&json!({"id": "synMsg000001", "time": 1_767_225_605, "event": "message", "topic": "alerts", "message": "while away"}))],
                Duration::from_millis(20),
            )
        };
        Reply::new(200)
            .header("content-type", SSE_TYPE)
            .with_body(body)
    })
    .await;
    let (stop, stop_rx) = oneshot::channel();
    let (deliver, mut delivered) = mpsc::unbounded_channel();
    let subscriber = tokio::spawn(follow(alerts(&server), stop_rx, deliver, 1));
    let events = until_messages(&mut delivered, 1).await;
    stop.send(()).unwrap();
    let outcome = subscriber.await.unwrap().unwrap();
    assert!(matches!(outcome.ended, Ended::Cancelled));
    assert_eq!(
        events.last().unwrap().message.as_deref(),
        Some("while away")
    );
    let targets: Vec<_> = server.requests().into_iter().map(|r| r.target).collect();
    assert_eq!(targets, ["/alerts/sse", "/alerts/sse?since=1767225600"]);
}

#[tokio::test]
async fn workflow_a_the_reconnect_budget_is_the_callers() {
    // Every connection breaks after `open`: with a budget of 2 the loop gives up on the third
    // loss and reports it.
    let server = TestServer::start(|_| {
        Reply::new(200)
            .header("content-type", SSE_TYPE)
            .with_body(Body::FailAfter(vec![sse_bytes(
                &json!({"id": "synOpen00001", "time": 1, "event": "open", "topic": "alerts"}),
            )]))
    })
    .await;
    let (_stop, stop_rx) = oneshot::channel();
    let (deliver, _delivered) = mpsc::unbounded_channel();
    let outcome = follow(alerts(&server), stop_rx, deliver, 2).await.unwrap();
    let Ended::Failed(error) = outcome.ended else {
        panic!("{:?}", outcome.ended)
    };
    assert_eq!(error.kind(), ReadErrorKind::Transport);
    assert_eq!(
        (outcome.reconnects, outcome.completion),
        (2, Completion::Failed)
    );
    assert_eq!(server.requests().len(), 3);
}

// ---------------------------------------------------------------------------------------------
// Workflow B
// ---------------------------------------------------------------------------------------------

/// Reads a whole poll response.
async fn replay(events: Events) -> Vec<Event> {
    let mut events = events;
    let mut out = Vec::new();
    while let Some(event) = events.next().await {
        out.push(event.expect("a clean replay"));
    }
    assert_eq!(events.completion(), Completion::EndOfInput);
    assert!(!events.truncated());
    out
}

fn ids(events: &[Event]) -> Vec<&str> {
    events.iter().map(|e| e.id.as_str()).collect()
}

#[tokio::test]
async fn workflow_b_publish_while_offline_then_replay_the_cache() {
    let fake = FakeNtfy::default();
    let server = fake.start().await;
    let topic = alerts_with_token(&server);
    let text = |t: &str| MessageText::new(t).unwrap();

    // Online: a live subscription sees `open`, then a message published now.
    let mut cursor = ReplayCursor::new();
    let mut seen = HashSet::new();
    let mut live = topic.subscribe_sse().send().await.unwrap();
    let open = live.next().await.unwrap().unwrap();
    assert_eq!(open.kind, EventKind::Open);
    cursor.observe(&open);
    let first = topic
        .publish(text("first"))
        .set_priority(Priority::High)
        .send()
        .await
        .unwrap();
    loop {
        let event = live.next().await.unwrap().unwrap();
        cursor.observe(&event);
        if event.is_message() {
            assert_eq!(event, first, "the live copy is the published message");
            seen.insert(event.id.clone());
            break;
        }
    }

    // Offline: the subscriber disconnects; three messages are published meanwhile, one of them
    // with `Cache: no`.
    drop(live);
    let second = topic.publish(text("second")).send().await.unwrap();
    let uncached = topic
        .publish(text("not kept"))
        .set_cache(Cache::No)
        .send()
        .await
        .unwrap();
    let third = topic
        .publish(text("third"))
        .set_title("Disk")
        .set_tags(Tags::new(["warning"]).unwrap())
        .send()
        .await
        .unwrap();
    assert!(second.expires.is_some());
    assert_eq!(
        uncached.expires, None,
        "Cache: no leaves the message without expiry"
    );
    assert_eq!(fake.cached(), 3);

    // Back online, replay after the last message received (the cursor's id).
    assert_eq!(cursor.since(), Some(Since::Id(first.id.clone())));
    let after_cursor = replay(
        topic
            .poll()
            .maybe_since(cursor.since())
            .send()
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(ids(&after_cursor), [second.id.as_str(), third.id.as_str()]);
    assert_eq!(after_cursor[1].title.as_deref(), Some("Disk"));
    assert_eq!(after_cursor[1].tags, ["warning"]);

    // Replay from a documented timestamp (the first message's `time`): the fake includes that
    // second, so the first message comes again. The SDK hands the duplicate over; skipping it
    // is this caller's decision, by id.
    let from_time = replay(
        topic
            .poll()
            .set_since(Since::Time(first.time))
            .send()
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(
        ids(&from_time),
        [first.id.as_str(), second.id.as_str(), third.id.as_str()]
    );
    let new: Vec<Event> = from_time
        .into_iter()
        .filter(|e| seen.insert(e.id.clone()))
        .collect();
    assert_eq!(ids(&new), [second.id.as_str(), third.id.as_str()]);

    // The `Cache: no` message is in no replay, whatever the cursor.
    let everything = replay(topic.poll().set_since(Since::All).send().await.unwrap()).await;
    assert!(everything.iter().all(|e| e.id != uncached.id));
    let latest = replay(topic.poll().set_since(Since::Latest).send().await.unwrap()).await;
    assert_eq!(ids(&latest), [third.id.as_str()]);

    // On the wire: the publish bodies are the texts, the replays are JSON polls.
    let requests = server.requests();
    let publish = requests
        .iter()
        .find(|r| r.method == "POST" && r.body == b"not kept")
        .unwrap();
    assert_eq!(publish.header("x-cache"), Some("no"));
    let polls: Vec<_> = requests
        .iter()
        .filter(|r| r.path() == "/alerts/json")
        .map(|r| r.target.as_str())
        .collect();
    let first_time = format!("/alerts/json?since={}&poll=1", first.time);
    let first_id = format!("/alerts/json?since={}&poll=1", first.id);
    assert_eq!(
        polls,
        [
            first_id.as_str(),
            first_time.as_str(),
            "/alerts/json?since=all&poll=1",
            "/alerts/json?since=latest&poll=1",
        ]
    );
}

#[tokio::test]
async fn workflow_b_a_protected_topic_without_credentials_is_a_typed_error() {
    let fake = FakeNtfy::default();
    let server = fake.start().await;

    let anonymous = alerts(&server).poll().send().await.unwrap_err();
    let api = anonymous.api().expect("an API error");
    assert_eq!((api.status().as_u16(), api.code()), (401, Some(40101)));
    assert!(api.is_auth());

    let wrong = client(&server)
        .with_token(&concord_sdk_ntfy::AccessToken::new("another-synthetic-value").unwrap())
        .topic(concord_sdk_ntfy::Topic::new("alerts").unwrap());
    let refused = wrong
        .publish(MessageText::new("x").unwrap())
        .send()
        .await
        .unwrap_err();
    assert_eq!(refused.api().and_then(|a| a.code()), Some(40301));
    assert_eq!(fake.cached(), 0, "nothing was published");
}

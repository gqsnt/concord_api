//! Records against a real server (C11.8 to C11.13): server-sent events from axum's `Sse`, with
//! its keep-alive comments and events flushed one by one, and JSON lines from a chunked body;
//! each read raw and mapped with `try_map` under both endings, and cut by the server before
//! the last record, cleanly or by breaking the body.

use crate::server::{Feed, Feeder, Frozen, Server, Tap, Wire};
use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use bytes::Bytes;
use concord::error::{Class, Kind};
use concord::read::{
    JsonLine, JsonLines, RecordError, RecordFail, RecordStream, Sse as SseReader, SseEvent, Step,
};
use serde::Deserialize;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// A line of the JSON lines.
#[derive(Debug, Deserialize, PartialEq)]
struct Row {
    n: u32,
}

/// An event `done` is the last record; any other event yields its data.
fn event(event: SseEvent) -> Result<Step<String>, RecordFail<()>> {
    Ok(match event.event.as_str() {
        "done" => Step::Last(event.data),
        _ => Step::Next(event.data),
    })
}

/// A line `{"n":0}` is the last record; any other line yields its number.
fn line(line: JsonLine) -> Result<Step<u32>, RecordFail<()>> {
    let row: Row = line.json()?;
    Ok(if row.n == 0 {
        Step::Last(row.n)
    } else {
        Step::Next(row.n)
    })
}

concord::api! {
    client Live {
        base "http://live.test";
        GET events() -> RecordStream<SseEvent> { path "events"; read SseReader::new(1024); }
        GET events_to_last() -> RecordStream<String, live::Error> {
            path "events";
            read SseReader::new(1024).try_map(event).until_last_record();
        }
        GET events_to_end() -> RecordStream<String, live::Error> {
            path "events";
            read SseReader::new(1024).try_map(event).until_end_of_body();
        }
        GET lines() -> RecordStream<Row> { path "lines"; read JsonLines::new(256); }
        GET lines_to_last() -> RecordStream<u32, live::Error> {
            path "lines";
            read JsonLines::new(256).try_map(line).until_last_record();
        }
        GET lines_to_end() -> RecordStream<u32, live::Error> {
            path "lines";
            read JsonLines::new(256).try_map(line).until_end_of_body();
        }
    }
}

const KEEP_ALIVE: Duration = Duration::from_secs(1);

/// The streams that the next requests answer with, in order.
#[derive(Clone, Default)]
struct Streams {
    events: Arc<Mutex<VecDeque<Feed<Event>>>>,
    lines: Arc<Mutex<VecDeque<Feed<Bytes>>>>,
}

impl Streams {
    fn events(&self) -> Feeder<Event> {
        let (feeder, feed) = Feed::new();
        self.events.lock().unwrap().push_back(feed);
        feeder
    }

    fn lines(&self) -> Feeder<Bytes> {
        let (feeder, feed) = Feed::new();
        self.lines.lock().unwrap().push_back(feed);
        feeder
    }
}

/// `/events`: axum's `Sse`, with a keep-alive comment after each second of silence.
/// `/lines`: a chunked body of JSON lines.
async fn live_server(streams: &Streams) -> Server {
    let router = Router::new()
        .route(
            "/events",
            get(|State(streams): State<Streams>| async move {
                let feed = streams.events.lock().unwrap().pop_front().unwrap();
                Sse::new(feed).keep_alive(KeepAlive::new().interval(KEEP_ALIVE).text("still here"))
            }),
        )
        .route(
            "/lines",
            get(|State(streams): State<Streams>| async move {
                let feed = streams.lines.lock().unwrap().pop_front().unwrap();
                (
                    [("content-type", "application/x-ndjson")],
                    axum::body::Body::from_stream(feed),
                )
                    .into_response()
            }),
        )
        .with_state(streams.clone());
    Server::start(router).await
}

/// Reads a stream in a task of its own and hands over each outcome, so that its wait is in
/// progress while the test moves the clock or feeds the server.
fn reader<T: Send + 'static, E: Send + 'static>(
    mut stream: RecordStream<T, E>,
) -> tokio::sync::mpsc::UnboundedReceiver<Result<Option<T>, E>> {
    let (got, outcomes) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move {
        loop {
            let next = stream.try_next().await;
            let end = !matches!(next, Ok(Some(_)));
            if got.send(next).is_err() || end {
                return;
            }
        }
    });
    outcomes
}

// Server-sent events --------------------------------------------------------------------

async fn raw_events(wire: Wire) {
    let _clock = Frozen::hold();
    let streams = Streams::default();
    let server = live_server(&streams).await;
    let tap = Tap::default();
    let live = Live::new()
        .with_transport(wire.http())
        .with_base(server.url())
        .with_middleware(tap.clone())
        .with_idle_timeout(Duration::from_millis(1500));

    let mut feeder = streams.events();
    let stream = live.events().await.unwrap();
    assert_eq!(
        stream.headers().get("content-type"),
        Some("text/event-stream")
    );
    let mut got = reader(stream);

    feeder.send(Event::default().data("a").id("1"));
    let first = got.recv().await.unwrap().unwrap().unwrap();
    assert_eq!(
        (first.event.as_str(), first.data.as_str(), first.id.as_str()),
        ("message", "a", "1")
    );

    // Four seconds with no event, longer than the idle timeout of 1.5 seconds: a keep-alive
    // comment each second is a sign of life (C11.10), and no record (C11.9).
    // Each step waits until the comment has reached the reader, which restarts its idle timer.
    for _ in 0..4 {
        let (polls, taken) = (feeder.polls(), tap.taken());
        tokio::time::advance(KEEP_ALIVE).await;
        feeder.polled_after(polls).await;
        tap.taken_after(taken).await;
    }
    feeder.send(Event::default().comment("a comment of the server's own"));
    feeder.send(Event::default().event("update").data("b\nc"));
    let second = got.recv().await.unwrap().unwrap().unwrap();
    assert_eq!(
        (second.event.as_str(), second.data.as_str()),
        ("update", "b\nc")
    );
    assert_eq!(
        second.id, "1",
        "the last id is kept from one event to the next"
    );

    // The server ends the body before any last record: a raw stream ends with it.
    drop(feeder);
    assert!(got.recv().await.unwrap().unwrap().is_none());

    // The server breaks the body: the failure is kept whole, with the head.
    let feeder = streams.events();
    let mut got = reader(live.events().await.unwrap());
    feeder.send(Event::default().data("before the break"));
    assert_eq!(
        got.recv().await.unwrap().unwrap().unwrap().data,
        "before the break"
    );
    feeder.fail();
    match got.recv().await.unwrap() {
        Err(RecordError::Stream(error)) => {
            assert_eq!(error.class(), Some(Class::Body));
            assert_eq!(error.status().map(|status| status.as_u16()), Some(200));
        }
        other => panic!("not the failure of the body: {other:?}"),
    }
}

#[tokio::test(start_paused = true)]
async fn c11_10_server_sent_events_from_axum_read_raw_with_keep_alive_comments_over_http1() {
    raw_events(Wire::One).await;
}

#[tokio::test(start_paused = true)]
async fn c11_10_server_sent_events_from_axum_read_raw_with_keep_alive_comments_over_http2() {
    raw_events(Wire::Two).await;
}

async fn mapped_events(wire: Wire) {
    let streams = Streams::default();
    let server = live_server(&streams).await;
    let live = Live::new()
        .with_transport(wire.http())
        .with_base(server.url());

    // Until the last record: the stream ends at `done`, before the body does.
    let feeder = streams.events();
    let mut got = reader(live.events_to_last().await.unwrap());
    for data in ["one", "two"] {
        feeder.send(Event::default().data(data));
        assert_eq!(got.recv().await.unwrap().unwrap().as_deref(), Some(data));
    }
    feeder.send(Event::default().event("done").data("fin"));
    assert_eq!(got.recv().await.unwrap().unwrap().as_deref(), Some("fin"));
    // The stream ends at once, while the server still holds the body open.
    assert_eq!(got.recv().await.unwrap().unwrap(), None);

    // Until the last record, and the server ends the body before it: a Mismatch.
    let feeder = streams.events();
    let mut got = reader(live.events_to_last().await.unwrap());
    feeder.send(Event::default().data("one"));
    assert_eq!(got.recv().await.unwrap().unwrap().as_deref(), Some("one"));
    drop(feeder);
    let error = got.recv().await.unwrap().unwrap_err();
    assert_eq!(error.kind(), Kind::Mismatch);
    assert!(
        error
            .to_string()
            .contains("the body ended before its last record"),
        "{error}"
    );

    // Until the end of the body: the same end is the normal end.
    let feeder = streams.events();
    let mut got = reader(live.events_to_end().await.unwrap());
    feeder.send(Event::default().data("one"));
    assert_eq!(got.recv().await.unwrap().unwrap().as_deref(), Some("one"));
    drop(feeder);
    assert_eq!(got.recv().await.unwrap().unwrap(), None);

    // The server breaks the body: the client's own Transport error, with the head.
    let feeder = streams.events();
    let mut got = reader(live.events_to_end().await.unwrap());
    feeder.send(Event::default().data("one"));
    assert_eq!(got.recv().await.unwrap().unwrap().as_deref(), Some("one"));
    feeder.fail();
    let error = got.recv().await.unwrap().unwrap_err();
    assert_eq!(error.kind(), Kind::Transport);
    assert_eq!(error.class(), Some(Class::Body));
    assert_eq!(error.status().map(|status| status.as_u16()), Some(200));
}

#[tokio::test]
async fn c11_13_server_sent_events_mapped_under_both_endings_and_cut_before_the_last_over_http1() {
    mapped_events(Wire::One).await;
}

#[tokio::test]
async fn c11_13_server_sent_events_mapped_under_both_endings_and_cut_before_the_last_over_http2() {
    mapped_events(Wire::Two).await;
}

// JSON lines ----------------------------------------------------------------------------

async fn raw_lines(wire: Wire) {
    let streams = Streams::default();
    let server = live_server(&streams).await;
    let live = Live::new()
        .with_transport(wire.http())
        .with_base(server.url());

    // Records are framed the same way wherever the chunks are cut (C11.9).
    let feeder = streams.lines();
    let mut got = reader(live.lines().await.unwrap());
    feeder.send(Bytes::from_static(b"{\"n\":1}\n{\"n\":"));
    assert_eq!(got.recv().await.unwrap().unwrap(), Some(Row { n: 1 }));
    feeder.send(Bytes::from_static(b"2}\r\n{\"n\":3}"));
    assert_eq!(got.recv().await.unwrap().unwrap(), Some(Row { n: 2 }));
    // A last line without LF is read when the body ends.
    drop(feeder);
    assert_eq!(got.recv().await.unwrap().unwrap(), Some(Row { n: 3 }));
    assert_eq!(got.recv().await.unwrap().unwrap(), None);

    // The server breaks the body inside a line.
    let feeder = streams.lines();
    let mut got = reader(live.lines().await.unwrap());
    feeder.send(Bytes::from_static(b"{\"n\":1}\n{\"n\":"));
    assert_eq!(got.recv().await.unwrap().unwrap(), Some(Row { n: 1 }));
    feeder.fail();
    match got.recv().await.unwrap() {
        Err(RecordError::Stream(error)) => assert_eq!(error.class(), Some(Class::Body)),
        other => panic!("not the failure of the body: {other:?}"),
    }
}

#[tokio::test]
async fn c11_9_json_lines_from_a_chunked_body_read_raw_and_broken_inside_a_line_over_http1() {
    raw_lines(Wire::One).await;
}

#[tokio::test]
async fn c11_9_json_lines_from_a_chunked_body_read_raw_and_broken_inside_a_line_over_http2() {
    raw_lines(Wire::Two).await;
}

async fn mapped_lines(wire: Wire) {
    let streams = Streams::default();
    let server = live_server(&streams).await;
    let live = Live::new()
        .with_transport(wire.http())
        .with_base(server.url());

    // Until the last record: `{"n":0}` ends the stream.
    let feeder = streams.lines();
    let mut got = reader(live.lines_to_last().await.unwrap());
    feeder.send(Bytes::from_static(b"{\"n\":5}\n"));
    assert_eq!(got.recv().await.unwrap().unwrap(), Some(5));
    feeder.send(Bytes::from_static(b"{\"n\":0}\n"));
    assert_eq!(got.recv().await.unwrap().unwrap(), Some(0));
    // The stream ends at once, while the server still holds the body open.
    assert_eq!(got.recv().await.unwrap().unwrap(), None);

    // Until the last record, and the server ends the body before it.
    let feeder = streams.lines();
    let mut got = reader(live.lines_to_last().await.unwrap());
    feeder.send(Bytes::from_static(b"{\"n\":5}\n"));
    assert_eq!(got.recv().await.unwrap().unwrap(), Some(5));
    drop(feeder);
    let error = got.recv().await.unwrap().unwrap_err();
    assert_eq!(error.kind(), Kind::Mismatch);
    assert!(
        error
            .to_string()
            .contains("the body ended before its last record"),
        "{error}"
    );

    // Until the end of the body: the same end is the normal end.
    let feeder = streams.lines();
    let mut got = reader(live.lines_to_end().await.unwrap());
    feeder.send(Bytes::from_static(b"{\"n\":5}\n"));
    assert_eq!(got.recv().await.unwrap().unwrap(), Some(5));
    drop(feeder);
    assert_eq!(got.recv().await.unwrap().unwrap(), None);

    // The server breaks the body inside a line.
    let feeder = streams.lines();
    let mut got = reader(live.lines_to_end().await.unwrap());
    feeder.send(Bytes::from_static(b"{\"n\":5}\n{\"n\""));
    assert_eq!(got.recv().await.unwrap().unwrap(), Some(5));
    feeder.fail();
    let error = got.recv().await.unwrap().unwrap_err();
    assert_eq!(error.kind(), Kind::Transport);
    assert_eq!(error.class(), Some(Class::Body));
}

#[tokio::test]
async fn c11_13_json_lines_mapped_under_both_endings_and_cut_before_the_last_over_http1() {
    mapped_lines(Wire::One).await;
}

#[tokio::test]
async fn c11_13_json_lines_mapped_under_both_endings_and_cut_before_the_last_over_http2() {
    mapped_lines(Wire::Two).await;
}

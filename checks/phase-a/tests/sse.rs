//! Server-sent events through a declared endpoint: `-> stream sse::Event` with `response Sse;`
//! returns the reader from `send()` (head checked, body unread), with the endpoint's identity in
//! every later error, a connection lost mid-event reported as such, and the standard-stream
//! bridge.

use std::time::Duration;

use concord::ErrorKind;
use concord::bytes::Bytes;
use concord::stream::{Completion, ReadErrorKind};
use concord_check_phase_a::{Reports, ReportsConfig};
use concord_example_testserver::{Body, Reply, TestServer};
use futures_util::StreamExt;

fn api(server: &TestServer) -> Reports {
    Reports::new(
        concord::reqwest::Client::new(),
        ReportsConfig {
            token: "Bearer t".to_owned(),
        },
    )
    .with_base_url(format!("{}/v1", server.url()))
    .unwrap()
}

fn sse(body: Body) -> Reply {
    Reply::new(200)
        .header("content-type", "text/event-stream; charset=utf-8")
        .with_body(body)
}

fn chunks(parts: &[&'static str]) -> Body {
    Body::Chunks(
        parts
            .iter()
            .map(|text| (Duration::ZERO, Bytes::from_static(text.as_bytes())))
            .collect(),
    )
}

#[tokio::test]
async fn a_declared_event_stream_returns_its_reader_and_reads_to_a_clean_end() {
    let server = TestServer::start(|_| {
        sse(chunks(&[
            ": connected\n\nretry: 1500\nid: 41\nevent: upd",
            "ate\ndata: {\"id\":1}\r",
            "\n\r\nid: 42\ndata: {\"id\":2}\ndata: {\"id\":3}\n\n",
        ]))
    })
    .await;
    let mut events = api(&server).live().send().await.unwrap();
    assert_eq!(events.position().records, 0, "opening reads no event");
    let request = server.single();
    assert_eq!(request.target, "/v1/live");
    assert_eq!(
        request.header("accept"),
        Some("text/event-stream"),
        "the format's Accept"
    );
    let first = events.next().await.unwrap().unwrap();
    assert_eq!(
        (first.event(), first.data(), first.id()),
        ("update", "{\"id\":1}", "41")
    );
    let second = events.next().await.unwrap().unwrap();
    assert_eq!(
        (second.event(), second.data(), second.id()),
        ("message", "{\"id\":2}\n{\"id\":3}", "42")
    );
    assert!(events.next().await.is_none());
    assert_eq!(events.completion(), Completion::EndOfInput);
    assert_eq!(events.last_event_id(), "42");
    assert_eq!(events.retry(), Some(Duration::from_millis(1500)));
    assert_eq!(events.position().records, 2);
}

/// A body that breaks off in the middle of an event (`Body::FailAfter`: no terminating chunk) is a
/// terminal `Transport` error naming the endpoint, URL removed, never a quiet end.
#[tokio::test]
async fn a_connection_lost_mid_event_names_the_endpoint() {
    let server = TestServer::start(|_| {
        sse(Body::FailAfter(vec![
            Bytes::from_static(b"id: 1\ndata: one\n\n"),
            Bytes::from_static(b"id: 2\ndata: tw"),
        ]))
    })
    .await;
    let mut events = api(&server).live().send().await.unwrap();
    assert_eq!(events.next().await.unwrap().unwrap().data(), "one");
    let error = events.next().await.unwrap().unwrap_err();
    assert_eq!(error.kind(), ReadErrorKind::Transport);
    assert!(error.is_terminal());
    assert_eq!(error.operation(), Some("Reports.live"));
    assert_eq!(error.position().records, 1);
    let shown = format!("{error} {error:?}");
    assert!(shown.starts_with("Reports.live: reading failed"), "{shown}");
    assert!(!shown.contains("127.0.0.1"), "{shown}");
    assert!(events.next().await.is_none(), "fused");
    assert_eq!(events.completion(), Completion::Failed);
    assert_eq!(
        events.last_event_id(),
        "1",
        "resume after the last complete event"
    );
}

/// A clean end of the body inside an event is a truncation error, also through the bridge to a
/// standard stream, which keeps the failure's kind.
#[tokio::test]
async fn a_body_ending_inside_an_event_fails_through_the_stream_bridge() {
    let server =
        TestServer::start(|_| sse(chunks(&["data: one\n\n", "event: update\ndata: two\n"]))).await;
    let mut stream = api(&server).live().send().await.unwrap().into_stream();
    assert_eq!(stream.next().await.unwrap().unwrap().data(), "one");
    let error = stream.next().await.unwrap().unwrap_err();
    assert_eq!(error.kind(), ReadErrorKind::Format);
    assert_eq!(error.operation(), Some("Reports.live"));
    assert!(stream.next().await.is_none());
    assert_eq!(stream.completion(), Completion::Failed);
    assert_eq!(stream.failure(), Some(ReadErrorKind::Format));
}

#[tokio::test]
async fn a_refused_event_stream_head_keeps_the_response_unread() {
    for content_type in ["application/json", "text/event-stream; charset=iso-8859-1"] {
        let server = TestServer::start(move |_| {
            Reply::new(200)
                .header("content-type", content_type)
                .body("{\"error\":\"not a stream\"}")
        })
        .await;
        let error = api(&server).live().send().await.unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Metadata, "{content_type}");
        assert_eq!(error.endpoint(), Some("Reports.live"));
        assert_eq!(
            error.into_response().unwrap().text().await.unwrap(),
            "{\"error\":\"not a stream\"}"
        );
    }
}

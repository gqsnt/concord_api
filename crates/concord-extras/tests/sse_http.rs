//! The server-sent events reader over real loopback HTTP/1.1 responses (reqwest): the first event
//! before the server finishes, opening refusals that keep the body unread, a connection lost in
//! the middle of an event, cancellation while awaiting data, and safe diagnostics. Each test owns
//! its peer (one connection, joined or aborted).
#![cfg(feature = "sse")]

mod support;

use std::time::{Duration, Instant};

use concord_core::Operation;
use concord_extras::sse::{SseOptions, SseReader};
use concord_extras::stream::{Completion, OpenErrorKind, ReadErrorKind};
use support::{Peer, client, end_chunks, read_head, until_closed, write_chunk, write_head};
use tokio::io::AsyncWriteExt;
use tokio::sync::oneshot;
use tokio::time::timeout;

const SECRET: &str = "SECRET-7f3a91";

const SSE_CHUNKED: [&str; 2] = [
    "content-type: text/event-stream",
    "transfer-encoding: chunked",
];

/// Event 1 arrives while the server withholds the rest; an event split across that pause is
/// completed when the tail arrives, and a `next()` dropped while waiting for it loses nothing.
#[tokio::test]
async fn first_event_arrives_before_the_server_finishes() {
    let (release, released) = oneshot::channel::<()>();
    let peer = Peer::start(move |mut s| async move {
        read_head(&mut s).await;
        write_head(&mut s, "200 OK", &SSE_CHUNKED).await;
        write_chunk(&mut s, b"retry: 2000\nid: 1\ndata: one\n\nid: 2\ndata: tw").await;
        released.await.expect("released by the test");
        write_chunk(&mut s, b"o\r").await;
        write_chunk(&mut s, b"\n\r\n: bye\n").await;
        end_chunks(&mut s).await;
        until_closed(&mut s).await;
    })
    .await;
    let client = client();
    let response = client.get(peer.url()).send().await.unwrap();
    let mut events = SseReader::default().open(response).unwrap();
    let first = timeout(Duration::from_secs(5), events.next())
        .await
        .expect("event 1 arrives while the tail is withheld")
        .unwrap()
        .unwrap();
    assert_eq!(
        (first.event(), first.data(), first.id()),
        ("message", "one", "1")
    );
    assert_eq!(events.retry(), Some(Duration::from_secs(2)));
    assert!(
        timeout(Duration::from_millis(100), events.next())
            .await
            .is_err(),
        "event 2 needs the withheld tail"
    );
    assert_eq!(events.completion(), Completion::Reading);
    assert_eq!(events.last_event_id(), "1", "event 2 is not complete");
    release.send(()).unwrap();
    let second = events.next().await.unwrap().unwrap();
    assert_eq!((second.data(), second.id()), ("two", "2"));
    assert!(events.next().await.is_none());
    assert_eq!(events.completion(), Completion::EndOfInput);
    assert_eq!(events.position().records, 2);
    drop(events);
    drop(client);
    peer.join().await;
}

/// Serves one response with `status` and `content_type` (or none) and a fixed body.
async fn serve(
    status: &'static str,
    content_type: Option<&'static str>,
    body: &'static [u8],
) -> Peer<()> {
    Peer::start(move |mut s| async move {
        read_head(&mut s).await;
        let mut headers = vec![format!("content-length: {}", body.len())];
        if let Some(ct) = content_type {
            headers.push(format!("content-type: {ct}"));
        }
        let headers: Vec<&str> = headers.iter().map(String::as_str).collect();
        write_head(&mut s, status, &headers).await;
        s.write_all(body).await.unwrap();
        until_closed(&mut s).await;
    })
    .await
}

/// A non-2xx status and a head the reader refuses are opening errors that keep the response
/// unread: its body is still there to read (a provider's error document).
#[tokio::test]
async fn opening_refusals_keep_the_body_unread() {
    for (status, content_type, kind) in [
        (
            "503 Service Unavailable",
            Some("text/event-stream"),
            OpenErrorKind::Status,
        ),
        ("200 OK", Some("application/json"), OpenErrorKind::Metadata),
        ("200 OK", None, OpenErrorKind::Metadata),
        (
            "200 OK",
            Some("text/event-stream; charset=latin1"),
            OpenErrorKind::Metadata,
        ),
    ] {
        let body: &'static [u8] = b"{\"error\":\"unavailable\"}";
        let peer = serve(status, content_type, body).await;
        let client = client();
        let response = client.get(peer.url()).send().await.unwrap();
        let operation = Operation::label("live_feed");
        let error = SseReader::default()
            .open((response, operation))
            .unwrap_err();
        assert_eq!(error.kind(), kind, "{status} {content_type:?}");
        assert_eq!(error.operation(), Some("live_feed"));
        assert!(error.to_string().starts_with("live_feed: "), "{error}");
        assert_eq!(error.into_response().bytes().await.unwrap(), body);
        drop(client);
        peer.join().await;
    }
}

/// A connection lost in the middle of an event is a terminal `Transport` error naming the
/// operation, after the events delivered before it; the URL (here with a synthetic secret in its
/// path) appears nowhere, not even in the unredacted source.
#[tokio::test]
async fn a_connection_lost_mid_event_is_a_transport_error() {
    const PREFIX: &[u8] = b"data: one\n\nevent: update\ndata: {\"partial\":";
    let peer = Peer::start(|mut s| async move {
        read_head(&mut s).await;
        write_head(&mut s, "200 OK", &SSE_CHUNKED).await;
        write_chunk(&mut s, PREFIX).await;
        // Close without the terminating chunk.
    })
    .await;
    let client = client();
    let url = format!("{}{SECRET}/events", peer.url());
    let response = client.get(&url).send().await.unwrap();
    let mut events = SseReader::new(SseOptions::new())
        .open((response, Operation::label("live_feed")))
        .unwrap();
    assert_eq!(events.next().await.unwrap().unwrap().data(), "one");
    let error = events.next().await.unwrap().unwrap_err();
    assert_eq!(error.kind(), ReadErrorKind::Transport);
    assert!(error.is_terminal());
    assert_eq!(error.operation(), Some("live_feed"));
    assert_eq!(error.position().records, 1);
    assert_eq!(error.position().bytes, PREFIX.len() as u64);
    assert_eq!(
        error.to_string(),
        format!(
            "live_feed: reading failed: reading the response body failed (after 1 records, {} bytes)",
            PREFIX.len()
        )
    );
    let source = error.source_unredacted().expect("the reqwest error");
    let shown = format!("{error:?} {source} {source:?} {events:?}");
    assert!(!shown.contains(SECRET), "{shown}");
    assert!(!shown.contains("partial"), "{shown}");
    assert!(events.next().await.is_none(), "fused");
    assert_eq!(events.completion(), Completion::Failed);
    drop(events);
    peer.join().await;
}

/// Read errors after opening name the operation and show labels and numbers, never the body.
#[tokio::test]
async fn read_diagnostics_never_show_event_content() {
    let peer = serve(
        "200 OK",
        Some("text/event-stream"),
        b"data: SECRET-7f3a91\n\ndata: SECRET-7f3a91\xFF\n\n",
    )
    .await;
    let client = client();
    let response = client.get(peer.url()).send().await.unwrap();
    let mut events = SseReader::default()
        .open((response, Operation::label("live_feed")))
        .unwrap();
    assert_eq!(events.next().await.unwrap().unwrap().data(), SECRET);
    let error = events.next().await.unwrap().unwrap_err();
    let shown = format!("{error} | {error:?}");
    assert_eq!(error.kind(), ReadErrorKind::Format);
    assert!(
        shown.starts_with("live_feed: malformed input: a line is not valid UTF-8"),
        "{shown}"
    );
    assert!(!shown.contains("7f3a91"), "{shown}");
    drop(events);
    drop(client);
    peer.join().await;
}

/// Dropping the events while a `next()` awaits data closes the connection promptly: the server
/// sees it closed, with no task left reading on the reader's behalf.
#[tokio::test]
async fn drop_while_awaiting_data_releases_the_connection() {
    let peer = Peer::start(|mut s| async move {
        read_head(&mut s).await;
        write_head(&mut s, "200 OK", &SSE_CHUNKED).await;
        write_chunk(&mut s, b"data: one\n\ndata: tw").await;
        until_closed(&mut s).await
    })
    .await;
    let client = client();
    let response = client.get(peer.url()).send().await.unwrap();
    let mut events = SseReader::default().open(response).unwrap();
    assert_eq!(events.next().await.unwrap().unwrap().data(), "one");
    let started = Instant::now();
    assert!(
        timeout(Duration::from_millis(50), events.next())
            .await
            .is_err(),
        "waiting for data that never comes"
    );
    assert!(started.elapsed() < Duration::from_secs(2));
    let dropped = Instant::now();
    drop(events);
    peer.join().await;
    let closed = dropped.elapsed();
    assert!(
        closed < Duration::from_secs(5),
        "the server saw the close after {closed:?}"
    );
    drop(client);
}

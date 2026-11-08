//! The server's own resource lifecycle: owned connection tasks, observable cleanup, bounded
//! repeated setup/use/shutdown, and the body behaviours the SDK tests rely on.

use std::time::Duration;

use bytes::Bytes;
use concord_example_testserver::{Body, Gate, Receive, Reply, TestServer, generated_records};
use tokio::io::AsyncWriteExt;

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap()
}

const WAIT: Duration = Duration::from_secs(5);

#[tokio::test]
async fn shutdown_closes_kept_alive_connections_and_waits_for_them() {
    let server = TestServer::start(|_| Reply::json(200, "{}")).await;
    let http = client();
    for _ in 0..3 {
        let r = http
            .get(format!("{}/a", server.url()))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        r.bytes().await.unwrap();
    }
    // One kept-alive connection served the three requests.
    assert_eq!(server.connections(), 1);
    assert!(server.wait_until(WAIT, |s| s.live_connections() == 1).await);
    let cleanup = server.shutdown().await;
    assert_eq!(cleanup.connections, 1);
    assert_eq!(cleanup.open_at_shutdown, 1);
    assert_eq!(
        cleanup.open_after, 0,
        "shutdown returned with a connection still open"
    );
}

#[tokio::test]
async fn a_stalled_body_does_not_block_shutdown() {
    let server = TestServer::start(|_| {
        Reply::new(200)
            .header("content-type", "text/plain")
            .with_body(Body::Stall)
    })
    .await;
    let http = client();
    let response = http.get(server.url()).send().await.unwrap();
    assert!(server.wait_until(WAIT, |s| s.active_bodies() == 1).await);
    let cleanup = tokio::time::timeout(WAIT, server.shutdown())
        .await
        .expect("shutdown must not wait for a stalled body");
    assert_eq!(cleanup.open_after, 0);
    // The client sees the connection end instead of a body.
    let mut response = response;
    assert!(response.chunk().await.is_err());
}

/// Bounded repetition of the lifecycle the provider tests perform (start, keep-alive requests,
/// shutdown), checking every cycle that the server's sockets are released before the next one
/// starts. On native Windows this is the purposeful resource exercise for the fixture: each cycle
/// binds a new ephemeral listener and opens a client connection.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn repeated_setup_use_shutdown_releases_every_connection() {
    const CYCLES: usize = 150;
    let http = client();
    let mut total_connections = 0;
    for cycle in 0..CYCLES {
        let server = TestServer::start(move |r| {
            Reply::json(200, format!("{{\"c\":{cycle},\"p\":\"{}\"}}", r.path()))
        })
        .await;
        for i in 0..2 {
            let r = http
                .get(format!("{}/item/{i}", server.url()))
                .send()
                .await
                .unwrap();
            assert_eq!(r.status(), 200);
            r.bytes().await.unwrap();
        }
        let cleanup = server.shutdown().await;
        assert_eq!(cleanup.open_after, 0, "cycle {cycle}");
        total_connections += cleanup.connections;
    }
    // A cycle normally reuses one connection for both requests; a pooled connection to the
    // previous (closed) server is never reused for another port.
    assert!(
        (CYCLES..=2 * CYCLES).contains(&total_connections),
        "{total_connections}"
    );
}

#[tokio::test]
async fn dropping_without_shutdown_aborts_connection_tasks() {
    let server = TestServer::start(|_| Reply::new(200).with_body(Body::Stall)).await;
    let http = client();
    let mut response = http.get(server.url()).send().await.unwrap();
    assert!(server.wait_until(WAIT, |s| s.active_bodies() == 1).await);
    drop(server);
    // The aborted connection task drops its socket: the client's pending read fails promptly.
    let read = tokio::time::timeout(WAIT, response.chunk())
        .await
        .expect("no prompt close");
    assert!(read.is_err());
}

#[tokio::test]
async fn fail_after_delivers_a_prefix_then_drops_the_connection() {
    let server = TestServer::start(|_| {
        Reply::new(200).with_body(Body::FailAfter(vec![
            Bytes::from_static(b"one\n"),
            Bytes::from_static(b"two\n"),
        ]))
    })
    .await;
    let mut response = client().get(server.url()).send().await.unwrap();
    let mut got = Vec::new();
    let error = loop {
        match response.chunk().await {
            Ok(Some(chunk)) => got.extend_from_slice(&chunk),
            Ok(None) => panic!("the body ended cleanly"),
            Err(e) => break e,
        }
    };
    assert_eq!(got, b"one\ntwo\n");
    assert!(
        error.is_body() || error.is_decode() || error.is_request(),
        "{error:?}"
    );
    server.shutdown().await;
}

#[tokio::test]
async fn a_generated_body_is_streamed_without_being_materialized() {
    // 200 000 records of 32 bytes, produced in 8 KiB chunks only as the socket accepts them.
    let server = TestServer::start(|_| {
        Reply::new(200).with_body(Body::Generate(
            generated_records(200_000, 8 * 1024, |i| format!("{i:031}\n").into_bytes()),
            None,
        ))
    })
    .await;
    let mut response = client().get(server.url()).send().await.unwrap();
    let mut first = response.chunk().await.unwrap().unwrap();
    // The first chunk arrives while most of the body has not been generated yet.
    assert!(first.len() <= 64 * 1024);
    assert!(server.body_bytes_sent() < 6_400_000);
    let mut total = first.len() as u64;
    while let Some(chunk) = response.chunk().await.unwrap() {
        total += chunk.len() as u64;
        first = chunk;
    }
    let _ = first;
    assert_eq!(total, 200_000 * 32);
    server.shutdown().await;
}

#[tokio::test]
async fn a_slow_receiver_counts_every_request_byte_without_storing_them_all() {
    let receive = Receive {
        record_limit: 1024,
        read_size: 4096,
        read_delay: Some(Duration::from_millis(1)),
    };
    let server = TestServer::start_with(receive, |r| {
        Reply::json(200, format!("{{\"n\":{}}}", r.body_len))
    })
    .await;
    let body = vec![b'x'; 256 * 1024];
    let text = client()
        .post(server.url())
        .body(body)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(text, "{\"n\":262144}");
    let request = server.single();
    assert_eq!(request.body.len(), 1024);
    assert_eq!(request.body_len, 256 * 1024);
    assert_eq!(server.request_body_bytes(), 256 * 1024);
    server.shutdown().await;
}

#[tokio::test]
async fn a_chunk_cut_off_by_a_closed_connection_is_received_but_never_complete() {
    let server = TestServer::start(|_| Reply::new(204)).await;
    let mut stream = tokio::net::TcpStream::connect(server.addr()).await.unwrap();
    // One complete 4-byte chunk, then 3 of the 8 bytes the next chunk announces; the client then
    // closes the connection without finishing the body.
    stream
        .write_all(
            b"POST /upload HTTP/1.1\r\nhost: loopback\r\ntransfer-encoding: chunked\r\n\r\n\
              4\r\nabcd\r\n8\r\nefg",
        )
        .await
        .unwrap();
    drop(stream);
    assert!(
        server
            .wait_until(WAIT, |s| s.connections() == 1 && s.live_connections() == 0)
            .await
    );
    // Every byte that arrived is received; only the whole chunk is complete.
    assert_eq!(server.request_body_bytes(), 7);
    assert_eq!(server.request_chunk_bytes(), 4);
    assert!(server.requests().is_empty(), "the request never completed");
    server.shutdown().await;
}

const CHUNKED_HEAD: &[u8] =
    b"POST /upload HTTP/1.1\r\nhost: loopback\r\ntransfer-encoding: chunked\r\n\r\n";

/// Sends `bytes` on a new connection, closes it, and waits until the server has closed its side.
async fn send_and_close(server: &TestServer, bytes: &[u8]) {
    let before = server.connections();
    let mut stream = tokio::net::TcpStream::connect(server.addr()).await.unwrap();
    stream.write_all(bytes).await.unwrap();
    drop(stream);
    assert!(
        server
            .wait_until(WAIT, |s| s.connections() == before + 1
                && s.live_connections() == 0)
            .await
    );
}

#[tokio::test]
async fn a_chunk_whose_crlf_never_arrives_is_received_but_never_complete() {
    let server = TestServer::start(|_| Reply::new(204)).await;
    send_and_close(&server, &[CHUNKED_HEAD, b"4\r\nabcd"].concat()).await;
    assert_eq!(server.request_body_bytes(), 4);
    assert_eq!(server.request_chunk_bytes(), 0);
    assert!(server.requests().is_empty(), "the request never completed");
    server.shutdown().await;
}

#[tokio::test]
async fn a_chunk_ended_by_other_bytes_than_crlf_is_refused_and_never_complete() {
    let server = TestServer::start(|_| Reply::new(204)).await;
    // Valid framing would follow ("0\r\n\r\n"), but the data is ended by "XY", not CRLF.
    send_and_close(&server, &[CHUNKED_HEAD, b"4\r\nabcdXY0\r\n\r\n"].concat()).await;
    assert_eq!(server.request_body_bytes(), 4);
    assert_eq!(server.request_chunk_bytes(), 0);
    assert!(
        server.requests().is_empty(),
        "a malformed body is not a request"
    );
    server.shutdown().await;
}

#[tokio::test]
async fn a_crlf_split_across_reads_completes_the_chunk() {
    // One byte per body read: once the head is parsed, CR and LF arrive in different reads.
    let receive = Receive {
        read_size: 1,
        ..Receive::default()
    };
    let server = TestServer::start_with(receive, |_| Reply::new(204)).await;
    let mut stream = tokio::net::TcpStream::connect(server.addr()).await.unwrap();
    // The head and half of the data first, so the rest of the body goes through body reads.
    stream
        .write_all(&[CHUNKED_HEAD, b"4\r\nab"].concat())
        .await
        .unwrap();
    assert!(
        server
            .wait_until(WAIT, |s| s.request_body_bytes() == 2)
            .await
    );
    stream.write_all(b"cd\r\n0\r\n\r\n").await.unwrap();
    assert!(server.wait_until(WAIT, |s| s.requests().len() == 1).await);
    assert_eq!(server.request_chunk_bytes(), 4);
    assert_eq!(server.single().body, b"abcd");
    drop(stream);
    server.shutdown().await;
}

/// An error and its sources, for assertion messages.
fn causes(error: reqwest::Error) -> String {
    let mut text = error.to_string();
    let mut source = std::error::Error::source(&error);
    while let Some(cause) = source {
        text.push_str(&format!(": {cause}"));
        source = cause.source();
    }
    text
}

#[tokio::test]
async fn a_failed_accept_neither_aborts_connections_nor_stops_accepting() {
    let gate = Gate::new();
    let held = gate.clone();
    let server = TestServer::start(move |r| match r.path() {
        "/held" => Reply::json(200, r#"{"held":true}"#).after(held.clone()),
        _ => Reply::json(200, r#"{"next":true}"#),
    })
    .await;
    // An accepted connection whose answer is still to come.
    let url = format!("{}/held", server.url());
    let in_flight = tokio::spawn(async move { client().get(url).send().await?.text().await });
    assert!(server.wait_until(WAIT, |s| s.requests().len() == 1).await);
    // The operating system fails one accept (a queued connection reset by its peer, say).
    server.fail_next_accept(std::io::ErrorKind::ConnectionReset);
    assert!(server.wait_until(WAIT, |s| s.accept_errors() == 1).await);
    // A new connection is still accepted and answered, and the held answer still arrives.
    let next = client()
        .get(format!("{}/next", server.url()))
        .send()
        .await
        .map(|r| r.status());
    gate.open();
    let held = in_flight.await.unwrap();
    assert_eq!(
        (next.map_err(causes), held.map_err(causes)),
        (
            Ok(reqwest::StatusCode::OK),
            Ok(r#"{"held":true}"#.to_owned())
        ),
        "(the new connection, the held connection)"
    );
    assert_eq!(server.connections(), 2);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test(start_paused = true)]
async fn failed_accepts_in_a_row_back_off_instead_of_spinning() {
    let server = TestServer::start(|_| Reply::json(200, "{}")).await;
    let started = tokio::time::Instant::now();
    for _ in 0..8 {
        server.fail_next_accept(std::io::ErrorKind::Other);
    }
    assert!(server.wait_until(WAIT, |s| s.accept_errors() == 8).await);
    // No pause after the first failure, then 1, 2, 4, 8, 16 and 32 ms before the next attempts.
    assert!(
        started.elapsed() >= Duration::from_millis(63),
        "{:?}",
        started.elapsed()
    );
    // The listener is still usable afterwards (real time again for the loopback exchange).
    tokio::time::resume();
    let status = client().get(server.url()).send().await.unwrap().status();
    assert_eq!(status, reqwest::StatusCode::OK);
    assert_eq!(server.shutdown().await.open_after, 0);
}

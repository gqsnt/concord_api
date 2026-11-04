//! The server's own resource lifecycle: owned connection tasks, observable cleanup, bounded
//! repeated setup/use/shutdown, and the body behaviours the SDK tests rely on.

use std::time::Duration;

use bytes::Bytes;
use concord_example_testserver::{Body, Receive, Reply, TestServer, generated_records};

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
        let r = http.get(format!("{}/a", server.url())).send().await.unwrap();
        assert_eq!(r.status(), 200);
        r.bytes().await.unwrap();
    }
    // One kept-alive connection served the three requests.
    assert_eq!(server.connections(), 1);
    assert!(server.wait_until(WAIT, |s| s.live_connections() == 1).await);
    let cleanup = server.shutdown().await;
    assert_eq!(cleanup.connections, 1);
    assert_eq!(cleanup.open_at_shutdown, 1);
    assert_eq!(cleanup.open_after, 0, "shutdown returned with a connection still open");
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
        let server = TestServer::start(move |r| Reply::json(200, format!("{{\"c\":{cycle},\"p\":\"{}\"}}", r.path()))).await;
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
    assert!(total_connections >= CYCLES && total_connections <= 2 * CYCLES, "{total_connections}");
}

#[tokio::test]
async fn dropping_without_shutdown_aborts_connection_tasks() {
    let server = TestServer::start(|_| Reply::new(200).with_body(Body::Stall)).await;
    let http = client();
    let mut response = http.get(server.url()).send().await.unwrap();
    assert!(server.wait_until(WAIT, |s| s.active_bodies() == 1).await);
    drop(server);
    // The aborted connection task drops its socket: the client's pending read fails promptly.
    let read = tokio::time::timeout(WAIT, response.chunk()).await.expect("no prompt close");
    assert!(read.is_err());
}

#[tokio::test]
async fn fail_after_delivers_a_prefix_then_drops_the_connection() {
    let server = TestServer::start(|_| {
        Reply::new(200).with_body(Body::FailAfter(vec![Bytes::from_static(b"one\n"), Bytes::from_static(b"two\n")]))
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
    assert!(error.is_body() || error.is_decode() || error.is_request(), "{error:?}");
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
    let server = TestServer::start_with(receive, |r| Reply::json(200, format!("{{\"n\":{}}}", r.body_len))).await;
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

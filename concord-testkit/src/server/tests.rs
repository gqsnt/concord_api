//! The server's own code over in-memory I/O: no socket, so the clock can be paused and
//! the bytes can be cut where a test wants. No test sleeps for real.

use super::*;
use std::future::Future;
use tokio::io::{duplex, DuplexStream};
use tokio::time::{timeout, Instant};

const SECOND: Duration = Duration::from_secs(1);

/// A server that listens nowhere: connections are attached by hand.
fn memory() -> Server {
    Server {
        addr: SocketAddr::from(([127, 0, 0, 1], 0)),
        shared: Arc::default(),
        accept: tokio::spawn(std::future::pending()),
    }
}

/// A connection to `server`: the client's end, and the task that serves it.
fn attach(server: &Server) -> (DuplexStream, JoinHandle<()>) {
    let (client, stream) = duplex(256 * 1024);
    let task = tokio::spawn(serve(stream, Arc::clone(&server.shared)));
    (client, task)
}

/// Send `bytes`, end the sending side, read until the server closes.
async fn exchange(server: &Server, bytes: &[u8]) -> Vec<u8> {
    let (mut client, task) = attach(server);
    client.write_all(bytes).await.unwrap();
    client.shutdown().await.unwrap();
    let mut out = Vec::new();
    client.read_to_end(&mut out).await.unwrap();
    task.await.unwrap();
    out
}

async fn read_head(io: &mut DuplexStream) -> Vec<u8> {
    let mut head = Vec::new();
    while !head.ends_with(b"\r\n\r\n") {
        head.push(io.read_u8().await.unwrap());
    }
    head
}

async fn read_text(io: &mut DuplexStream, length: usize) -> String {
    let mut bytes = vec![0; length];
    io.read_exact(&mut bytes).await.unwrap();
    String::from_utf8(bytes).unwrap()
}

/// Poll a future once, with a waker that does nothing.
fn poll_once<F: Future + Unpin>(future: &mut F) -> Option<F::Output> {
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    match std::pin::Pin::new(future).poll(&mut cx) {
        std::task::Poll::Ready(output) => Some(output),
        std::task::Poll::Pending => None,
    }
}

#[tokio::test]
async fn a_request_is_recorded_when_its_head_arrives_and_a_broken_body_stays() {
    let server = memory();
    server.on("POST", "/lost").reply(204);
    let (mut client, task) = attach(&server);
    client
        .write_all(b"POST /lost HTTP/1.1\r\nHost: x\r\nContent-Length: 3\r\n\r\na")
        .await
        .unwrap();
    let request = server.next_request().await;
    assert_eq!(request.target(), "/lost");
    assert_eq!(request.body_state(), BodyState::Coming);

    // The client goes away in the middle of its body.
    drop(client);
    task.await.unwrap();
    let requests = server.requests();
    assert_eq!(
        requests.len(),
        1,
        "a request whose head arrived stays recorded"
    );
    assert_eq!(requests[0].body(), b"a");
    assert_eq!(requests[0].body_state(), BodyState::Broken);
    assert_eq!(server.unused(), 1, "no rule answered a broken request");
}

#[tokio::test]
async fn a_slow_upload_keeps_the_place_of_its_head() {
    let server = memory();
    server.on("POST", "/slow").reply(204);
    server.on("GET", "/fast").reply(204);
    let (mut slow, task) = attach(&server);
    slow.write_all(b"POST /slow HTTP/1.1\r\nHost: x\r\nContent-Length: 3\r\n\r\na")
        .await
        .unwrap();
    assert_eq!(server.next_request().await.target(), "/slow");

    exchange(
        &server,
        b"GET /fast HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert_eq!(server.next_request().await.target(), "/fast");
    assert_eq!(server.requests()[0].body_state(), BodyState::Coming);

    slow.write_all(b"bc").await.unwrap();
    assert!(read_head(&mut slow).await.starts_with(b"HTTP/1.1 204 "));
    let requests = server.requests();
    let targets: Vec<&str> = requests.iter().map(Request::target).collect();
    assert_eq!(targets, ["/slow", "/fast"]);
    assert_eq!(requests[0].body(), b"abc");
    assert_eq!(requests[0].body_state(), BodyState::Ended);
    assert_eq!(requests[1].body_state(), BodyState::Ended);
    task.abort();
}

#[tokio::test(start_paused = true)]
async fn expect_100_continue_is_answered_before_the_body() {
    let server = memory();
    server.on("POST", "/up").reply(201);
    server.on("GET", "/plain").reply(200);
    let (mut client, task) = attach(&server);
    client
        .write_all(
            b"POST /up HTTP/1.1\r\nHost: x\r\nContent-Length: 3\r\nExpect: 100-Continue\r\n\r\n",
        )
        .await
        .unwrap();
    let interim = timeout(30 * SECOND, read_head(&mut client)).await;
    assert_eq!(interim.unwrap(), b"HTTP/1.1 100 Continue\r\n\r\n");
    client.write_all(b"abc").await.unwrap();
    assert!(read_head(&mut client).await.starts_with(b"HTTP/1.1 201 "));
    assert_eq!(server.requests()[0].body(), b"abc");

    // Without a body, no interim answer: the final one comes at once.
    client
        .write_all(b"GET /plain HTTP/1.1\r\nHost: x\r\nExpect: 100-continue\r\n\r\n")
        .await
        .unwrap();
    assert!(read_head(&mut client).await.starts_with(b"HTTP/1.1 200 "));
    task.abort();
}

#[tokio::test(start_paused = true)]
async fn the_limit_of_a_head_does_not_depend_on_where_a_packet_ends() {
    let mut head = b"GET / HTTP/1.1\r\nHost: x\r\nX-Pad: ".to_vec();
    head.resize(MAX_HEAD, b'a');

    // A head of exactly `MAX_HEAD` bytes, its delimiter cut after the first CR.
    let mut first = head.clone();
    first.push(b'\r');
    let (stream, mut peer) = duplex(1024);
    let mut connection = Connection {
        stream,
        buf: BytesMut::from(&first[..]),
    };
    peer.write_all(b"\n\r\n").await.unwrap();
    let request = connection.read_head().await;
    assert!(matches!(request, Ok(Some(_))), "a head of MAX_HEAD bytes");

    // One byte more is too long, wherever the packet ends.
    let mut longer = head.clone();
    longer.extend_from_slice(b"a\r");
    let (stream, mut peer) = duplex(1024);
    let mut connection = Connection {
        stream,
        buf: BytesMut::from(&longer[..]),
    };
    peer.write_all(b"\n\r\n").await.unwrap();
    assert!(matches!(connection.read_head().await, Err(Bad::Malformed)));

    // A buffer that cannot hold a head of MAX_HEAD bytes is refused without waiting.
    let mut garbage = head;
    garbage.extend_from_slice(b"aaaa");
    let (stream, _peer) = duplex(1024);
    let mut connection = Connection {
        stream,
        buf: BytesMut::from(&garbage[..]),
    };
    let result = timeout(SECOND, connection.read_head()).await;
    assert!(matches!(result, Ok(Err(Bad::Malformed))));
}

#[tokio::test(start_paused = true)]
async fn the_limit_of_a_chunk_line_does_not_depend_on_where_a_packet_ends() {
    let shared = Shared::default();
    let mut line = b"1;pad=".to_vec();
    line.resize(MAX_LINE, b'a');
    line.push(b'\r');
    let (stream, mut peer) = duplex(1024);
    let mut connection = Connection {
        stream,
        buf: BytesMut::from(&line[..]),
    };
    peer.write_all(b"\nx\r\n0\r\n\r\n").await.unwrap();
    assert!(connection.read_chunked(&shared, 0).await.is_ok());

    let mut longer = line;
    longer.insert(10, b'a');
    let (stream, mut peer) = duplex(1024);
    let mut connection = Connection {
        stream,
        buf: BytesMut::from(&longer[..]),
    };
    peer.write_all(b"\nx\r\n0\r\n\r\n").await.unwrap();
    assert!(matches!(
        connection.read_chunked(&shared, 0).await,
        Err(Bad::Malformed)
    ));
}

#[tokio::test]
async fn the_record_keeps_each_header_and_trailer_line_as_sent() {
    let server = memory();
    server.on("POST", "/up").reply(204);
    exchange(
        &server,
        b"POST /up HTTP/1.1\r\nHost: x\r\nX-Ows:\t  a \t \r\nTransfer-Encoding: chunked\r\n\r\n\
          2\r\nab\r\n0;final=yes\r\nX-Check:  123 \r\nx-sum: 9\r\n\r\n",
    )
    .await;
    let request = &server.requests()[0];
    let raw: Vec<&[u8]> = request.raw_headers().iter().map(|line| &line[..]).collect();
    assert_eq!(
        raw,
        [
            &b"Host: x"[..],
            b"X-Ows:\t  a \t ",
            b"Transfer-Encoding: chunked",
        ]
    );
    assert_eq!(
        request.header("x-ows"),
        Some("a"),
        "the tidy accessor stays"
    );
    let raw: Vec<&[u8]> = request
        .raw_trailers()
        .iter()
        .map(|line| &line[..])
        .collect();
    assert_eq!(raw, [&b"X-Check:  123 "[..], b"x-sum: 9"]);
    let trailers: Vec<(&str, &[u8])> = request
        .trailers()
        .iter()
        .map(|(name, value)| (name.as_str(), &value[..]))
        .collect();
    assert_eq!(trailers, [("X-Check", &b"123"[..]), ("x-sum", b"9")]);
    assert_eq!(request.body(), b"ab");
}

#[tokio::test]
async fn header_gives_the_first_value_even_when_it_is_not_utf8() {
    let server = memory();
    server.on("GET", "/").reply(204);
    exchange(
        &server,
        b"GET / HTTP/1.1\r\nHost: x\r\nX-Dup: \xff\r\nX-Dup: second\r\n\r\n",
    )
    .await;
    let request = &server.requests()[0];
    assert_eq!(request.header("x-dup"), None);
    assert_eq!(request.header_all("x-dup"), ["second"]);
}

#[tokio::test(start_paused = true)]
async fn a_pause_between_pieces() {
    let server = memory();
    server
        .on("GET", "/slow")
        .reply(200)
        .body("a")
        .pause(7 * SECOND)
        .body("b");
    let (mut client, task) = attach(&server);
    client
        .write_all(b"GET /slow HTTP/1.1\r\nHost: x\r\n\r\n")
        .await
        .unwrap();
    assert!(read_head(&mut client).await.starts_with(b"HTTP/1.1 200 "));
    assert_eq!(read_text(&mut client, 1).await, "a");
    let start = Instant::now();
    assert!(
        timeout(6 * SECOND, client.read_u8()).await.is_err(),
        "b came before its pause"
    );
    assert_eq!(read_text(&mut client, 1).await, "b");
    assert_eq!(start.elapsed(), 7 * SECOND);
    task.abort();
}

#[tokio::test(start_paused = true)]
async fn a_gate_holds_what_follows_in_the_body() {
    let server = memory();
    let gate = Gate::new();
    server
        .on("GET", "/feed")
        .reply(200)
        .body("first ")
        .wait(&gate)
        .body("second");
    let (mut client, task) = attach(&server);
    client
        .write_all(b"GET /feed HTTP/1.1\r\nHost: x\r\n\r\n")
        .await
        .unwrap();
    read_head(&mut client).await;
    assert_eq!(read_text(&mut client, 6).await, "first ");
    assert!(
        timeout(3600 * SECOND, client.read_u8()).await.is_err(),
        "the body went on with the gate closed"
    );
    gate.open();
    assert_eq!(read_text(&mut client, 6).await, "second");
    task.abort();
}

#[tokio::test(start_paused = true)]
async fn the_head_can_wait_for_a_time_and_for_a_gate() {
    let server = memory();
    let gate = Gate::new();
    server
        .on("GET", "/held")
        .reply(200)
        .after(5 * SECOND)
        .after_gate(&gate)
        .body("ok");
    let (mut client, task) = attach(&server);
    client
        .write_all(b"GET /held HTTP/1.1\r\nHost: x\r\n\r\n")
        .await
        .unwrap();
    let start = Instant::now();
    assert!(timeout(3600 * SECOND, client.read_u8()).await.is_err());
    assert_eq!(start.elapsed(), 3600 * SECOND);
    gate.open();
    assert!(read_head(&mut client).await.starts_with(b"HTTP/1.1 200 "));
    assert_eq!(read_text(&mut client, 2).await, "ok");

    // The time alone.
    server.on("GET", "/late").reply(204).after(5 * SECOND);
    client
        .write_all(b"GET /late HTTP/1.1\r\nHost: x\r\n\r\n")
        .await
        .unwrap();
    let start = Instant::now();
    assert!(timeout(4 * SECOND, client.read_u8()).await.is_err());
    assert!(read_head(&mut client).await.starts_with(b"HTTP/1.1 204 "));
    assert_eq!(start.elapsed(), 5 * SECOND);
    task.abort();
}

#[tokio::test]
async fn a_status_with_no_body_stays_with_no_body() {
    for (status, reason) in [
        (100, "Continue"),
        (204, "No Content"),
        (304, "Not Modified"),
    ] {
        for scripted in ["declared", "chunked"] {
            let server = memory();
            let reply = server.on("GET", "/").reply(status).body("BAD");
            drop(match scripted {
                "declared" => reply.declare_length(3),
                _ => reply.chunked(),
            });
            let out = exchange(&server, b"GET / HTTP/1.1\r\nHost: x\r\n\r\n").await;
            let expected = format!("HTTP/1.1 {status} {reason}\r\n\r\n");
            assert_eq!(out, expected.as_bytes(), "{status} {scripted}");
        }

        // Unless the test asks for a malformed answer.
        let server = memory();
        server
            .on("GET", "/")
            .reply(status)
            .declare_length(3)
            .body("BAD")
            .malformed_with_body();
        let out = exchange(&server, b"GET / HTTP/1.1\r\nHost: x\r\n\r\n").await;
        let expected = format!("HTTP/1.1 {status} {reason}\r\ncontent-length: 3\r\n\r\nBAD");
        assert_eq!(out, expected.as_bytes());
    }
}

#[tokio::test]
async fn next_request_waits_for_a_request_and_gives_each_once() {
    let server = memory();
    let mut waiting = Box::pin(server.next_request());
    let (mut client, task) = attach(&server);
    tokio::task::yield_now().await;
    assert!(poll_once(&mut waiting).is_none(), "nothing has arrived");
    client
        .write_all(b"GET /one HTTP/1.1\r\nHost: x\r\n\r\nGET /two HTTP/1.1\r\nHost: x\r\n\r\n")
        .await
        .unwrap();
    assert_eq!(waiting.await.target(), "/one");
    assert_eq!(server.next_request().await.target(), "/two");
    task.abort();
}

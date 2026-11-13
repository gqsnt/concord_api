//! The tests of `Server`: a plain `reqwest::Client` (and raw bytes, where `reqwest` cannot
//! send what is checked) against it. No test sleeps: each waits on an event.

use concord_core::reqwest;
use concord_testkit::server::BodyState;
use concord_testkit::{Gate, Server};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

fn client() -> reqwest::Client {
    reqwest::Client::builder().no_proxy().build().unwrap()
}

/// Send raw bytes, end the sending side, and read until the server closes.
async fn raw(server: &Server, bytes: &[u8]) -> String {
    let mut stream = TcpStream::connect(server.addr()).await.unwrap();
    stream.write_all(bytes).await.unwrap();
    stream.shutdown().await.unwrap();
    let mut answer = Vec::new();
    let _ = stream.read_to_end(&mut answer).await;
    String::from_utf8_lossy(&answer).into_owned()
}

#[tokio::test]
async fn the_url_is_local_and_dropping_the_server_stops_it() {
    let server = Server::start().await;
    let url = server.url();
    assert!(url.starts_with("http://127.0.0.1:"), "{url}");
    assert!(!url.ends_with('/'));
    server.on("GET", "/").reply(204);
    assert_eq!(client().get(&url).send().await.unwrap().status(), 204);

    drop(server);
    assert!(client().get(&url).send().await.is_err());
}

#[tokio::test]
async fn a_reply_has_its_status_headers_and_body() {
    let server = Server::start().await;
    server
        .on("GET", "/tasks")
        .reply(200)
        .header("content-type", "application/json")
        .header("x-many", "1")
        .header("x-many", "2")
        .body(r#"[]"#);

    let answer = client()
        .get(format!("{}/tasks", server.url()))
        .send()
        .await
        .unwrap();
    assert_eq!(answer.status(), 200);
    assert_eq!(answer.headers()["content-type"], "application/json");
    assert_eq!(answer.headers()["content-length"], "2");
    let many: Vec<_> = answer.headers().get_all("x-many").iter().collect();
    assert_eq!(many, ["1", "2"]);
    assert_eq!(answer.text().await.unwrap(), "[]");
    assert_eq!(server.unused(), 0);
}

#[tokio::test]
async fn replies_for_one_route_come_in_order_then_599() {
    let server = Server::start().await;
    server.on("GET", "/tasks").reply(503).body("busy");
    server.on("GET", "/tasks").reply(200).body("ok");
    assert_eq!(server.unused(), 2);
    let url = format!("{}/tasks", server.url());
    let client = client();

    let first = client.get(&url).send().await.unwrap();
    assert_eq!(first.status(), 503);
    assert_eq!(first.text().await.unwrap(), "busy");
    let second = client.get(&url).send().await.unwrap();
    assert_eq!(second.status(), 200);
    assert_eq!(second.text().await.unwrap(), "ok");
    let third = client.get(&url).send().await.unwrap();
    assert_eq!(third.status(), 599);
    assert_eq!(server.requests().len(), 3);
}

#[tokio::test]
async fn a_request_that_no_rule_matches_gets_599_and_is_recorded() {
    let server = Server::start().await;
    server.on("GET", "/tasks").reply(200);

    let answer = client()
        .post(format!("{}/tasks?x=1", server.url()))
        .send()
        .await
        .unwrap();
    assert_eq!(answer.status(), 599);
    assert_eq!(
        answer.text().await.unwrap(),
        "concord-testkit: no rule for POST /tasks?x=1\n"
    );
    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method(), "POST");
    assert_eq!(server.unused(), 1, "the GET rule is still there");
}

#[tokio::test]
async fn a_rule_with_a_query_matches_the_target_exactly() {
    let server = Server::start().await;
    server.on("GET", "/tasks?page=2").reply(200).body("two");
    server.on("GET", "/tasks").reply(200).body("any");
    let client = client();

    let page1 = client
        .get(format!("{}/tasks?page=1", server.url()))
        .send()
        .await
        .unwrap();
    assert_eq!(page1.text().await.unwrap(), "any");
    let page2 = client
        .get(format!("{}/tasks?page=2", server.url()))
        .send()
        .await
        .unwrap();
    assert_eq!(page2.text().await.unwrap(), "two");
}

#[tokio::test]
async fn every_request_is_recorded_as_sent() {
    let server = Server::start().await;
    server.on("PUT", "/a/b%2Fc").reply(204);

    let mut headers = reqwest::header::HeaderMap::new();
    headers.append("x-one", "1".parse().unwrap());
    headers.append("x-one", "2".parse().unwrap());
    headers.append("x-two", "  spaced ".parse().unwrap());
    let answer = client()
        .put(format!("{}/a/b%2Fc?q=%20x&q=y&flag", server.url()))
        .headers(headers)
        .body("the body")
        .send()
        .await
        .unwrap();
    assert_eq!(answer.status(), 204);

    let requests = server.requests();
    let request = &requests[0];
    assert_eq!(request.method(), "PUT");
    assert_eq!(request.target(), "/a/b%2Fc?q=%20x&q=y&flag");
    assert_eq!(request.path(), "/a/b%2Fc");
    assert_eq!(request.query(), Some("q=%20x&q=y&flag"));
    assert_eq!(request.version(), "HTTP/1.1");
    assert_eq!(request.header_all("X-ONE"), ["1", "2"]);
    assert_eq!(request.header("x-two"), Some("spaced"));
    assert_eq!(request.header("content-length"), Some("8"));
    assert_eq!(request.body(), b"the body");
    let names: Vec<&str> = request
        .headers()
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();
    let one = names.iter().position(|name| *name == "x-one").unwrap();
    assert_eq!(names[one + 1], "x-one", "a repeated name stays repeated");
}

#[tokio::test]
async fn headers_keep_their_order_and_case_and_a_chunked_body_is_decoded() {
    let server = Server::start().await;
    server.on("POST", "/up").reply(201).body("one");
    server.on("GET", "/next").reply(200).body("two");

    // Two requests in one write: the second waits in the buffer of the first.
    let answer = raw(
        &server,
        b"POST /up HTTP/1.1\r\nHost: x\r\nX-A: 1\r\nx-b:2\r\nX-A:  3 \r\nTransfer-Encoding: chunked\r\n\r\n\
          4;ext=1\r\nabcd\r\n2\r\nef\r\n0\r\nTrailer: t\r\n\r\n\
          GET /next HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(answer.starts_with("HTTP/1.1 201 Created\r\n"), "{answer}");
    assert!(answer.contains("\r\n\r\none"), "{answer}");
    assert!(answer.contains("HTTP/1.1 200 OK\r\n"), "{answer}");
    assert!(answer.ends_with("\r\n\r\ntwo"), "{answer}");

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    let headers: Vec<(&str, &[u8])> = requests[0]
        .headers()
        .iter()
        .map(|(name, value)| (name.as_str(), &value[..]))
        .collect();
    assert_eq!(
        headers,
        [
            ("Host", &b"x"[..]),
            ("X-A", b"1"),
            ("x-b", b"2"),
            ("X-A", b"3"),
            ("Transfer-Encoding", b"chunked"),
        ]
    );
    assert_eq!(requests[0].body(), b"abcdef");
    assert_eq!(requests[1].target(), "/next");

    // Each line also as sent, and the trailers.
    let raw: Vec<&[u8]> = requests[0]
        .raw_headers()
        .iter()
        .map(|line| &line[..])
        .collect();
    assert_eq!(
        raw,
        [
            &b"Host: x"[..],
            b"X-A: 1",
            b"x-b:2",
            b"X-A:  3 ",
            b"Transfer-Encoding: chunked",
        ]
    );
    let raw: Vec<&[u8]> = requests[0]
        .raw_trailers()
        .iter()
        .map(|line| &line[..])
        .collect();
    assert_eq!(raw, [b"Trailer: t"]);
    assert_eq!(requests[0].trailers()[0].0, "Trailer");
    assert_eq!(requests[0].body_state(), BodyState::Ended);
}

#[tokio::test]
async fn a_request_is_recorded_when_its_head_arrives() {
    let server = Server::start().await;
    server.on("POST", "/slow").reply(201);
    server.on("GET", "/fast").reply(200).body("fast");

    let mut slow = TcpStream::connect(server.addr()).await.unwrap();
    slow.write_all(b"POST /slow HTTP/1.1\r\nHost: x\r\nContent-Length: 3\r\n\r\na")
        .await
        .unwrap();
    let first = server.next_request().await;
    assert_eq!(first.target(), "/slow");
    assert_eq!(first.body_state(), BodyState::Coming);

    // A whole other request while the upload waits.
    let fast = client()
        .get(format!("{}/fast", server.url()))
        .send()
        .await
        .unwrap();
    assert_eq!(fast.text().await.unwrap(), "fast");
    assert_eq!(server.next_request().await.target(), "/fast");

    slow.write_all(b"bc").await.unwrap();
    let mut head = Vec::new();
    while !head.ends_with(b"\r\n\r\n") {
        head.push(slow.read_u8().await.unwrap());
    }
    assert!(head.starts_with(b"HTTP/1.1 201 "));
    let requests = server.requests();
    let targets: Vec<&str> = requests.iter().map(|request| request.target()).collect();
    assert_eq!(
        targets,
        ["/slow", "/fast"],
        "in the order the heads arrived"
    );
    assert_eq!(requests[0].body(), b"abc");
    assert_eq!(requests[0].body_state(), BodyState::Ended);
}

#[tokio::test]
async fn the_head_of_a_reply_waits_for_a_gate() {
    let server = Server::start().await;
    let gate = Gate::new();
    server
        .on("GET", "/held")
        .reply(200)
        .after_gate(&gate)
        .body("ok");
    server.on("GET", "/other").reply(200).body("other");
    let client = client();

    let held = client.get(format!("{}/held", server.url())).send();
    tokio::pin!(held);
    let other = async {
        // The held request has arrived; its head waits while another goes and comes back.
        assert_eq!(server.next_request().await.target(), "/held");
        let other = client
            .get(format!("{}/other", server.url()))
            .send()
            .await
            .unwrap();
        other.text().await.unwrap()
    };
    tokio::select! {
        biased;
        held = &mut held => panic!("the head came with the gate closed: {held:?}"),
        other = other => assert_eq!(other, "other"),
    }
    gate.open();
    let held = held.await.unwrap();
    assert_eq!(held.status(), 200);
    assert_eq!(held.text().await.unwrap(), "ok");
}

#[tokio::test]
async fn bytes_that_are_not_a_request_get_400_and_the_server_goes_on() {
    let server = Server::start().await;
    for garbage in [
        &b"\x00\xff\r\n\r\n"[..],
        b"GET /\r\n\r\n",
        b"GET / HTTP/1.1\r\nno colon\r\n\r\n",
        b"GET / HTTP/1.1\r\n folded: x\r\n\r\n",
        b"GET / HTTP/1.1\r\ncontent-length: 1\r\ncontent-length: 2\r\n\r\n",
        b"GET / HTTP/1.1\r\ncontent-length: -1\r\n\r\n",
        b"GET / HTTP/1.1\r\ntransfer-encoding: gzip\r\n\r\n",
        b"POST / HTTP/1.1\r\ntransfer-encoding: chunked\r\n\r\nzz\r\n",
        b"POST / HTTP/1.1\r\ntransfer-encoding: chunked\r\n\r\n2\r\nabcd\r\n",
    ] {
        let answer = raw(&server, garbage).await;
        assert!(answer.starts_with("HTTP/1.1 400 "), "{garbage:?}: {answer}");
    }
    let too_large = format!("POST / HTTP/1.1\r\ncontent-length: {}\r\n\r\n", u64::MAX);
    assert!(raw(&server, too_large.as_bytes())
        .await
        .starts_with("HTTP/1.1 413 "));
    let huge_chunk = "POST / HTTP/1.1\r\ntransfer-encoding: chunked\r\n\r\nffffffffffffffff\r\n";
    assert!(raw(&server, huge_chunk.as_bytes())
        .await
        .starts_with("HTTP/1.1 413 "));
    let long_head = format!("GET / HTTP/1.1\r\nx: {}\r\n\r\n", "a".repeat(70 * 1024));
    assert!(raw(&server, long_head.as_bytes())
        .await
        .starts_with("HTTP/1.1 400 "));
    // A request cut in the middle, then closed.
    assert_eq!(raw(&server, b"GET / HTT").await, "");

    // P0.2-fix, decision 1: a request is recorded when its head arrives. The four whose
    // head was read (two bodies that break their chunked framing, two that are too large)
    // are recorded, their body broken; the others are not requests.
    let requests = server.requests();
    assert_eq!(requests.len(), 4);
    assert!(requests
        .iter()
        .all(|request| request.target() == "/" && request.body_state() == BodyState::Broken));
    server.on("GET", "/").reply(200).body("still here");
    let answer = client().get(server.url()).send().await.unwrap();
    assert_eq!(answer.text().await.unwrap(), "still here");
}

#[tokio::test]
async fn close_ends_the_connection_before_the_head_and_records_the_request() {
    let server = Server::start().await;
    server.on("GET", "/gone").close();

    let result = client().get(format!("{}/gone", server.url())).send().await;
    assert!(result.is_err(), "{result:?}");
    assert_eq!(server.requests()[0].target(), "/gone");
}

#[tokio::test]
async fn a_declared_length_larger_than_the_body_then_the_connection_closes() {
    let server = Server::start().await;
    server
        .on("GET", "/short")
        .reply(200)
        .declare_length(100)
        .body("only this");

    let answer = client()
        .get(format!("{}/short", server.url()))
        .send()
        .await
        .unwrap();
    assert_eq!(answer.status(), 200);
    assert_eq!(answer.content_length(), Some(100));
    let error = answer.bytes().await.unwrap_err();
    assert!(error.is_body() || error.is_decode(), "{error:?}");
}

#[tokio::test]
async fn pieces_come_apart_and_a_gate_holds_what_follows() {
    for chunked in [false, true] {
        let server = Server::start().await;
        let gate = Gate::new();
        let reply = server
            .on("GET", "/feed")
            .reply(200)
            .body("first ")
            .wait(&gate)
            .body("second");
        drop(if chunked { reply.chunked() } else { reply });
        server.on("GET", "/other").reply(200).body("other");
        let client = client();

        let mut answer = client
            .get(format!("{}/feed", server.url()))
            .send()
            .await
            .unwrap();
        let expected_length = if chunked { None } else { Some(12) };
        assert_eq!(answer.content_length(), expected_length);
        // TCP may cut a write anywhere: gather what came before the gate.
        let mut before = Vec::new();
        while before.len() < 6 {
            before.extend_from_slice(&answer.chunk().await.unwrap().unwrap());
        }
        assert_eq!(before, b"first ", "more than the first piece came");
        {
            // The gate is closed: the body gives nothing while a whole other request goes
            // and comes back.
            let chunk = answer.chunk();
            let other = async {
                let other = client
                    .get(format!("{}/other", server.url()))
                    .send()
                    .await
                    .unwrap();
                other.text().await.unwrap()
            };
            tokio::select! {
                biased;
                chunk = chunk => panic!("the body went on with the gate closed: {chunk:?}"),
                other = other => assert_eq!(other, "other"),
            }
        }
        gate.open();
        let mut after = Vec::new();
        while let Some(chunk) = answer.chunk().await.unwrap() {
            after.extend_from_slice(&chunk);
        }
        assert_eq!(after, b"second");
    }
}

// The pause between pieces is tested with the clock paused, over in-memory I/O:
// `a_pause_between_pieces` in `src/server/tests.rs`.

#[tokio::test]
async fn a_head_and_then_silence() {
    let server = Server::start().await;
    server
        .on("GET", "/silent")
        .reply(200)
        .header("x-a", "1")
        .silence();
    server.on("GET", "/other").reply(200).body("other");
    let client = client();

    let mut answer = client
        .get(format!("{}/silent", server.url()))
        .send()
        .await
        .unwrap();
    assert_eq!(answer.status(), 200);
    assert_eq!(answer.headers()["x-a"], "1");
    {
        // The body gives nothing while a whole other request goes and comes back.
        let chunk = answer.chunk();
        let other = async {
            let other = client
                .get(format!("{}/other", server.url()))
                .send()
                .await
                .unwrap();
            other.text().await.unwrap()
        };
        tokio::select! {
            biased;
            chunk = chunk => panic!("the silent body gave {chunk:?}"),
            other = other => assert_eq!(other, "other"),
        }
    }
    // Stopping the server ends the silence.
    drop(server);
    assert!(answer.chunk().await.is_err());
}

#[tokio::test]
async fn a_large_body_in_pieces() {
    const SIZE: u64 = 10 * 1024 * 1024 + 3;
    for chunked in [false, true] {
        let server = Server::start().await;
        let reply = server.on("GET", "/large").reply(200).large_body(SIZE);
        drop(if chunked { reply.chunked() } else { reply });

        let mut answer = client()
            .get(format!("{}/large", server.url()))
            .send()
            .await
            .unwrap();
        let mut total = 0u64;
        while let Some(chunk) = answer.chunk().await.unwrap() {
            assert!(chunk.iter().all(|byte| *byte == b'x'));
            total += chunk.len() as u64;
        }
        assert_eq!(total, SIZE);
    }
}

#[tokio::test]
async fn head_gets_the_head_only() {
    let server = Server::start().await;
    server.on("HEAD", "/h").reply(200).body("12345");
    server.on("GET", "/h").reply(200).body("after");
    let client = client();
    let answer = client
        .head(format!("{}/h", server.url()))
        .send()
        .await
        .unwrap();
    assert_eq!(answer.headers()["content-length"], "5");
    assert_eq!(answer.bytes().await.unwrap().len(), 0);
    // The same connection serves the next request.
    let answer = client
        .get(format!("{}/h", server.url()))
        .send()
        .await
        .unwrap();
    assert_eq!(answer.text().await.unwrap(), "after");
}

#[test]
#[should_panic(expected = "use `.declare_length(n)` or `.chunked()`")]
fn a_framing_header_is_a_mistake_in_the_script() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let server = Server::start().await;
        let _ = server
            .on("GET", "/")
            .reply(200)
            .header("Content-Length", "3");
    });
}

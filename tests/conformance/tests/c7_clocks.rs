//! C7, Concord's own client and the clocks: `concord::Http` (no redirect, no cookies, no
//! decompression), the default transport, a client of the user's, the two clocks row by
//! row (tables 7.1 to 7.3 of docs/reference/cases.md), and a drop (docs/reference/contracts.md C7;
//! docs/internals/engine.md, "The two clocks", "A drop" and "Concord's own HTTP client"; docs/reference/rules.md rules 19 and 22).
//!
//! The tests of the clocks run with Tokio's clock paused, over a `Script`; the tests of
//! `concord::Http` run on a local `Server`, in real time.
//!
//! Rows that are not here: "A reader waits for something that is not the body" (needs an
//! endpoint with a reader of the author's that waits), "Between two pages" (pages), and
//! row "Building" of table 7.2 (it is not a wait: nothing to see).

mod common;

use common::engine::*;
use concord::error::{Class, Kept, Kind};
use concord::read::StreamError;
use concord::Retry;
use concord_testkit::server::Server;
use concord_testkit::Script;
use std::future::IntoFuture;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;

/// A `Duration` long enough that the clock for silence never ends first.
const LONG: Duration = Duration::from_secs(300);

/// What a cell of tables 7.1 and 7.2 checks: the outcome, and when it came.
fn timed<T, E>(
    result: &Result<T, concord::Error<E>>,
    start: Instant,
    at: Duration,
    want: &str,
    holds: impl FnOnce(&Result<T, concord::Error<E>>) -> bool,
) -> Result<(), String> {
    check(start.elapsed() == at, || {
        format!(
            "it ended at {:?}, not {at:?}: {}",
            start.elapsed(),
            outcome(result)
        )
    })?;
    check(holds(result), || format!("{}, not {want}", outcome(result)))
}

/// [`timed`], for what a `ByteStream` gives (a `concord::read::StreamError`).
fn timed_stream<T>(
    result: &Result<T, StreamError>,
    start: Instant,
    at: Duration,
    want: &str,
    holds: impl FnOnce(&Result<T, StreamError>) -> bool,
) -> Result<(), String> {
    check(start.elapsed() == at, || {
        format!(
            "it ended at {:?}, not {at:?}: {}",
            start.elapsed(),
            stream_outcome(result)
        )
    })?;
    check(holds(result), || {
        format!("{}, not {want}", stream_outcome(result))
    })
}

fn is_timeout<T, E>(result: &Result<T, concord::Error<E>>) -> bool {
    matches!(result, Err(error) if error.is_timeout() && error.kind() == Kind::Transport)
}

// C7.1 ------------------------------------------------------------------------------

#[test]
fn c7_1_http_new_and_http_with_give_a_client() {
    assert!(concord::Http::new().is_ok());
    assert!(concord::Http::with(|builder| builder.no_proxy()).is_ok());
}

/// `Tasks` with `concord::Http` on a local server.
fn tasks_on(server: &Server) -> tour::Tasks {
    let http = concord::Http::with(|builder| builder.no_proxy()).unwrap();
    tour::Tasks::new()
        .with_transport(http)
        .with_base(format!("{}/api/tasks", server.url()))
}

#[tokio::test]
async fn c7_1_http_follows_no_redirect() {
    let server = Server::start().await;
    server
        .on("GET", "/api/tasks/7")
        .reply(302)
        .header("location", "/api/tasks/8");
    server
        .on("GET", "/api/tasks/8")
        .reply(200)
        .header("content-type", "application/json")
        .body(TASK);

    let error = failure(tasks_on(&server).get(7).await);

    assert_eq!(error.kind(), Kind::Refused);
    assert!(error.is_status(302));
    assert_eq!(error.to_string(), "refused: status 302, in tasks.get");
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn c7_1_http_follows_no_redirect_even_when_its_closure_asks_for_one() {
    let server = Server::start().await;
    server
        .on("GET", "/api/tasks/7")
        .reply(307)
        .header("location", "/api/tasks/8");
    let http = concord::Http::with(|builder| {
        builder
            .no_proxy()
            .redirect(concord::reqwest::redirect::Policy::limited(10))
    })
    .unwrap();

    let error = failure(
        tour::Tasks::new()
            .with_transport(http)
            .with_base(format!("{}/api/tasks", server.url()))
            .get(7)
            .await,
    );

    assert!(error.is_status(307));
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn c7_1_http_keeps_no_cookies() {
    let server = Server::start().await;
    server
        .on("GET", "/api/tasks/7")
        .reply(200)
        .header("content-type", "application/json")
        .header("set-cookie", "session=s-1; Path=/")
        .body(TASK);
    server
        .on("GET", "/api/tasks/7")
        .reply(200)
        .header("content-type", "application/json")
        .body(TASK);
    let tasks = tasks_on(&server);

    success(tasks.get(7).await);
    success(tasks.get(7).await);

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1].header("cookie"), None);
}

#[tokio::test]
async fn c7_1_http_does_not_decompress() {
    let server = Server::start().await;
    server
        .on("GET", "/repo/readme")
        .reply(200)
        .header("content-encoding", "gzip")
        .body("as it came");
    let http = concord::Http::with(|builder| builder.no_proxy()).unwrap();

    let text = success(
        tour::Files::new("key")
            .with_transport(http)
            .with_base(server.url())
            .readme("repo")
            .await,
    );

    assert_eq!(text, "as it came");
}

#[tokio::test(start_paused = true)]
async fn c7_1_http_sets_30_seconds_to_connect() {
    // C7.1: "`Http::with(f)` sets 30 seconds to connect [...] `Http::new()` is
    // `Http::with(|b| b)`". A server takes the connection and never answers the TLS
    // handshake; the clock for silence is set longer, so only the connect timeout can end it.
    use std::io::Read;
    use std::sync::atomic::{AtomicBool, Ordering};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("https://{}/api/tasks", listener.local_addr().unwrap());
    let hello = Arc::new(AtomicBool::new(false));
    let received = hello.clone();
    let (release, hold) = std::sync::mpsc::channel::<()>();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream.read_exact(&mut [0]).unwrap();
        received.store(true, Ordering::SeqCst);
        // Hold the connection open, and say nothing.
        let _ = hold.recv();
    });
    let call = tour::Tasks::new()
        .with_transport(concord::Http::new().unwrap())
        .with_base(url)
        .get(7)
        .with_idle_timeout(secs(300));
    let mut sending = Box::pin(call.into_future());
    // Poll until the server has the first byte of the handshake, without letting the paused
    // clock move.
    while !hello.load(Ordering::SeqCst) {
        assert!(
            futures_now(sending.as_mut()).is_none(),
            "an outcome before the handshake"
        );
        tokio::task::yield_now().await;
    }
    let start = Instant::now();
    let outcome = tokio::time::timeout(secs(31), sending).await;
    release.send(()).unwrap();
    server.join().unwrap();

    let error = failure(outcome.expect("the connect timeout ended the call before 31 s"));
    assert_eq!(start.elapsed(), secs(30));
    assert_eq!(error.class(), Some(Class::Timeout));
    assert_eq!(
        error.to_string(),
        "transport error: no connection after 30 s (the connect timeout of the HTTP client), in tasks.get"
    );
}

/// Poll a future once.
fn futures_now<F: std::future::Future>(future: std::pin::Pin<&mut F>) -> Option<F::Output> {
    let waker = std::task::Waker::noop();
    match future.poll(&mut std::task::Context::from_waker(waker)) {
        std::task::Poll::Ready(output) => Some(output),
        std::task::Poll::Pending => None,
    }
}

// C7.3 ------------------------------------------------------------------------------

#[tokio::test]
async fn c7_3_a_client_given_no_transport_sends_with_concords_own() {
    let server = Server::start().await;
    server
        .on("GET", "/api/tasks/7")
        .reply(200)
        .header("content-type", "application/json")
        .body(TASK);
    server
        .on("GET", "/api/tasks/8")
        .reply(302)
        .header("location", "/api/tasks/7");
    let tasks = tour::Tasks::new().with_base(format!("{}/api/tasks", server.url()));
    let clone = tasks.clone();

    assert_eq!(success(tasks.get(7).await).id, 7);
    // It is a `concord::Http`: no redirect is followed.
    assert!(failure(clone.get(8).await).is_status(302));
    assert_eq!(server.requests().len(), 2);
}

#[tokio::test]
async fn c7_3_the_default_transport_is_made_once_and_shared_by_every_clone() {
    // C7.3: "The default transport, a `concord::Http`, is made at the first send of a client
    // that was given none, once, in a cell that all its clones share." One `Http` keeps the
    // connection that it opened: a server that keeps it alive sees one connection for the
    // sends of a client and of its clones.
    use std::io::{BufRead, BufReader, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/api/tasks", listener.local_addr().unwrap());
    let connections = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let accepted = connections.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { return };
            accepted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut writer = stream;
                loop {
                    // One request: its head, and no body (a `DELETE` with none).
                    let mut line = String::new();
                    loop {
                        line.clear();
                        match reader.read_line(&mut line) {
                            Ok(0) | Err(_) => return,
                            Ok(_) if line == "\r\n" => break,
                            Ok(_) => {}
                        }
                    }
                    let answer = b"HTTP/1.1 204 No Content\r\nconnection: keep-alive\r\n\r\n";
                    if writer.write_all(answer).is_err() {
                        return;
                    }
                }
            });
        }
    });

    let client = tour::Tasks::new().with_base(url);
    let clone = client.clone();
    let other = clone.clone();
    for (n, client) in [&client, &clone, &other, &client].into_iter().enumerate() {
        success(client.delete(n as u64).await);
        // Let the connection go back to its pool.
        for _ in 0..16 {
            tokio::task::yield_now().await;
        }
    }

    assert_eq!(
        connections.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "each clone made a transport of its own"
    );
}

// C7.5 ------------------------------------------------------------------------------

#[tokio::test]
async fn c7_5_a_client_of_the_user_that_follows_redirects_does_it_below_the_middlewares() {
    let server = Server::start().await;
    server
        .on("GET", "/api/tasks/7")
        .reply(302)
        .header("location", "/api/tasks/8");
    server
        .on("GET", "/api/tasks/8")
        .reply(200)
        .header("content-type", "application/json")
        .body(TASK);
    let client = concord::reqwest::Client::builder()
        .no_proxy()
        .build()
        .unwrap();
    let log = Log::default();

    let task = success(
        tour::Tasks::new()
            .with_transport(client)
            .with_middleware(Logs {
                log: log.clone(),
                name: "a",
            })
            .with_base(format!("{}/api/tasks", server.url()))
            .get(7)
            .await,
    );

    assert_eq!(task.id, 7);
    assert_eq!(
        log.events(),
        ["a out 1 tasks.get", "a sees x-signature: false", "a back"]
    );
    assert_eq!(server.requests().len(), 2);
}

// C7.6, table 7.1: `timeout` --------------------------------------------------------

/// The call of a cell: `tasks.get` with a `timeout` of 60 s and no silence that ends first.
fn get_60(client: tour::Tasks) -> tour::tasks::Get {
    client.get(7).with_timeout(secs(60)).with_idle_timeout(LONG)
}

const ENDS: &str = "What the caller gets when it ends";

fn table_7_1() -> Vec<Cell> {
    vec![
        cell("Building", ENDS, async {
            let script = Script::new();
            json(&script, 200, TASK);
            let start = Instant::now();
            let result = tasks(&script).get(7).with_timeout(secs(0)).await;
            timed(&result, start, secs(0), "the Timeout error", is_timeout)?;
            check(script.requests().is_empty(), || {
                "a request was handed over".to_owned()
            })
        }),
        cell("A middleware waits before it sends on", ENDS, async {
            let script = Script::new();
            json(&script, 200, TASK);
            let start = Instant::now();
            let client = tasks(&script).with_middleware(Waits {
                before: secs(100),
                after: secs(0),
            });
            let result = get_60(client).await;
            timed(&result, start, secs(60), "the Timeout error", is_timeout)?;
            check(script.requests().is_empty(), || {
                "a request was handed over".to_owned()
            })
        }),
        cell(
            "A middleware waits after the transport answered",
            ENDS,
            async {
                let script = Script::new();
                json(&script, 200, TASK);
                let start = Instant::now();
                let client = tasks(&script).with_middleware(Waits {
                    before: secs(0),
                    after: secs(100),
                });
                let result = get_60(client).await;
                timed(&result, start, secs(60), "the Timeout error", is_timeout)
            },
        ),
        cell("The request goes out", ENDS, async {
            let script = Script::new();
            script.answer(200).after(secs(100));
            json(&script, 200, TASK);
            let start = Instant::now();
            let result = get_60(tasks(&script).with_retry(Retry::standard())).await;
            timed(&result, start, secs(60), "the Timeout error", is_timeout)?;
            check(script.requests().len() == 1, || "another try".to_owned())
        }),
        cell("A stream body goes out", ENDS, async {
            let script = Script::new();
            script.answer(200).read_upload();
            let body = paced_stream(&["a"; 10], secs(10)).await;
            let start = Instant::now();
            let result = files(&script)
                .upload("a.bin", body)
                .with_timeout(secs(60))
                .with_idle_timeout(LONG)
                .await;
            timed(&result, start, secs(60), "the Timeout error", is_timeout)
        }),
        cell("Waiting for the head", ENDS, async {
            let script = Script::new();
            script.never();
            let start = Instant::now();
            let result = get_60(tasks(&script)).await;
            timed(&result, start, secs(60), "the Timeout error", is_timeout)
        }),
        cell("Reading the body", ENDS, async {
            let script = Script::new();
            script
                .answer(200)
                .header("content-type", "application/json")
                .body("[")
                .pause(secs(100))
                .body("]");
            let start = Instant::now();
            let result = get_60(tasks(&script)).await;
            timed(&result, start, secs(60), "the Timeout error", is_timeout)?;

            // A refusal that was already known is given in its place.
            let script = Script::new();
            script.answer(404).body("par").pause(secs(100)).body("t");
            let start = Instant::now();
            let result = get_60(tasks(&script)).await;
            timed(
                &result,
                start,
                secs(60),
                "the refusal, interrupted",
                |result| {
                    matches!(result, Err(error) if error.kind() == Kind::Refused
                    && error.is_status(404)
                    && error.kept() == Some(Kept::Interrupted)
                    && error.body() == Some(&b"par"[..]))
                },
            )
        }),
        cell("A ByteStream in the caller's hands", ENDS, async {
            let script = Script::new();
            script
                .answer(200)
                .header("x-a", "1")
                .body("a")
                .pause(secs(100))
                .body("b");
            let start = Instant::now();
            let mut stream = match files(&script)
                .download(tour::FileId::from("f"))
                .with_timeout(secs(60))
                .with_idle_timeout(LONG)
                .await
            {
                Ok(stream) => stream,
                Err(error) => return Err(format!("{error}")),
            };
            let _ = stream.try_next().await;
            let result = stream.try_next().await;
            timed_stream(
                &result,
                start,
                secs(60),
                "a timeout from the stream",
                |result| {
                    matches!(result, Err(error) if error.is_timeout()
                    && error.is_status(200)
                    && error.headers().and_then(|h| h.get("x-a")) == Some("1")
                    && error.endpoint() == "files.download"
                    && error.tries() == 1)
                },
            )
        }),
        cell("A wait between two tries", ENDS, async {
            let script = Script::new();
            script.answer(503).header("retry-after", "20");
            json(&script, 200, TASK);
            let start = Instant::now();
            let result = tasks(&script)
                .with_retry(Retry::standard())
                .get(7)
                .with_timeout(secs(10))
                .await;
            timed(
                &result,
                start,
                secs(0),
                "the last outcome, unchanged",
                |result| matches!(result, Err(error) if error.is_status(503) && error.tries() == 1),
            )
        }),
        cell(
            "After send_raw(), the answer in the caller's hands",
            ENDS,
            async {
                let script = Script::new();
                script.answer(200).pause(secs(100)).body("late");
                let start = Instant::now();
                let response = match tasks(&script)
                    .get(7)
                    .with_timeout(secs(10))
                    .with_idle_timeout(secs(5))
                    .send_raw()
                    .await
                {
                    Ok(response) => response,
                    Err(error) => return Err(format!("{error}")),
                };
                let body = response.bytes().await.map_err(|e| e.to_string())?;
                check(&body[..] == b"late" && start.elapsed() == secs(100), || {
                    "a clock followed the answer".to_owned()
                })
            },
        ),
    ]
}

#[tokio::test(start_paused = true)]
async fn c7_6_table_7_1_timeout() {
    // "`ByteStream::try_next()` gives a `concord::read::StreamError`".
    run_cells("table 7.1", table_7_1()).await;
}

// C7.6, table 7.2: `idle_timeout`, the clock for silence -----------------------------

fn table_7_2() -> Vec<Cell> {
    vec![
        cell("A middleware waits before it sends on", ENDS, async {
            let script = Script::new();
            json(&script, 200, TASK);
            let start = Instant::now();
            let result = tasks(&script)
                .with_middleware(Waits {
                    before: secs(100),
                    after: secs(0),
                })
                .get(7)
                .await;
            timed(&result, start, secs(100), "the value", Result::is_ok)
        }),
        cell(
            "A middleware waits after the transport answered",
            ENDS,
            async {
                let script = Script::new();
                json(&script, 200, TASK);
                let start = Instant::now();
                let result = tasks(&script)
                    .with_middleware(Waits {
                        before: secs(0),
                        after: secs(100),
                    })
                    .get(7)
                    .await;
                timed(&result, start, secs(100), "the value", Result::is_ok)
            },
        ),
        cell("The request goes out", ENDS, async {
            // A body in memory goes out in one piece: nothing starts the clock again.
            let script = Script::new();
            script.answer(200).read_upload().after(secs(31));
            let start = Instant::now();
            let result = files(&script)
                .upload("a.bin", concord::Body::new("abc", "text/plain"))
                .await;
            timed(&result, start, secs(30), "the Timeout error", is_timeout)?;
            let line = result.err().map(|error| error.to_string());
            check(
                line.as_deref()
                    == Some("transport error: no answer for 30 s (with_idle_timeout changes it), in files.upload"),
                || format!("{line:?}"),
            )?;
            // Retry may then try again.
            let script = Script::new();
            script.never();
            json(&script, 200, TASK);
            let start = Instant::now();
            let result = tasks(&script).with_retry(Retry::standard()).get(7).await;
            check(result.is_ok() && script.requests().len() == 2, || {
                outcome(&result)
            })?;
            check(start.elapsed() >= secs(30), || "too soon".to_owned())
        }),
        cell("A stream body goes out", ENDS, async {
            // Each piece that is taken starts it again: 60 s of upload, 20 s of silence.
            let script = Script::new();
            script.answer(200).read_upload();
            let body = paced_stream(&["a", "b", "c"], secs(20)).await;
            let start = Instant::now();
            let result = files(&script).upload("a.bin", body).await;
            timed(&result, start, secs(60), "the value", Result::is_ok)?;
            // A piece that is 40 s late.
            let script = Script::new();
            script.answer(200).read_upload();
            let body = paced_stream(&["a", "b"], secs(40)).await;
            let start = Instant::now();
            let result = files(&script).upload("a.bin", body).await;
            timed(&result, start, secs(30), "the Timeout error", is_timeout)
        }),
        cell("Waiting for the head", ENDS, async {
            let script = Script::new();
            script
                .answer(200)
                .after(secs(29))
                .header("content-type", "application/json")
                .body(TASK);
            let start = Instant::now();
            let result = tasks(&script).get(7).await;
            timed(&result, start, secs(29), "the value", Result::is_ok)?;
            let script = Script::new();
            script
                .answer(200)
                .after(secs(31))
                .header("content-type", "application/json")
                .body(TASK);
            let start = Instant::now();
            let result = tasks(&script).get(7).await;
            timed(&result, start, secs(30), "the Timeout error", is_timeout)
        }),
        cell("Reading the body", ENDS, async {
            // Each piece starts it again.
            let script = Script::new();
            script
                .answer(200)
                .header("content-type", "application/json")
                .pause(secs(20))
                .body(&TASK[..10])
                .pause(secs(20))
                .body(&TASK[10..20])
                .pause(secs(20))
                .body(&TASK[20..]);
            let start = Instant::now();
            let result = tasks(&script).get(7).await;
            timed(&result, start, secs(60), "the value", Result::is_ok)?;
            // A silence of 31 s.
            let script = Script::new();
            script
                .answer(200)
                .header("content-type", "application/json")
                .body(&TASK[..10])
                .pause(secs(31))
                .body(&TASK[10..]);
            let start = Instant::now();
            let result = tasks(&script).get(7).await;
            timed(&result, start, secs(30), "the Timeout error", is_timeout)
        }),
        cell("A ByteStream in the caller's hands", ENDS, async {
            // `files.download` says `idle_timeout 120 s`. It runs while a `try_next()`
            // waits, not between two of them.
            let script = Script::new();
            script.answer(200).body("a").pause(secs(100)).body("b");
            let start = Instant::now();
            let mut stream = match files(&script).download(tour::FileId::from("f")).await {
                Ok(stream) => stream,
                Err(error) => return Err(format!("{error}")),
            };
            let _ = stream.try_next().await;
            tokio::time::advance(secs(200)).await;
            let result = stream.try_next().await;
            // 200 s between the two calls, then at most 100 s of waiting: never 120 s of
            // silence while a `try_next()` waits.
            check(
                matches!(&result, Ok(Some(piece)) if &piece[..] == b"b"),
                || format!("{}, not the next piece", stream_outcome(&result)),
            )?;
            check(
                start.elapsed() >= secs(200) && start.elapsed() <= secs(300),
                || format!("it ended at {:?}", start.elapsed()),
            )?;
            let script = Script::new();
            script.answer(200).body("a").pause(secs(121)).body("b");
            let start = Instant::now();
            let mut stream = match files(&script).download(tour::FileId::from("f")).await {
                Ok(stream) => stream,
                Err(error) => return Err(format!("{error}")),
            };
            let _ = stream.try_next().await;
            let result = stream.try_next().await;
            timed_stream(
                &result,
                start,
                secs(120),
                "a timeout from the stream",
                |result| matches!(result, Err(error) if error.is_timeout() && error.is_status(200)),
            )
        }),
        cell("A wait between two tries", ENDS, async {
            let script = Script::new();
            script.answer(503).header("retry-after", "45");
            json(&script, 200, TASK);
            let start = Instant::now();
            let result = tasks(&script).with_retry(Retry::standard()).get(7).await;
            timed(&result, start, secs(45), "the value", Result::is_ok)
        }),
        cell(
            "After send_raw(), the answer in the caller's hands",
            ENDS,
            async {
                let script = Script::new();
                script.answer(200).pause(secs(40)).body("late");
                let start = Instant::now();
                let response = match tasks(&script).get(7).send_raw().await {
                    Ok(response) => response,
                    Err(error) => return Err(format!("{error}")),
                };
                let body = response.bytes().await.map_err(|e| e.to_string())?;
                check(&body[..] == b"late" && start.elapsed() == secs(40), || {
                    "a clock followed the answer".to_owned()
                })
            },
        ),
    ]
}

#[tokio::test(start_paused = true)]
async fn c7_6_table_7_2_idle_timeout() {
    // "`ByteStream::try_next()` gives a `concord::read::StreamError`".
    run_cells("table 7.2", table_7_2()).await;
}

// C7.6, table 7.3: the length of the clock for silence -------------------------------

#[tokio::test(start_paused = true)]
async fn c7_6_table_7_3_the_length_of_the_clock_for_silence() {
    // The description says `idle_timeout 120 s` on `files.download`; `files.readme` has none.
    let rows: [(&str, bool, bool, bool, u64); 8] = [
        ("Set / Set / Set", true, true, true, 10),
        ("Set / Set / Not set", true, true, false, 10),
        ("Set / Not set / Set", true, false, true, 10),
        ("Set / Not set / Not set", true, false, false, 10),
        ("Not set / Set / Set", false, true, true, 120),
        ("Not set / Set / Not set", false, true, false, 120),
        ("Not set / Not set / Set", false, false, true, 50),
        ("Not set / Not set / Not set", false, false, false, 30),
    ];
    let mut cells = Vec::new();
    for (row, on_call, in_description, on_client, length) in rows {
        cells.push(cell(row, "The length", async move {
            let script = Script::new();
            script.never();
            let mut client = files(&script);
            if on_client {
                client = client.with_idle_timeout(secs(50));
            }
            let start = Instant::now();
            let result = if in_description {
                let mut call = client.download(tour::FileId::from("f"));
                if on_call {
                    call = call.with_idle_timeout(secs(10));
                }
                call.await.map(drop)
            } else {
                let mut call = client.readme("repo");
                if on_call {
                    call = call.with_idle_timeout(secs(10));
                }
                call.await.map(drop)
            };
            timed(
                &result,
                start,
                secs(length),
                "the Timeout error",
                is_timeout,
            )
        }));
    }
    run_cells("table 7.3", cells).await;
}

// C7.6, beside the tables ------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c7_6_a_clock_ends_a_wait_and_work_that_is_ready_finishes() {
    // The head comes at the very instant at which the clock for silence ends.
    let script = Script::new();
    script
        .answer(200)
        .after(secs(30))
        .header("content-type", "application/json")
        .body(TASK);
    assert_eq!(success(tasks(&script).get(7).await).id, 7);

    // The same at the end of `timeout`: the request was handed over before it ended.
    let script = Script::new();
    script
        .answer(200)
        .after(secs(60))
        .header("content-type", "application/json")
        .body(TASK);
    assert_eq!(
        success(
            tasks(&script)
                .get(7)
                .with_timeout(secs(60))
                .with_idle_timeout(LONG)
                .await
        )
        .id,
        7
    );
}

#[tokio::test(start_paused = true)]
async fn c7_6_neither_clock_changes_the_other() {
    // Each clock keeps its own length when the other is set.
    let script = Script::new();
    script.never();
    let start = Instant::now();
    let error = failure(
        tasks(&script)
            .with_timeout(secs(100))
            .get(7)
            .with_idle_timeout(secs(30))
            .await,
    );
    assert_eq!(start.elapsed(), secs(30));
    assert_eq!(
        error.to_string(),
        "transport error: no answer for 30 s (with_idle_timeout changes it), in tasks.get"
    );

    let script = Script::new();
    script.never();
    let start = Instant::now();
    let error = failure(
        tasks(&script)
            .with_idle_timeout(secs(100))
            .get(7)
            .with_timeout(secs(60))
            .await,
    );
    assert_eq!(start.elapsed(), secs(60));
    assert_eq!(
        error.to_string(),
        "transport error: the call took longer than 60 s (with_timeout), in tasks.get"
    );
}

#[tokio::test(start_paused = true)]
async fn c7_6_a_timeout_and_an_idle_timeout_that_end_together_report_the_timeout() {
    // "When the timeout of the call and the `idle_timeout` of a body end at the
    // same instant, the error is the timeout of the call. [...] of two clocks that end
    // together, the one that covers more is reported".
    const TIMEOUT: &str =
        "transport error: the call took longer than 5 s (with_timeout), in tasks.get";

    // Before the head: both clocks start when the call is handed over.
    let script = Script::new();
    script.never();
    let start = Instant::now();
    let error = failure(
        tasks(&script)
            .with_timeout(secs(5))
            .get(7)
            .with_idle_timeout(secs(5))
            .await,
    );
    assert_eq!(start.elapsed(), secs(5));
    assert!(error.is_timeout());
    assert_eq!(error.to_string(), TIMEOUT);

    // While the body is read: the head came after 2 s, a piece after 2 s more; then
    // silence. The clock for silence (3 s) and the timeout (7 s) end together.
    let script = Script::new();
    script
        .answer(200)
        .header("content-type", "application/json")
        .after(secs(2))
        .pause(secs(2))
        .body("{")
        .stall();
    let start = Instant::now();
    let error = failure(
        tasks(&script)
            .with_timeout(secs(7))
            .get(7)
            .with_idle_timeout(secs(3))
            .await,
    );
    assert_eq!(start.elapsed(), secs(7));
    assert!(error.is_timeout());
    assert_eq!(
        error.to_string(),
        "transport error: the call took longer than 7 s (with_timeout), in tasks.get"
    );

    // A `ByteStream` that was handed over: the same.
    let script = Script::new();
    script.answer(200).pause(secs(2)).body("a").stall();
    let start = Instant::now();
    let mut stream = success(
        files(&script)
            .with_timeout(secs(5))
            .download(tour::FileId::from("f"))
            .with_idle_timeout(secs(3))
            .await,
    );
    let mut pieces = 0;
    let error = loop {
        match stream.try_next().await {
            Ok(Some(_)) => pieces += 1,
            Ok(None) => panic!("the stream stalls; it does not end"),
            Err(error) => break error,
        }
    };
    assert_eq!(pieces, 1);
    assert_eq!(start.elapsed(), secs(5));
    assert!(error.is_timeout());
    assert_eq!(
        error.to_string(),
        "transport error: the call took longer than 5 s (with_timeout), in files.download"
    );
}

#[tokio::test(start_paused = true)]
async fn c7_6_timeout_comes_from_the_call_or_else_from_the_client() {
    let script = Script::new();
    script.never();
    script.never();
    let client = tasks(&script)
        .with_timeout(secs(60))
        .with_idle_timeout(LONG);

    let start = Instant::now();
    assert!(failure(client.get(7).await).is_timeout());
    assert_eq!(start.elapsed(), secs(60));

    let start = Instant::now();
    assert!(failure(client.get(7).with_timeout(secs(90)).await).is_timeout());
    assert_eq!(start.elapsed(), secs(90));
}

/// A reader of the caller's own that waits for ever.
#[derive(Clone, Copy)]
struct WaitsForEver;

impl concord::read::Reader<()> for WaitsForEver {
    async fn read(
        &self,
        _incoming: concord::read::Incoming,
    ) -> Result<(), concord::read::ReadError> {
        std::future::pending().await
    }
}

#[tokio::test(start_paused = true)]
async fn c7_6_timeout_is_one_clock_for_the_tries_the_waits_and_a_readers_own_wait() {
    // C7.6: "*Timeout* starts when the call is sent [...] and bounds everything: building, the
    // middlewares, each try, each wait between tries, the reading of the body". A first try of
    // 3 s, a wait of 2 s, then a reader that waits for ever: the call ends at 10 s, not 10 s
    // after any of them began. (Table 7.1, row "A reader's own wait": only `timeout`.)
    let script = Script::new();
    script.answer(503).after(secs(3)).header("retry-after", "2");
    script.answer(200).body("{}");
    let core = concord::__private::Core::default()
        .with_transport(script.clone())
        .with_timeout(secs(10))
        .with_retry(Retry::standard());
    let start = Instant::now();

    let error = failure(prepared_get(core, WaitsForEver).send().await);

    assert!(error.is_timeout(), "{error}");
    assert_eq!(start.elapsed(), secs(10));
    assert_eq!(script.requests().len(), 2);
    assert_eq!(
        error.to_string(),
        "transport error: the call took longer than 10 s (with_timeout), in tasks.get, after 2 tries"
    );

    // The same through a client of `tests/tour`: the second body never ends, and the clock for
    // silence is longer than the call.
    let script = Script::new();
    script.answer(503).after(secs(3)).header("retry-after", "2");
    script
        .answer(200)
        .header("content-type", "application/json")
        .body("{")
        .stall();
    let start = Instant::now();
    let error = failure(
        tasks(&script)
            .with_retry(Retry::standard())
            .get(7)
            .with_timeout(secs(10))
            .with_idle_timeout(LONG)
            .await,
    );
    assert!(error.is_timeout(), "{error}");
    assert_eq!(start.elapsed(), secs(10));
}

#[tokio::test(start_paused = true)]
async fn c7_6_a_refusal_already_known_is_the_result_when_timeout_ends_in_its_problem_reader() {
    // C7.6: "An outcome that is already known is never lost to a clock"; C3.9: "`refused`
    // writes into it what it knows of a refusal". The body of the refusal is whole; then the
    // reader of its problem waits for ever, and `timeout` ends.
    use concord::__private as engine;
    let script = Script::new();
    script.answer(503).header("x-a", "1").body("evidence");
    let core = engine::Core::<()>::default()
        .with_transport(script)
        .with_timeout(secs(2));
    let options = engine::Options::default();
    let run = engine::Run::start(&core, &options, "tasks.get");
    let mut draft = engine::Draft::new(http::Method::GET);
    draft.base(&[engine::Piece::Lit("https://tasks.example.com/api/tasks/7")]);
    let prepared: engine::Prepared<(), ()> =
        draft
            .prepare(
                run,
                core,
                options,
                engine::Idempotent::ByMethod,
                |response, info| async move {
                    Err(engine::refused(&WaitsForEver, response, &info).await)
                },
            )
            .unwrap();
    let start = Instant::now();

    let error = failure(prepared.send().await);

    assert_eq!(error.kind(), Kind::Refused, "{error}");
    assert!(error.is_status(503));
    assert_eq!(error.headers().and_then(|h| h.get("x-a")), Some("1"));
    assert_eq!(error.body(), Some(&b"evidence"[..]));
    assert_eq!(start.elapsed(), secs(2));
}

#[tokio::test]
async fn c7_6_a_head_and_then_silence_on_the_wire() {
    let server = Server::start().await;
    server
        .on("GET", "/api/tasks/7")
        .reply(200)
        .header("content-type", "application/json")
        .body("{")
        .silence();

    let error = failure(
        tasks_on(&server)
            .get(7)
            .with_idle_timeout(Duration::from_millis(200))
            .await,
    );

    assert_eq!(error.kind(), Kind::Transport);
    assert_eq!(error.class(), Some(Class::Timeout));
}

// C7.7 ------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c7_7_a_call_that_is_not_sent_or_not_polled_sends_nothing() {
    let script = Script::new();
    json(&script, 200, TASK);

    drop(tasks(&script).get(7));
    drop(tasks(&script).get(7).send());
    drop(tasks(&script).get(7).send_full());
    drop(tasks(&script).get(7).send_raw());

    assert!(script.requests().is_empty());
    assert_eq!(script.unused(), 1);
}

#[tokio::test(start_paused = true)]
async fn c7_7_a_call_that_is_not_polled_builds_nothing() {
    // engine: "Nothing runs before the first poll." A future of `send_full()`, `send_raw()` or
    // `send()` that is dropped unpolled runs no edit (C3.1), so no request was built.
    let count = Count::default();
    for way in 0..4 {
        let edits = count.clone();
        let call = tour::Tasks::new().get(7).edit_request(move |_| edits.add());
        match way {
            0 => drop(call.send_full()),
            1 => drop(call.send_raw()),
            2 => drop(call.send()),
            _ => drop(call.send_if_none_match("\"v1\"")),
        }
    }
    assert_eq!(count.get(), 0);
}

#[tokio::test(start_paused = true)]
async fn c7_7_a_call_that_is_dropped_starts_no_other_try() {
    let script = Script::new();
    script.answer(503).header("retry-after", "10");
    json(&script, 200, TASK);

    let call = tasks(&script).with_retry(Retry::standard()).get(7);
    assert!(tokio::time::timeout(secs(5), call.into_future())
        .await
        .is_err());
    tokio::time::advance(secs(60)).await;

    assert_eq!(script.requests().len(), 1);
    assert_eq!(script.unused(), 1);
}

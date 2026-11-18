//! C9.5 and C9.6: when the call's error states that no byte of its request was written
//! (`Error::request_not_sent`), checked against what a local server received.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use concord_core::__private::{Core, Draft, Idempotent, Options, Run};
use concord_core::error::{Class, Kind};
use concord_core::transport::{TransportError, TryInfo};
use concord_core::{http, reqwest, Answer, Body, Error, Http, Middleware, Next, Retry, Transport};
use futures_util::FutureExt;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

const FORM: &str = "grant_type=refresh_token&refresh_token=first";

fn run<T>(work: impl std::future::Future<Output = T>) -> T {
    // A real clock: the servers are threads, and no step of these tests waits on time.
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(work)
}

fn http() -> Http {
    Http::with(|builder| builder.no_proxy()).unwrap()
}

/// A local port with nothing listening on it.
fn closed_port() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    addr
}

fn leak(url: String) -> &'static str {
    Box::leak(url.into_boxed_str())
}

async fn send(
    core: &Core<()>,
    method: http::Method,
    url: &'static str,
    body: Option<Body>,
) -> Error {
    let options = Options::default();
    let run = Run::start(core, &options, "tokens.exchange");
    let mut draft = Draft::from_literal(method, url);
    if let Some(body) = body {
        draft.body(body);
    }
    let result = async {
        draft
            .prepare(
                run,
                core.clone(),
                options,
                Idempotent::ByMethod,
                |response, info| async move {
                    if !response.status().is_success() {
                        return Err(concord_core::__private::refused_plain(response, &info).await);
                    }
                    Ok(Answer::new(
                        response.status(),
                        response.headers().clone().into(),
                        (),
                    ))
                },
            )?
            .send()
            .await
    }
    .await;
    match result {
        Ok(answer) => panic!("the call must fail, got {}", answer.status()),
        Err(error) => error,
    }
}

async fn post(core: &Core<()>, url: &'static str) -> Error {
    let body = Body::new(FORM, "application/x-www-form-urlencoded");
    send(core, http::Method::POST, url, Some(body)).await
}

fn assert_connect(error: &Error, not_sent: bool) {
    assert_eq!(error.kind(), Kind::Transport, "{error}");
    assert_eq!(error.tries(), 1, "{error}");
    assert_eq!(error.request_not_sent(), not_sent, "{error}");
}

/// Reads one request head, byte by byte, and returns it.
fn read_head(stream: &mut TcpStream) -> Vec<u8> {
    let mut head = Vec::new();
    let mut byte = [0];
    while !head.ends_with(b"\r\n\r\n") {
        stream.read_exact(&mut byte).unwrap();
        head.push(byte[0]);
    }
    head
}

/// Serves one connection on a thread and returns what `serve` returns.
fn one_connection<T: Send + 'static>(
    serve: impl FnOnce(TcpStream) -> T + Send + 'static,
) -> (&'static str, std::thread::JoinHandle<T>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        serve(stream)
    });
    (leak(format!("http://{addr}/token")), server)
}

#[test]
fn c9_5_http_states_not_sent_for_a_refused_connection() {
    // Nothing listens on the port: no byte can have arrived anywhere.
    let url = leak(format!("http://{}/token", closed_port()));
    let error = run(async {
        let core = Core::<()>::default().with_transport(http());
        post(&core, url).await
    });
    assert_connect(&error, true);
    assert_eq!(error.class(), Some(Class::Connect));
    assert_eq!(
        error.to_string(),
        "transport error: connection refused, in tokens.exchange"
    );
    // It survives the conversion of the problem's type.
    assert!(error.map_problem(|()| 0u8).request_not_sent());

    // A host name that does not resolve: the connector fails before any socket.
    struct NoHost;
    impl reqwest::dns::Resolve for NoHost {
        fn resolve(&self, _: reqwest::dns::Name) -> reqwest::dns::Resolving {
            Box::pin(std::future::ready(Err(Box::new(std::io::Error::from(
                std::io::ErrorKind::NotFound,
            ))
                as Box<dyn std::error::Error + Send + Sync>)))
        }
    }
    let error = run(async {
        let http = Http::with(|builder| builder.no_proxy().dns_resolver(Arc::new(NoHost))).unwrap();
        let core = Core::<()>::default().with_transport(http);
        post(&core, "http://identity.test/token").await
    });
    assert_connect(&error, true);
    assert_eq!(
        error.to_string(),
        "transport error: the host name could not be resolved, in tokens.exchange"
    );
}

#[test]
fn c9_5_http_does_not_state_not_sent_after_the_request_was_written() {
    // Accepted, and closed before the server read anything: the request was written all
    // the same, into the server's buffer, where `peek` sees it without reading it.
    let (url, server) = one_connection(|stream| {
        let mut seen = [0; 64];
        let count = stream.peek(&mut seen).unwrap();
        // Closing with unread bytes resets the connection.
        drop(stream);
        seen[..count].to_vec()
    });
    let error = run(async {
        let core = Core::<()>::default().with_transport(http());
        post(&core, url).await
    });
    assert!(server
        .join()
        .unwrap()
        .starts_with(b"POST /token HTTP/1.1\r\n"));
    assert_connect(&error, false);
    assert_eq!(error.class(), Some(Class::Connect));

    // Reset after the head was read: the head and part of the body were sent.
    let (url, server) = one_connection(|mut stream| {
        let head = read_head(&mut stream);
        drop(stream);
        head
    });
    let error = run(async {
        let core = Core::<()>::default().with_transport(http());
        let body = Body::new(vec![b'x'; 256 * 1024], "application/octet-stream");
        send(&core, http::Method::POST, url, Some(body)).await
    });
    let head = server.join().unwrap();
    assert!(head.starts_with(b"POST /token HTTP/1.1\r\n"), "{head:?}");
    assert_connect(&error, false);
    assert_eq!(error.class(), Some(Class::Connect));
}

#[test]
fn c9_5_http_does_not_state_not_sent_for_a_nested_connect_error() {
    // An upload whose stream fails, after the head was sent, with the error of another
    // request whose connection was refused. That inner error is a connect error, and
    // `reqwest::Error::is_connect` finds it anywhere in the chain.
    let refused = run(async {
        reqwest::Client::builder()
            .no_proxy()
            .build()
            .unwrap()
            .get(format!("http://{}/", closed_port()))
            .send()
            .await
            .unwrap_err()
    });
    assert!(refused.is_connect());
    let (head_read, head_seen) = tokio::sync::oneshot::channel::<()>();
    let (url, server) = one_connection(move |mut stream| {
        let head = read_head(&mut stream);
        head_read.send(()).unwrap();
        let mut rest = Vec::new();
        let _ = stream.read_to_end(&mut rest);
        (head, rest)
    });
    let error = run(async {
        let core = Core::<()>::default().with_transport(http());
        // The stream fails only once the server has read the head.
        let chunks = futures_util::stream::once(async move {
            head_seen.await.unwrap();
            Err::<bytes::Bytes, _>(refused)
        });
        post_stream(&core, url, Body::stream(chunks)).await
    });
    let (head, _) = server.join().unwrap();
    assert!(head.starts_with(b"POST /token HTTP/1.1\r\n"), "{head:?}");
    assert_connect(&error, false);
    let mut nested = false;
    let mut cause = std::error::Error::source(&error);
    while let Some(current) = cause {
        nested |= current
            .downcast_ref::<reqwest::Error>()
            .is_some_and(reqwest::Error::is_connect);
        cause = current.source();
    }
    assert!(nested, "the chain holds the other request's connect error");
}

async fn post_stream(core: &Core<()>, url: &'static str, body: Body) -> Error {
    send(core, http::Method::POST, url, Some(body)).await
}

#[cfg(feature = "rustls")]
#[test]
fn c9_5_http_states_not_sent_after_the_connect_timeout() {
    // The server reads the TLS handshake and never answers it: no connection exists, and
    // what arrived is TLS handshake records only, never application data.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = leak(format!("https://{}/token", listener.local_addr().unwrap()));
    let hello = Arc::new(AtomicBool::new(false));
    let received = hello.clone();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut bytes = vec![0; 5];
        stream.read_exact(&mut bytes).unwrap();
        received.store(true, Ordering::SeqCst);
        // Everything until the client closes the connection.
        let _ = stream.read_to_end(&mut bytes);
        bytes
    });
    let error = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .start_paused(true)
        .build()
        .unwrap()
        .block_on(async {
            let core = Core::<()>::default()
                .with_transport(http())
                .with_idle_timeout(Duration::from_secs(3600));
            let mut sending = std::pin::pin!(post(&core, url));
            // Poll real I/O on a frozen clock until the server has the handshake.
            while !hello.load(Ordering::SeqCst) {
                assert!(sending.as_mut().now_or_never().is_none());
                tokio::task::yield_now().await;
            }
            tokio::time::advance(Duration::from_secs(30)).await;
            sending.await
        });
    let bytes = server.join().unwrap();
    // TLS records: a type, a version, a length of two bytes, and the content.
    let mut at = 0;
    while at < bytes.len() {
        assert_eq!(
            bytes[at], 0x16,
            "a handshake record, never application data (0x17)"
        );
        let length = usize::from(u16::from_be_bytes([bytes[at + 3], bytes[at + 4]]));
        at += 5 + length;
    }
    assert_connect(&error, true);
    assert_eq!(error.class(), Some(Class::Timeout));
    assert_eq!(
        error.to_string(),
        "transport error: no connection (the connect timeout of the HTTP client), in tokens.exchange"
    );
}

/// A transport of the author's, whose failure is given.
struct Fails(fn() -> TransportError);

impl Transport for Fails {
    async fn send(
        &self,
        _: reqwest::Request,
        _: TryInfo,
    ) -> Result<reqwest::Response, TransportError> {
        Err((self.0)())
    }
}

#[test]
fn c9_5_a_transport_of_the_authors_states_not_sent() {
    let url = "http://identity.test/token";
    let error = run(async {
        let core =
            Core::<()>::default().with_transport(Fails(|| TransportError::connect().not_sent()));
        post(&core, url).await
    });
    assert_connect(&error, true);
    assert_eq!(error.class(), Some(Class::Connect));
    // `with_source` keeps the statement.
    let error = run(async {
        let core = Core::<()>::default().with_transport(Fails(|| {
            TransportError::timeout()
                .not_sent()
                .with_source(std::io::Error::other("offline"))
        }));
        post(&core, url).await
    });
    assert_connect(&error, true);
    assert_eq!(error.class(), Some(Class::Timeout));
    // Without it, a failed connection states nothing; a blocked request states it by its class.
    for (failure, not_sent) in [
        (TransportError::connect as fn() -> TransportError, false),
        (TransportError::timeout, false),
        (TransportError::blocked, true),
    ] {
        let error = run(async {
            let core = Core::<()>::default().with_transport(Fails(failure));
            post(&core, url).await
        });
        assert_connect(&error, not_sent);
    }
}

#[test]
fn c9_5_a_transport_that_wraps_http_and_sends_twice_states_nothing() {
    // The author's transport sends once to a server that answers, then once to a closed
    // port, and returns the second failure, which `Http` stated.
    struct Twice {
        http: Http,
        refused: reqwest::Url,
    }
    impl Transport for Twice {
        async fn send(
            &self,
            request: reqwest::Request,
            info: TryInfo,
        ) -> Result<reqwest::Response, TransportError> {
            let mut again = request.try_clone().unwrap();
            *again.url_mut() = self.refused.clone();
            let _answered = self.http.send(request, info.clone()).await?;
            self.http.send(again, info).await
        }
    }
    let (url, server) = one_connection(|mut stream| {
        let head = read_head(&mut stream);
        let mut body = vec![0; FORM.len()];
        stream.read_exact(&mut body).unwrap();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
            .unwrap();
        (head, body)
    });
    let refused = format!("http://{}/token", closed_port()).parse().unwrap();
    let error = run(async {
        let core = Core::<()>::default().with_transport(Twice {
            http: http(),
            refused,
        });
        post(&core, url).await
    });
    let (head, body) = server.join().unwrap();
    assert!(head.starts_with(b"POST /token HTTP/1.1\r\n"));
    assert_eq!(body, FORM.as_bytes());
    assert_eq!(error.class(), Some(Class::Connect));
    assert_connect(&error, false);
}

/// Sends the request through the rest of the chain `sends` times and returns the last
/// result; with `drop_first`, it first starts one send and drops it before it returns.
struct Sender {
    sends: usize,
    drop_first: bool,
}

impl Middleware for Sender {
    async fn handle(
        &self,
        request: reqwest::Request,
        info: TryInfo,
        next: Next<'_>,
    ) -> Result<reqwest::Response, TransportError> {
        if self.drop_first {
            let copy = request.try_clone().unwrap();
            assert!(next.send(copy, info.clone()).now_or_never().is_none());
        }
        for _ in 1..self.sends {
            let _ = next.send(request.try_clone().unwrap(), info.clone()).await;
        }
        next.send(request, info).await
    }
}

/// Answers `401` to its first send if `answer_first`, never returns from it if
/// `pending_first`, and otherwise fails stating that nothing was sent.
struct Scripted {
    sends: AtomicUsize,
    answer_first: bool,
    pending_first: bool,
}

impl Transport for Scripted {
    async fn send(
        &self,
        _: reqwest::Request,
        _: TryInfo,
    ) -> Result<reqwest::Response, TransportError> {
        if self.sends.fetch_add(1, Ordering::SeqCst) == 0 {
            if self.answer_first {
                return Ok(concord_core::reply(401, ""));
            }
            if self.pending_first {
                std::future::pending::<()>().await;
            }
        }
        Err(TransportError::connect().not_sent())
    }
}

#[test]
fn c9_6_a_second_send_in_one_attempt_takes_the_statement_away() {
    let cases = [
        // (sends, answer_first, pending_first and drop_first, request_not_sent)
        // One send that states it, passed on by the middleware.
        (1, false, false, true),
        // Two sends that both state it.
        (2, false, false, true),
        // A response first, then a send that states it.
        (2, true, false, false),
        // A send started and dropped before it returned may have written: the statement
        // of the next send is not enough.
        (1, false, true, false),
    ];
    for (sends, answer_first, dropped, not_sent) in cases {
        let error = run(async {
            let core = Core::<()>::default()
                .with_middleware(Sender {
                    sends,
                    drop_first: dropped,
                })
                .with_transport(Scripted {
                    sends: AtomicUsize::new(0),
                    answer_first,
                    pending_first: dropped,
                });
            post(&core, "http://identity.test/token").await
        });
        assert_connect(&error, not_sent);
    }
}

/// Runs the work on paused time, polling it without letting the clock run: the clock moves
/// only past the engine's wait after an attempt ended.
fn run_paused<F: std::future::Future>(work: impl FnOnce(Arc<AtomicBool>) -> F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .start_paused(true)
        .build()
        .unwrap()
        .block_on(async {
            let ended = Arc::new(AtomicBool::new(false));
            let mut work = std::pin::pin!(work(ended.clone()));
            loop {
                if let Some(output) = work.as_mut().now_or_never() {
                    break output;
                }
                if ended.swap(false, Ordering::SeqCst) {
                    // The longest wait of the standard table before a second attempt.
                    tokio::time::advance(Duration::from_secs(1)).await;
                } else {
                    tokio::task::yield_now().await;
                }
            }
        })
}

/// `Http`, sending the attempts after the first to `refused` if it is set, and raising
/// `ended` when an attempt returns.
struct Attempts {
    http: Http,
    refused: Option<SocketAddr>,
    ended: Arc<AtomicBool>,
}

impl Transport for Attempts {
    async fn send(
        &self,
        mut request: reqwest::Request,
        info: TryInfo,
    ) -> Result<reqwest::Response, TransportError> {
        if let Some(refused) = self.refused.filter(|_| info.number() > 1) {
            request.url_mut().set_port(Some(refused.port())).unwrap();
        }
        let result = self.http.send(request, info).await;
        self.ended.store(true, Ordering::SeqCst);
        result
    }
}

#[test]
fn c9_3_a_failure_after_an_earlier_attempt_states_nothing() {
    // The first attempt gets a response, the second cannot connect.
    let (url, server) = one_connection(|mut stream| {
        let head = read_head(&mut stream);
        stream
            .write_all(b"HTTP/1.1 503 Service Unavailable\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
            .unwrap();
        head
    });
    let refused = Some(closed_port());
    let error = run_paused(|ended| async move {
        let core = Core::<()>::default()
            .with_transport(Attempts {
                http: http(),
                refused,
                ended,
            })
            .with_retry(Retry::none().tries(2).when(|_| Some(Duration::ZERO)));
        send(&core, http::Method::GET, url, None).await
    });
    assert!(server
        .join()
        .unwrap()
        .starts_with(b"GET /token HTTP/1.1\r\n"));
    assert_eq!(error.tries(), 2);
    assert_eq!(error.class(), Some(Class::Connect));
    assert!(!error.request_not_sent());

    // Two attempts that both could not connect: nothing was sent on either.
    let url = leak(format!("http://{}/token", closed_port()));
    let error = run_paused(|ended| async move {
        let core = Core::<()>::default()
            .with_transport(Attempts {
                http: http(),
                refused: None,
                ended,
            })
            .with_retry(Retry::none().tries(2).when(|_| Some(Duration::ZERO)));
        send(&core, http::Method::GET, url, None).await
    });
    assert_eq!(error.tries(), 2);
    assert!(error.request_not_sent());
}

/// Answers `200` to its first send and then fails with `failure`; records every arrival.
struct Recorded {
    arrivals: Arc<AtomicUsize>,
    failure: fn() -> TransportError,
}

impl Transport for Recorded {
    async fn send(
        &self,
        _: reqwest::Request,
        _: TryInfo,
    ) -> Result<reqwest::Response, TransportError> {
        if self.arrivals.fetch_add(1, Ordering::SeqCst) == 0 {
            return Ok(concord_core::reply(200, ""));
        }
        Err((self.failure)())
    }
}

/// Sends a copy through the chain with a `TryInfo` of its own making, then the request with
/// the one it received.
struct NewMetadata;

impl Middleware for NewMetadata {
    async fn handle(
        &self,
        request: reqwest::Request,
        info: TryInfo,
        next: Next<'_>,
    ) -> Result<reqwest::Response, TransportError> {
        let copy = request.try_clone().unwrap();
        let made = TryInfo::new(info.endpoint(), info.number());
        assert_eq!(next.send(copy, made).await?.status(), 200);
        next.send(request, info).await
    }
}

#[test]
fn c9_6_a_send_with_metadata_of_the_middlewares_making_is_still_recorded() {
    for failure in [
        (|| TransportError::connect().not_sent()) as fn() -> TransportError,
        TransportError::blocked,
    ] {
        let arrivals = Arc::new(AtomicUsize::new(0));
        let transport = Recorded {
            arrivals: arrivals.clone(),
            failure,
        };
        let error = run(async {
            let core = Core::<()>::default()
                .with_middleware(NewMetadata)
                .with_transport(transport);
            post(&core, "http://identity.test/token").await
        });
        assert_eq!(
            arrivals.load(Ordering::SeqCst),
            2,
            "a 200 was received first"
        );
        assert_eq!(error.tries(), 1);
        assert!(!error.request_not_sent(), "{error}");
    }
}

/// Hands a copy of the request, with the attempt's `TryInfo`, to a task that sends it to
/// `other` once `gate` opens; with `queued`, the send is created before the request goes on.
struct Deferred {
    other: Arc<Recorded>,
    queued: bool,
    gate: std::sync::Mutex<Option<tokio::sync::oneshot::Receiver<()>>>,
    done: std::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
}

impl Middleware for Deferred {
    async fn handle(
        &self,
        request: reqwest::Request,
        info: TryInfo,
        next: Next<'_>,
    ) -> Result<reqwest::Response, TransportError> {
        let other = self.other.clone();
        let copy = request.try_clone().unwrap();
        let kept = info.clone();
        let queued = self.queued;
        let gate = self.gate.lock().unwrap().take().unwrap();
        let done = self.done.lock().unwrap().take().unwrap();
        tokio::spawn(async move {
            let response = if queued {
                let send = Next::to(&*other).send(copy, kept);
                gate.await.unwrap();
                send.await
            } else {
                gate.await.unwrap();
                Next::to(&*other).send(copy, kept).await
            };
            assert_eq!(response.unwrap().status(), 200);
            done.send(()).unwrap();
        });
        tokio::task::yield_now().await;
        next.send(request, info).await
    }
}

#[test]
fn c9_6_a_send_that_holds_the_attempts_metadata_takes_the_statement_away() {
    for queued in [true, false] {
        let other = Arc::new(Recorded {
            arrivals: Arc::new(AtomicUsize::new(0)),
            failure: TransportError::connect,
        });
        let (open, gate) = tokio::sync::oneshot::channel();
        let (done, finished) = tokio::sync::oneshot::channel();
        let deferred = Deferred {
            other: other.clone(),
            queued,
            gate: std::sync::Mutex::new(Some(gate)),
            done: std::sync::Mutex::new(Some(done)),
        };
        run(async {
            let core = Core::<()>::default()
                .with_middleware(deferred)
                .with_transport(Fails(|| TransportError::connect().not_sent()));
            let error = post(&core, "http://identity.test/token").await;
            assert_eq!(other.arrivals.load(Ordering::SeqCst), 0);
            assert!(!error.request_not_sent(), "queued: {queued}");
            open.send(()).unwrap();
            finished.await.unwrap();
            // The send arrived after the call returned its error.
            assert_eq!(other.arrivals.load(Ordering::SeqCst), 1);
        });
    }
}

/// Replaces the metadata with a `TryInfo` of its own making before the layers below.
struct Rebuilds;

impl Middleware for Rebuilds {
    async fn handle(
        &self,
        request: reqwest::Request,
        info: TryInfo,
        next: Next<'_>,
    ) -> Result<reqwest::Response, TransportError> {
        next.send(request, TryInfo::new(info.endpoint(), info.number()))
            .await
    }
}

/// How `Sideways` sends besides the chain.
#[derive(Clone, Copy, Debug)]
enum Aside {
    /// A copy through `Next::to(other)` with the metadata it received, answered `200`.
    Alternate,
    /// A copy kept, with the metadata it received, and sent later.
    Kept,
    /// A send created through the chain and dropped before its first poll.
    NeverPolled,
}

struct Sideways {
    aside: Aside,
    other: Arc<Recorded>,
    kept: Arc<std::sync::Mutex<Option<(reqwest::Request, TryInfo)>>>,
}

impl Middleware for Sideways {
    async fn handle(
        &self,
        request: reqwest::Request,
        info: TryInfo,
        next: Next<'_>,
    ) -> Result<reqwest::Response, TransportError> {
        let copy = request.try_clone().unwrap();
        match self.aside {
            Aside::Alternate => {
                let response = Next::to(&*self.other).send(copy, info.clone()).await?;
                assert_eq!(response.status(), 200);
            }
            Aside::Kept => *self.kept.lock().unwrap() = Some((copy, info.clone())),
            Aside::NeverPolled => drop(next.send(copy, info.clone())),
        }
        next.send(request, info).await
    }
}

#[test]
fn c9_6_a_send_beside_the_chain_with_the_attempts_metadata_takes_the_statement_away() {
    for (aside, rebuilt) in [
        (Aside::Alternate, false),
        (Aside::Kept, true),
        (Aside::NeverPolled, false),
        (Aside::NeverPolled, true),
    ] {
        let other = Arc::new(Recorded {
            arrivals: Arc::new(AtomicUsize::new(0)),
            failure: TransportError::connect,
        });
        let kept = Arc::new(std::sync::Mutex::new(None));
        let sideways = Sideways {
            aside,
            other: other.clone(),
            kept: kept.clone(),
        };
        run(async {
            let mut core = Core::<()>::default();
            if rebuilt {
                core = core.with_middleware(Rebuilds);
            }
            let core = core
                .with_middleware(sideways)
                .with_transport(Fails(|| TransportError::connect().not_sent()));
            let error = post(&core, "http://identity.test/token").await;
            assert!(!error.request_not_sent(), "{aside:?}, rebuilt: {rebuilt}");
            assert!(!matches!(aside, Aside::Kept) || other.arrivals.load(Ordering::SeqCst) == 0);
            let kept = kept.lock().unwrap().take();
            if let Some((copy, info)) = kept {
                // The kept copy reaches a transport after the call returned its error.
                let response = Next::to(&*other).send(copy, info).await.unwrap();
                assert_eq!(response.status(), 200);
            }
        });
        let expected = usize::from(matches!(aside, Aside::Alternate | Aside::Kept));
        assert_eq!(other.arrivals.load(Ordering::SeqCst), expected, "{aside:?}");
    }
}

/// What the `fake` of `Direct` answers.
type Answering = fn(&reqwest::Request) -> Result<reqwest::Response, TransportError>;

/// Calls a `fake` directly, beside the chain, with the metadata it received.
struct Direct(concord_core::transport::Fake<Answering>);

impl Middleware for Direct {
    async fn handle(
        &self,
        request: reqwest::Request,
        info: TryInfo,
        next: Next<'_>,
    ) -> Result<reqwest::Response, TransportError> {
        let copy = request.try_clone().unwrap();
        assert_eq!(self.0.send(copy, info.clone()).await?.status(), 200);
        next.send(request, info).await
    }
}

#[test]
fn c9_6_a_direct_send_to_a_fake_with_the_attempts_metadata_is_recorded() {
    let answer: Answering = |_| Ok(concord_core::reply(200, ""));
    let error = run(async {
        let core = Core::<()>::default()
            .with_middleware(Direct(concord_core::fake(answer)))
            .with_transport(Fails(|| TransportError::connect().not_sent()));
        post(&core, "http://identity.test/token").await
    });
    assert!(!error.request_not_sent(), "{error}");
}

/// How `Early` uses a send beside the chain before it forwards the request.
#[derive(Clone, Copy, Debug)]
enum Early {
    /// A direct send to `Http`, created with the attempt's metadata and dropped unpolled.
    DropHttp,
    /// The same with a `reqwest::Client`.
    DropClient,
    /// The same with a `fake`.
    DropFake,
    /// A direct send to a `fake` whose callback panics, the panic caught.
    PanicFake,
    /// A send through the chain whose next layer panics when it is called, the panic caught.
    PanicLayer,
}

struct Before(Early);

impl Middleware for Before {
    async fn handle(
        &self,
        request: reqwest::Request,
        info: TryInfo,
        next: Next<'_>,
    ) -> Result<reqwest::Response, TransportError> {
        let copy = request.try_clone().unwrap();
        let panics: Answering = |_| panic!("the callback panics");
        let answers: Answering = |_| Ok(concord_core::reply(200, ""));
        match self.0 {
            Early::DropHttp => drop(http().send(copy, info.clone())),
            Early::DropClient => drop(reqwest::Client::new().send(copy, info.clone())),
            Early::DropFake => drop(concord_core::fake(answers).send(copy, info.clone())),
            Early::PanicFake => {
                let fake = concord_core::fake(panics);
                let mut sending = Box::pin(fake.send(copy, info.clone()));
                let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let _ = sending.as_mut().now_or_never();
                }));
                assert!(caught.is_err());
            }
            Early::PanicLayer => {
                let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    drop(next.send(copy, info.clone()));
                }));
                assert!(caught.is_err());
            }
        }
        next.send(request, info).await
    }
}

/// A layer whose `handle` runs code of its own as soon as it is called, and panics the
/// first time.
struct Eager(std::sync::atomic::AtomicBool);

impl Middleware for Eager {
    fn handle(
        &self,
        request: reqwest::Request,
        info: TryInfo,
        next: Next<'_>,
    ) -> impl std::future::Future<Output = Result<reqwest::Response, TransportError>> + Send {
        assert!(self.0.swap(true, Ordering::SeqCst), "the first call panics");
        next.send(request, info)
    }
}

#[test]
fn c9_6_a_send_is_recorded_before_any_code_of_the_authors_runs() {
    for early in [
        Early::DropHttp,
        Early::DropClient,
        Early::DropFake,
        Early::PanicFake,
        Early::PanicLayer,
    ] {
        let error = run(async {
            let mut core = Core::<()>::default().with_middleware(Before(early));
            if let Early::PanicLayer = early {
                core = core.with_middleware(Eager(std::sync::atomic::AtomicBool::new(false)));
            }
            let core = core.with_transport(Fails(|| TransportError::connect().not_sent()));
            post(&core, "http://identity.test/token").await
        });
        assert!(!error.request_not_sent(), "{early:?}");
    }
}

//! The server of these tests: axum over hyper and h2, on `127.0.0.1:0`, serving HTTP/1.1 and
//! HTTP/2 without TLS (h2 by prior knowledge) on one port, and what the tests need around it.
//!
//! - [`Server`] records every request that reaches a route ([`Arrival`]), and shuts down
//!   gracefully on demand, as `axum::serve` does: hyper sends `GOAWAY` on HTTP/2.
//! - [`Link`] is one accepted connection. A handler can cut it ([`Link::cut`]): from then on
//!   the socket refuses every read and write, so hyper drops the connection without writing
//!   another byte. [`after_head`] is a body that breaks, or ends short, once its head has
//!   been written.
//! - [`Feed`] is a body or an event stream whose pieces the test sends one by one.
//! - [`Frozen`], [`Probe`] and [`on_paused_clock`] run a test on Tokio's paused clock with
//!   real sockets: see [`Frozen`].

use axum::extract::connect_info::Connected;
use axum::extract::{ConnectInfo, Request, State};
use axum::middleware::Next;
use axum::response::Response;
use axum::serve::{IncomingStream, Listener};
use axum::Router;
use bytes::Bytes;
use concord::http::{HeaderMap, Method, Version};
use concord::transport::{TransportError, TryInfo};
use concord::{reqwest, Middleware};
use futures_core::Stream;
use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::Instant;

/// The version of HTTP that a client speaks to the server.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wire {
    /// HTTP/1.1: `concord::Http` as it is, with no proxy.
    One,
    /// HTTP/2 without TLS, by prior knowledge: `Http::with(|b| b.http2_prior_knowledge())`.
    Two,
}

impl Wire {
    /// Concord's own client for this version. `no_proxy()` keeps a proxy of the environment out
    /// of a test on loopback; every other setting is `concord::Http`'s own.
    pub fn http(self) -> concord::Http {
        self.http_with(|builder| builder)
    }

    /// Concord's own client for this version, with more settings.
    pub fn http_with(
        self,
        more: impl FnOnce(reqwest::ClientBuilder) -> reqwest::ClientBuilder,
    ) -> concord::Http {
        concord::Http::with(move |builder| {
            let builder = more(builder.no_proxy());
            match self {
                Wire::One => builder.http1_only(),
                Wire::Two => builder.http2_prior_knowledge(),
            }
        })
        .expect("the HTTP client builds")
    }

    /// The version that the server sees.
    pub fn version(self) -> Version {
        match self {
            Wire::One => Version::HTTP_11,
            Wire::Two => Version::HTTP_2,
        }
    }
}

/// One request as it reached the server, before its route ran.
#[derive(Clone, Debug)]
pub struct Arrival {
    /// The method.
    pub method: Method,
    /// The path and the query, as the request line or `:path` gave them.
    pub target: String,
    /// The version of HTTP.
    pub version: Version,
    /// The connection it came on, numbered from 1 in the order of acceptance.
    pub connection: u64,
    /// The headers.
    pub headers: HeaderMap,
}

/// What reached the server, shared by its routes and the test.
#[derive(Clone)]
pub struct Seen {
    arrivals: Arc<Mutex<Vec<Arrival>>>,
    count: watch::Sender<usize>,
}

impl Default for Seen {
    fn default() -> Self {
        Seen {
            arrivals: Arc::default(),
            count: watch::channel(0).0,
        }
    }
}

impl Seen {
    /// Every request so far, in the order of arrival.
    pub fn all(&self) -> Vec<Arrival> {
        self.arrivals.lock().unwrap().clone()
    }

    /// How many requests have arrived.
    pub fn len(&self) -> usize {
        *self.count.borrow()
    }

    /// The targets of every request so far.
    pub fn targets(&self) -> Vec<String> {
        self.all()
            .into_iter()
            .map(|arrival| arrival.target)
            .collect()
    }

    /// Waits until at least `n` requests have arrived.
    pub async fn until(&self, n: usize) {
        let mut count = self.count.subscribe();
        count.wait_for(|seen| *seen >= n).await.unwrap();
    }

    fn push(&self, arrival: Arrival) {
        let mut arrivals = self.arrivals.lock().unwrap();
        arrivals.push(arrival);
        self.count.send_replace(arrivals.len());
    }
}

/// One accepted connection.
#[derive(Default)]
pub struct Link {
    id: u64,
    cut: AtomicBool,
    written: AtomicU64,
    wakers: Mutex<Vec<Waker>>,
}

impl Link {
    /// Cuts the connection: every later read and write of its socket fails, so hyper drops it
    /// without writing another byte. What was already written stays written.
    pub fn cut(&self) {
        self.cut.store(true, Ordering::SeqCst);
        self.wake();
    }

    /// The bytes written to the socket so far.
    pub fn written(&self) -> u64 {
        self.written.load(Ordering::SeqCst)
    }

    fn is_cut(&self) -> bool {
        self.cut.load(Ordering::SeqCst)
    }

    fn park(&self, waker: &Waker) {
        let mut wakers = self.wakers.lock().unwrap();
        if !wakers.iter().any(|known| known.will_wake(waker)) {
            wakers.push(waker.clone());
        }
    }

    fn wake(&self) {
        for waker in std::mem::take(&mut *self.wakers.lock().unwrap()) {
            waker.wake();
        }
    }
}

/// The peer of a request, as `ConnectInfo<Peer>` gives it to a handler.
#[derive(Clone)]
pub struct Peer {
    /// The client's address.
    pub addr: SocketAddr,
    /// The connection.
    pub link: Arc<Link>,
}

impl std::fmt::Debug for Peer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Peer")
            .field("addr", &self.addr)
            .field("connection", &self.link.id)
            .finish()
    }
}

impl Connected<IncomingStream<'_, Listening>> for Peer {
    fn connect_info(stream: IncomingStream<'_, Listening>) -> Self {
        stream.remote_addr().clone()
    }
}

/// The socket of one connection: a `TcpStream` that its [`Link`] can cut.
pub struct Conn {
    tcp: TcpStream,
    link: Arc<Link>,
}

fn refused() -> io::Error {
    io::Error::new(
        io::ErrorKind::ConnectionAborted,
        "the test cut the connection",
    )
}

impl AsyncRead for Conn {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.link.is_cut() {
            return Poll::Ready(Err(refused()));
        }
        self.link.park(cx.waker());
        Pin::new(&mut self.tcp).poll_read(cx, buf)
    }
}

impl AsyncWrite for Conn {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        if self.link.is_cut() {
            return Poll::Ready(Err(refused()));
        }
        let written = Pin::new(&mut self.tcp).poll_write(cx, buf);
        if let Poll::Ready(Ok(n)) = written {
            self.link.written.fetch_add(n as u64, Ordering::SeqCst);
            self.link.wake();
        }
        written
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        if self.link.is_cut() {
            return Poll::Ready(Err(refused()));
        }
        let written = Pin::new(&mut self.tcp).poll_write_vectored(cx, bufs);
        if let Poll::Ready(Ok(n)) = written {
            self.link.written.fetch_add(n as u64, Ordering::SeqCst);
            self.link.wake();
        }
        written
    }

    fn is_write_vectored(&self) -> bool {
        self.tcp.is_write_vectored()
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if self.link.is_cut() {
            return Poll::Ready(Err(refused()));
        }
        Pin::new(&mut self.tcp).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.tcp).poll_shutdown(cx)
    }
}

/// The listener of the server: it numbers its connections and lets a handler cut one.
pub struct Listening {
    tcp: TcpListener,
    accepted: u64,
}

impl Listener for Listening {
    type Io = Conn;
    type Addr = Peer;

    async fn accept(&mut self) -> (Conn, Peer) {
        loop {
            if let Ok((tcp, addr)) = self.tcp.accept().await {
                // Small frames leave at once, as most servers set it: no wait for an
                // acknowledgement of the frame before.
                let _ = tcp.set_nodelay(true);
                self.accepted += 1;
                let link = Arc::new(Link {
                    id: self.accepted,
                    ..Link::default()
                });
                let peer = Peer {
                    addr,
                    link: link.clone(),
                };
                return (Conn { tcp, link }, peer);
            }
        }
    }

    fn local_addr(&self) -> io::Result<Peer> {
        Ok(Peer {
            addr: self.tcp.local_addr()?,
            link: Arc::default(),
        })
    }
}

/// A server on `127.0.0.1`, serving a router.
pub struct Server {
    addr: SocketAddr,
    /// What reached it.
    pub seen: Seen,
    stop: Option<oneshot::Sender<()>>,
    done: Option<tokio::task::JoinHandle<()>>,
}

impl Server {
    /// Starts `router` on a port of its own. Every request is recorded before its route runs.
    pub async fn start(router: Router) -> Server {
        let tcp = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = tcp.local_addr().unwrap();
        let seen = Seen::default();
        let router = router.layer(axum::middleware::from_fn_with_state(seen.clone(), record));
        let (stop, stopped) = oneshot::channel::<()>();
        let listening = Listening { tcp, accepted: 0 };
        let done = tokio::spawn(async move {
            axum::serve(
                listening,
                router.into_make_service_with_connect_info::<Peer>(),
            )
            .with_graceful_shutdown(async move {
                let _ = stopped.await;
            })
            .await
            .unwrap();
        });
        Server {
            addr,
            seen,
            stop: Some(stop),
            done: Some(done),
        }
    }

    /// `http://127.0.0.1:<port>`, for `with_base`.
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Starts a graceful shutdown: the server accepts no connection, finishes the requests in
    /// flight, and closes each connection (HTTP/2: `GOAWAY`, then the close).
    pub fn shutdown(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
    }

    /// Waits until a shutdown has closed every connection and the port.
    pub async fn stopped(&mut self) {
        self.shutdown();
        if let Some(done) = self.done.take() {
            done.await.unwrap();
        }
    }
}

async fn record(
    State(seen): State<Seen>,
    ConnectInfo(peer): ConnectInfo<Peer>,
    request: Request,
    next: Next,
) -> Response {
    seen.push(Arrival {
        method: request.method().clone(),
        target: request
            .uri()
            .path_and_query()
            .map(|target| target.as_str().to_owned())
            .unwrap_or_default(),
        version: request.version(),
        connection: peer.link.id,
        headers: request.headers().clone(),
    });
    next.run(request).await
}

/// A gate that holds handlers until the test opens it.
#[derive(Clone)]
pub struct Gate(watch::Sender<bool>);

impl Default for Gate {
    fn default() -> Self {
        Gate(watch::channel(false).0)
    }
}

impl Gate {
    /// Opens the gate, for good.
    pub fn open(&self) {
        self.0.send_replace(true);
    }

    /// Waits until the gate is open.
    pub async fn wait(&self) {
        self.0.subscribe().wait_for(|open| *open).await.unwrap();
    }
}

/// A stream that a test feeds piece by piece: a body, or the events of `Sse`. It counts how
/// often the server polled it.
pub struct Feed<T> {
    pieces: mpsc::UnboundedReceiver<Result<T, io::Error>>,
    polls: watch::Sender<u64>,
}

/// The test's end of a [`Feed`].
pub struct Feeder<T> {
    pieces: mpsc::UnboundedSender<Result<T, io::Error>>,
    polls: watch::Receiver<u64>,
}

impl<T> Feed<T> {
    /// A new stream and its feeder. The stream ends when the feeder is dropped.
    pub fn new() -> (Feeder<T>, Feed<T>) {
        let (send, pieces) = mpsc::unbounded_channel();
        let (polls, seen) = watch::channel(0);
        (
            Feeder {
                pieces: send,
                polls: seen,
            },
            Feed { pieces, polls },
        )
    }
}

impl<T> Feeder<T> {
    /// Sends the next piece.
    pub fn send(&self, piece: T) {
        self.pieces.send(Ok(piece)).unwrap();
    }

    /// Breaks the stream: the server aborts the body (HTTP/1.1: it closes the connection;
    /// HTTP/2: it resets the stream).
    pub fn fail(&self) {
        self.pieces
            .send(Err(io::Error::other("the test broke the body")))
            .unwrap();
    }

    /// How often the server has polled the stream so far.
    pub fn polls(&self) -> u64 {
        *self.polls.borrow()
    }

    /// Waits until the server polls the stream again, after `polls`.
    pub async fn polled_after(&mut self, polls: u64) {
        self.polls
            .wait_for(|now| *now > polls)
            .await
            .expect("the server dropped the stream: the client let its response go");
    }
}

impl<T> Stream for Feed<T> {
    type Item = Result<T, io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.polls.send_modify(|polls| *polls += 1);
        self.pieces.poll_recv(cx)
    }
}

/// How a body of [`after_head`] ends, once its head is on the socket.
#[derive(Clone, Copy, Debug)]
pub enum Ending {
    /// The body fails: hyper aborts it (HTTP/1.1: it closes the connection; HTTP/2: it
    /// resets the stream).
    Break,
    /// The body ends cleanly, whatever length its head announced.
    Stop,
}

/// A body that sends `first`, then ends as `ending` says once the socket has written
/// something more (the head, or the head and `first`): the client has the head of the
/// response, and not the whole body that it announced. Without the wait, hyper can find the
/// end of the body before it writes the head, and then writes nothing.
pub fn after_head(link: Arc<Link>, first: Bytes, ending: Ending) -> axum::body::Body {
    axum::body::Body::from_stream(AfterHead {
        link,
        first: Some(first),
        written: 0,
        ending,
    })
}

struct AfterHead {
    link: Arc<Link>,
    first: Option<Bytes>,
    written: u64,
    ending: Ending,
}

impl Stream for AfterHead {
    type Item = Result<Bytes, io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(first) = self.first.take() {
            self.written = self.link.written();
            return Poll::Ready(Some(Ok(first)));
        }
        self.link.park(cx.waker());
        if self.link.written() <= self.written {
            return Poll::Pending;
        }
        match self.ending {
            Ending::Break => Poll::Ready(Some(Err(io::Error::other("the test cut the body")))),
            Ending::Stop => Poll::Ready(None),
        }
    }
}

/// Holds Tokio's paused clock still while a test uses real sockets.
///
/// On the paused clock, Tokio moves time forward by itself whenever the runtime has nothing to
/// do, and a runtime waiting for a socket has nothing to do: the idle timeout of a call would
/// expire while its response is still on its way. While a blocking task runs, Tokio does not
/// move the paused clock by itself (current-thread runtime), so a `Frozen` holds one until it
/// is dropped, and the test moves the clock with `tokio::time::advance`.
pub struct Frozen {
    _held: std::sync::mpsc::Sender<()>,
}

impl Frozen {
    /// Holds the clock from now on. The test must run with `start_paused = true`.
    pub fn hold() -> Frozen {
        let (held, release) = std::sync::mpsc::channel::<()>();
        drop(tokio::task::spawn_blocking(move || {
            let _ = release.recv();
        }));
        Frozen { _held: held }
    }
}

/// What a [`Probe`] saw: each attempt that it handed on, and when its response head (or its
/// failure) came back, on the clock of the test.
#[derive(Clone, Debug, Default)]
pub struct Probed {
    /// When each attempt was handed on.
    pub sent: Vec<Instant>,
    /// When each attempt came back.
    pub back: Vec<Instant>,
}

/// A middleware of the caller's that watches the attempts that pass through it.
#[derive(Clone)]
pub struct Probe(Arc<Mutex<Probed>>, watch::Sender<u64>);

impl Default for Probe {
    fn default() -> Self {
        Probe(Arc::default(), watch::channel(0).0)
    }
}

impl Probe {
    /// What it saw so far.
    pub fn seen(&self) -> Probed {
        self.0.lock().unwrap().clone()
    }

    /// The attempts that it handed on and that have not come back yet.
    pub fn on_the_wire(&self) -> usize {
        let seen = self.0.lock().unwrap();
        seen.sent.len() - seen.back.len()
    }

    /// Waits until it has handed on at least `n` attempts.
    pub async fn until_sent(&self, n: usize) {
        let probe = self.clone();
        self.1
            .subscribe()
            .wait_for(move |_| probe.0.lock().unwrap().sent.len() >= n)
            .await
            .unwrap();
    }

    fn note(&self, change: impl FnOnce(&mut Probed)) {
        change(&mut self.0.lock().unwrap());
        self.1.send_modify(|n| *n += 1);
    }
}

impl Middleware for Probe {
    async fn handle(
        &self,
        request: reqwest::Request,
        info: TryInfo,
        next: concord::Next<'_>,
    ) -> Result<reqwest::Response, TransportError> {
        self.note(|seen| seen.sent.push(Instant::now()));
        let back = next.send(request, info).await;
        self.note(|seen| seen.back.push(Instant::now()));
        back
    }
}

/// Runs `work` to its end on the paused clock held by a [`Frozen`]. The clock moves by `step`
/// only when every attempt that `probe` handed on has come back, so that it never moves while
/// a request or a response head is on the socket; then what `work` waits for is a timer.
/// `most` bounds how far the clock moves: it must stay below the idle timeout of the calls.
pub async fn on_paused_clock<T: Send + 'static>(
    probe: &Probe,
    step: Duration,
    most: Duration,
    work: impl Future<Output = T> + Send + 'static,
) -> T {
    let start = Instant::now();
    let mut task = tokio::spawn(work);
    let mut changed = probe.1.subscribe();
    loop {
        // Let the work go as far as it can without the clock: each yield also polls the
        // sockets.
        for _ in 0..16 {
            tokio::task::yield_now().await;
        }
        if task.is_finished() {
            return (&mut task).await.unwrap();
        }
        if probe.on_the_wire() > 0 {
            tokio::select! {
                out = &mut task => return out.unwrap(),
                _ = changed.changed() => {}
            }
            continue;
        }
        assert!(
            start.elapsed() < most,
            "the work did not end within {most:?} of the paused clock"
        );
        tokio::time::advance(step).await;
    }
}

/// Lets the runtime poll the sockets and run what they woke, without moving the clock.
pub async fn settle() {
    for _ in 0..16 {
        tokio::task::yield_now().await;
    }
}

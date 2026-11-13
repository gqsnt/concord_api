//! A real local HTTP/1.1 server that a test scripts.
//!
//! The server reads requests itself, byte by byte, so that it can record them exactly as
//! they arrived (header names in their case, in their order, a repeated name repeated,
//! each line as sent), and write answers that misbehave on purpose.

use bytes::{Bytes, BytesMut};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio::task::{JoinHandle, JoinSet};

#[cfg(test)]
mod tests;

/// The largest head of a request that the server reads, without its last empty line.
const MAX_HEAD: usize = 64 * 1024;
/// The largest line of chunked framing that the server reads, without its CRLF.
const MAX_LINE: usize = 4 * 1024;
/// The largest body of a request that the server records; a larger one gets a `413`.
pub const MAX_REQUEST_BODY: usize = 64 * 1024 * 1024;
/// The size of each write of a large body.
const FILL_SIZE: usize = 64 * 1024;
/// The bytes of a large body: `x`, repeated.
static FILL: [u8; FILL_SIZE] = [b'x'; FILL_SIZE];

/// A real HTTP/1.1 server on `127.0.0.1`, scripted by a test. Dropping it stops it and
/// closes every connection.
///
/// Each rule (`server.on(method, path)...`) answers one request, in the order the rules
/// were written. A request that no rule matches gets a `599` and is recorded.
pub struct Server {
    addr: SocketAddr,
    shared: Arc<Shared>,
    accept: JoinHandle<()>,
}

impl std::fmt::Debug for Server {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Server")
            .field("addr", &self.addr)
            .finish_non_exhaustive()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        // The accept task owns the connections: aborting it aborts them.
        self.accept.abort();
    }
}

struct Shared {
    state: Mutex<State>,
    /// Told each time a request arrives.
    arrived: watch::Sender<()>,
}

impl Default for Shared {
    fn default() -> Shared {
        Shared {
            state: Mutex::default(),
            arrived: watch::Sender::new(()),
        }
    }
}

#[derive(Default)]
struct State {
    rules: Vec<Rule>,
    requests: Vec<Slot>,
    /// The first request that [`Server::next_request`] has not given yet.
    next: usize,
}

/// A recorded request, and its body while it comes.
struct Slot {
    request: Request,
    growing: BytesMut,
}

impl Slot {
    fn snapshot(&self) -> Request {
        let mut request = self.request.clone();
        if !self.growing.is_empty() {
            request.body = Bytes::copy_from_slice(&self.growing);
        }
        request
    }
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        // A panic elsewhere must not hide what was recorded.
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Record a request whose head arrived; its index.
    fn record(&self, request: Request) -> usize {
        let index = {
            let mut state = self.lock();
            state.requests.push(Slot {
                request,
                growing: BytesMut::new(),
            });
            state.requests.len() - 1
        };
        self.arrived.send_replace(());
        index
    }

    /// More of the body of the request at `index`.
    fn append(&self, index: usize, piece: &[u8]) {
        if let Some(slot) = self.lock().requests.get_mut(index) {
            slot.growing.extend_from_slice(piece);
        }
    }

    /// The body of the request at `index` ended or broke.
    fn finish(&self, index: usize, state: BodyState, trailers: Fields) {
        if let Some(slot) = self.lock().requests.get_mut(index) {
            slot.request.body = slot.growing.split().freeze();
            slot.request.body_state = state;
            slot.request.trailers = trailers.tidy;
            slot.request.raw_trailers = trailers.raw;
        }
    }
}

struct Rule {
    method: String,
    path: String,
    action: Action,
}

impl Rule {
    fn matches(&self, method: &str, target: &str) -> bool {
        if self.method != method {
            return false;
        }
        if self.path.contains('?') {
            self.path == target
        } else {
            self.path == path_of(target)
        }
    }
}

enum Action {
    Close,
    Reply(Spec),
}

#[derive(Default)]
struct Spec {
    status: u16,
    headers: Vec<(String, String)>,
    /// Pauses and gates before the head.
    hold: Vec<Step>,
    steps: Vec<Step>,
    declared: Option<u64>,
    chunked: bool,
    silence: bool,
    malformed: bool,
}

enum Step {
    Bytes(Bytes),
    Large(u64),
    Pause(Duration),
    Wait(Gate),
}

impl Server {
    /// Listen on `127.0.0.1:0`.
    ///
    /// # Panics
    ///
    /// When no port can be opened on `127.0.0.1`, or outside a Tokio runtime.
    pub async fn start() -> Server {
        let listener = match TcpListener::bind(("127.0.0.1", 0)).await {
            Ok(listener) => listener,
            Err(error) => panic!("concord-testkit: cannot listen on 127.0.0.1: {error}"),
        };
        let addr = match listener.local_addr() {
            Ok(addr) => addr,
            Err(error) => panic!("concord-testkit: the server has no address: {error}"),
        };
        let shared = Arc::new(Shared::default());
        let accept = tokio::spawn(accept(listener, Arc::clone(&shared)));
        Server {
            addr,
            shared,
            accept,
        }
    }

    /// `http://127.0.0.1:<port>`, without a slash at the end.
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// The address the server listens on.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// The next request with this method (exact, case-sensitive) and this path gets what
    /// follows. The path is compared without the query, unless `path` holds a `?`: then
    /// the path and query are compared exactly as sent.
    ///
    /// Each rule answers once, when the body of its request has ended. Rules for the same
    /// route answer in the order they were written: a `503` then a `200` is two rules.
    pub fn on(&self, method: &str, path: &str) -> On<'_> {
        On {
            shared: &self.shared,
            method: method.to_owned(),
            path: path.to_owned(),
        }
    }

    /// Every request that arrived, in the order their heads arrived, those that no rule
    /// matched included. A request is recorded as soon as its head has arrived, with what
    /// came of its body so far ([`Request::body_state`]).
    pub fn requests(&self) -> Vec<Request> {
        self.shared
            .lock()
            .requests
            .iter()
            .map(Slot::snapshot)
            .collect()
    }

    /// Wait until a request arrives, and give it: the first call gives the first request
    /// that arrived, the next call the second, and so on, whether it arrived before the
    /// call or not. Its head is complete; its body is as far as it had come
    /// ([`Request::body_state`]): read [`Server::requests`] later for the rest.
    pub async fn next_request(&self) -> Request {
        let mut arrived = self.shared.arrived.subscribe();
        loop {
            {
                let mut state = self.shared.lock();
                let index = state.next;
                if let Some(request) = state.requests.get(index).map(Slot::snapshot) {
                    state.next = index + 1;
                    return request;
                }
            }
            // The sender lives in `self`: the channel cannot close while we wait.
            if arrived.changed().await.is_err() {
                std::future::pending::<()>().await;
            }
        }
    }

    /// How many rules have not answered yet.
    pub fn unused(&self) -> usize {
        self.shared.lock().rules.len()
    }
}

/// A route of a [`Server`] that waits for what to do: [`On::reply`] or [`On::close`].
#[must_use = "a rule does nothing until `.reply(status)` or `.close()`"]
pub struct On<'a> {
    shared: &'a Shared,
    method: String,
    path: String,
}

impl std::fmt::Debug for On<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("On")
            .field("method", &self.method)
            .field("path", &self.path)
            .finish()
    }
}

impl<'a> On<'a> {
    /// Answer with this status. The rule is added when the [`Reply`] is dropped, at the
    /// end of the statement.
    ///
    /// # Panics
    ///
    /// When `status` is not between 100 and 999.
    pub fn reply(self, status: u16) -> Reply<'a> {
        assert!(
            (100..=999).contains(&status),
            "concord-testkit: {status} is not a status code (100 to 999)"
        );
        Reply {
            shared: self.shared,
            method: self.method,
            path: self.path,
            spec: Spec {
                status,
                ..Spec::default()
            },
        }
    }

    /// Read the request, record it, and close the connection before any byte of an answer.
    pub fn close(self) {
        self.shared.lock().rules.push(Rule {
            method: self.method,
            path: self.path,
            action: Action::Close,
        });
    }
}

/// An answer being scripted. It is added to the server when dropped, at the end of the
/// statement.
///
/// By default the server frames the body with a `content-length` that it computes, and
/// keeps the connection open. [`Reply::chunked`], [`Reply::declare_length`] and
/// [`Reply::silence`] change that. For `HEAD`, the head only is written. For `1xx`,
/// `204` and `304`, neither a framing header nor a body is written, whatever the script
/// says, unless it asks for a malformed answer with [`Reply::malformed_with_body`].
pub struct Reply<'a> {
    shared: &'a Shared,
    method: String,
    path: String,
    spec: Spec,
}

impl std::fmt::Debug for Reply<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reply")
            .field("method", &self.method)
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl Drop for Reply<'_> {
    fn drop(&mut self) {
        self.shared.lock().rules.push(Rule {
            method: std::mem::take(&mut self.method),
            path: std::mem::take(&mut self.path),
            action: Action::Reply(std::mem::take(&mut self.spec)),
        });
    }
}

impl Reply<'_> {
    /// A header, written as given, after those before it. A name may repeat.
    ///
    /// # Panics
    ///
    /// When the name is `content-length` or `transfer-encoding` (the server frames the
    /// body: use [`Reply::declare_length`] or [`Reply::chunked`]), or when the name or the
    /// value holds a line break.
    pub fn header(mut self, name: &str, value: &str) -> Self {
        assert!(
            !name.eq_ignore_ascii_case("content-length")
                && !name.eq_ignore_ascii_case("transfer-encoding"),
            "concord-testkit: the server writes `{name}` itself; use `.declare_length(n)` or `.chunked()`"
        );
        assert!(
            !name.contains(['\r', '\n']) && !value.contains(['\r', '\n']),
            "concord-testkit: a header holds a line break: {name:?}: {value:?}"
        );
        self.spec.headers.push((name.to_owned(), value.to_owned()));
        self
    }

    /// Hold the head back for this much of Tokio's time (real time, unless the clock is
    /// paused), counted from the end of the request's body. Calls of `after` and
    /// [`Reply::after_gate`] are kept in the order they were written.
    pub fn after(mut self, duration: Duration) -> Self {
        self.spec.hold.push(Step::Pause(duration));
        self
    }

    /// Hold the head back until the test opens `gate`. For a test that waits on events,
    /// not on time.
    pub fn after_gate(mut self, gate: &Gate) -> Self {
        self.spec.hold.push(Step::Wait(gate.clone()));
        self
    }

    /// A piece of the body. Each piece is written as one write; call again for the next.
    pub fn body(mut self, piece: impl Into<Bytes>) -> Self {
        self.spec.steps.push(Step::Bytes(piece.into()));
        self
    }

    /// `size` bytes of `x` in the body, written in pieces of 64 KiB and never held in
    /// memory at once.
    pub fn large_body(mut self, size: u64) -> Self {
        self.spec.steps.push(Step::Large(size));
        self
    }

    /// A pause in the body, before what follows, of Tokio's time (real time, unless the
    /// clock is paused).
    pub fn pause(mut self, duration: Duration) -> Self {
        self.spec.steps.push(Step::Pause(duration));
        self
    }

    /// Wait until the test opens `gate` before what follows in the body. For a test that
    /// waits on events, not on time.
    pub fn wait(mut self, gate: &Gate) -> Self {
        self.spec.steps.push(Step::Wait(gate.clone()));
        self
    }

    /// Frame the body with `transfer-encoding: chunked`: each piece is one chunk.
    pub fn chunked(mut self) -> Self {
        self.spec.chunked = true;
        self
    }

    /// Write `content-length: length` whatever the body is, write the body as it is, then
    /// close the connection. With a body shorter than `length`: a body that ends before
    /// its announced size.
    pub fn declare_length(mut self, length: u64) -> Self {
        self.spec.declared = Some(length);
        self
    }

    /// A malformed answer: write the framing header and the body as scripted even where
    /// HTTP forbids them (a `1xx`, `204` or `304`, or an answer to `HEAD`).
    pub fn malformed_with_body(mut self) -> Self {
        self.spec.malformed = true;
        self
    }

    /// After what was scripted, write nothing more and never close, until the server is
    /// dropped. Without [`Reply::declare_length`], the body is chunked, so that the
    /// client waits for more: `.reply(200).silence()` is a head and then silence.
    pub fn silence(mut self) -> Self {
        self.spec.silence = true;
        self
    }
}

/// A gate that a [`Reply`] waits on ([`Reply::wait`], [`Reply::after_gate`]) until the
/// test opens it.
#[derive(Clone)]
pub struct Gate {
    open: Arc<watch::Sender<bool>>,
}

impl std::fmt::Debug for Gate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Gate")
            .field("open", &*self.open.borrow())
            .finish()
    }
}

impl Default for Gate {
    fn default() -> Gate {
        Gate::new()
    }
}

impl Gate {
    /// A closed gate.
    pub fn new() -> Gate {
        Gate {
            open: Arc::new(watch::Sender::new(false)),
        }
    }

    /// Open the gate; it stays open.
    pub fn open(&self) {
        self.open.send_replace(true);
    }

    async fn passed(&self) {
        let mut open = self.open.subscribe();
        // The sender lives in `self`: the channel cannot close while we wait.
        let _ = open.wait_for(|open| *open).await;
    }
}

/// What came of the body of a recorded [`Request`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyState {
    /// The head arrived; the body has not ended yet.
    Coming,
    /// The body ended as its framing said (or there was none).
    Ended,
    /// The body broke: the connection ended in the middle, its framing was malformed, or
    /// it was larger than [`MAX_REQUEST_BODY`].
    Broken,
}

/// A request as it arrived at a [`Server`].
#[derive(Clone, Debug)]
pub struct Request {
    method: String,
    target: String,
    version: String,
    headers: Vec<(String, Bytes)>,
    raw_headers: Vec<Bytes>,
    trailers: Vec<(String, Bytes)>,
    raw_trailers: Vec<Bytes>,
    body: Bytes,
    body_state: BodyState,
}

impl Request {
    /// The method, as sent: `GET`.
    pub fn method(&self) -> &str {
        &self.method
    }

    /// The path and query, exactly as sent: `/tasks?page=2`.
    pub fn target(&self) -> &str {
        &self.target
    }

    /// The path, without the query: `/tasks`.
    pub fn path(&self) -> &str {
        path_of(&self.target)
    }

    /// The query, without `?`: `page=2`. `None` without a `?`.
    pub fn query(&self) -> Option<&str> {
        self.target.split_once('?').map(|(_, query)| query)
    }

    /// The version, as sent: `HTTP/1.1`.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Every header, in the order it arrived, its name in the case it was sent, its value
    /// without the spaces around it. A name sent twice is here twice. For the bytes as
    /// sent: [`Request::raw_headers`].
    pub fn headers(&self) -> &[(String, Bytes)] {
        &self.headers
    }

    /// Every header line exactly as sent, in order, without its CRLF: `X-A:  1 `.
    pub fn raw_headers(&self) -> &[Bytes] {
        &self.raw_headers
    }

    /// The value of the first header with this name, the name compared without case.
    /// `None` when absent, or when that first value is not UTF-8 (see
    /// [`Request::headers`]).
    pub fn header(&self, name: &str) -> Option<&str> {
        let (_, value) = self
            .headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))?;
        std::str::from_utf8(value).ok()
    }

    /// Every value of a header that is UTF-8, in order, the name compared without case.
    pub fn header_all(&self, name: &str) -> Vec<&str> {
        self.headers
            .iter()
            .filter(|(n, _)| n.eq_ignore_ascii_case(name))
            .filter_map(|(_, value)| std::str::from_utf8(value).ok())
            .collect()
    }

    /// The trailers of a chunked body, in order, as [`Request::headers`] gives headers.
    /// Empty until the body has ended.
    pub fn trailers(&self) -> &[(String, Bytes)] {
        &self.trailers
    }

    /// Every trailer line exactly as sent, in order, without its CRLF.
    pub fn raw_trailers(&self) -> &[Bytes] {
        &self.raw_trailers
    }

    /// The bytes of the body that arrived, after chunked framing is taken off: all of
    /// them once the body has ended, what had come so far otherwise.
    pub fn body(&self) -> &[u8] {
        &self.body
    }

    /// Whether the body ended, broke, or is still coming.
    pub fn body_state(&self) -> BodyState {
        self.body_state
    }
}

fn path_of(target: &str) -> &str {
    match target.split_once('?') {
        Some((path, _)) => path,
        None => target,
    }
}

fn keep_alive(version: &str, headers: &[(String, Bytes)]) -> bool {
    version == "HTTP/1.1"
        && !headers
            .iter()
            .filter(|(name, _)| name.eq_ignore_ascii_case("connection"))
            .any(|(_, value)| {
                value
                    .split(|byte| *byte == b',')
                    .any(|token| trim(token).eq_ignore_ascii_case(b"close"))
            })
}

fn expects_continue(version: &str, headers: &[(String, Bytes)]) -> bool {
    version == "HTTP/1.1"
        && headers.iter().any(|(name, value)| {
            name.eq_ignore_ascii_case("expect") && value.eq_ignore_ascii_case(b"100-continue")
        })
}

async fn accept(listener: TcpListener, shared: Arc<Shared>) {
    let mut connections = JoinSet::new();
    loop {
        tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => {
                    // Each piece leaves at once, so that a client sees the pieces apart.
                    let _ = stream.set_nodelay(true);
                    connections.spawn(serve(stream, Arc::clone(&shared)));
                }
                // For example, too many open files: try again later.
                Err(_) => tokio::task::yield_now().await,
            },
            Some(_) = connections.join_next(), if !connections.is_empty() => {}
        }
    }
}

/// Why a request could not be read.
enum Bad {
    /// The bytes are not a request.
    Malformed,
    /// The body is larger than [`MAX_REQUEST_BODY`].
    TooLarge,
    /// The connection failed or ended in the middle.
    Gone,
}

/// Header or trailer fields: tidy, and each line as sent.
#[derive(Default)]
struct Fields {
    tidy: Vec<(String, Bytes)>,
    raw: Vec<Bytes>,
}

struct Connection<S> {
    stream: S,
    buf: BytesMut,
}

async fn serve<S>(stream: S, shared: Arc<Shared>)
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut connection = Connection {
        stream,
        buf: BytesMut::with_capacity(8 * 1024),
    };
    loop {
        let request = match connection.read_head().await {
            Ok(Some(request)) => request,
            Ok(None) | Err(Bad::Gone) => return,
            Err(Bad::Malformed | Bad::TooLarge) => {
                let _ = connection.write_error(400, "Bad Request").await;
                return;
            }
        };
        let framing = match framing(&request.headers) {
            Ok(framing) => Ok(framing),
            Err(Bad::Malformed | Bad::Gone) => {
                // A head whose framing cannot be read is not a request: not recorded.
                let _ = connection.write_error(400, "Bad Request").await;
                return;
            }
            Err(Bad::TooLarge) => Err(Bad::TooLarge),
        };
        let method = request.method.clone();
        let target = request.target.clone();
        let keep_alive = keep_alive(&request.version, &request.headers);
        let expects_continue = expects_continue(&request.version, &request.headers);
        let index = shared.record(request);

        let body = match framing {
            Err(bad) => Err(bad),
            Ok(Framing::Empty) => Ok(Fields::default()),
            Ok(framing) => {
                if expects_continue
                    && connection
                        .stream
                        .write_all(b"HTTP/1.1 100 Continue\r\n\r\n")
                        .await
                        .is_err()
                {
                    shared.finish(index, BodyState::Broken, Fields::default());
                    return;
                }
                connection.read_body(framing, &shared, index).await
            }
        };
        match body {
            Ok(trailers) => shared.finish(index, BodyState::Ended, trailers),
            Err(bad) => {
                shared.finish(index, BodyState::Broken, Fields::default());
                let _ = match bad {
                    Bad::Gone => return,
                    Bad::Malformed => connection.write_error(400, "Bad Request").await,
                    Bad::TooLarge => connection.write_error(413, "Content Too Large").await,
                };
                return;
            }
        }

        let head_only = method == "HEAD";
        let action = {
            let mut state = shared.lock();
            let found = state
                .rules
                .iter()
                .position(|rule| rule.matches(&method, &target));
            found.map(|index| state.rules.remove(index).action)
        };
        let open = match action {
            Some(Action::Close) => {
                let _ = connection.stream.shutdown().await;
                return;
            }
            Some(Action::Reply(spec)) => connection.write_reply(spec, head_only).await,
            None => {
                let unmatched = format!("concord-testkit: no rule for {method} {target}\n");
                connection.write_unmatched(unmatched, head_only).await
            }
        };
        if !(open && keep_alive) {
            let _ = connection.stream.shutdown().await;
            return;
        }
    }
}

impl<S> Connection<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    /// Read more bytes. `false` at the end of the stream.
    async fn fill(&mut self) -> Result<bool, Bad> {
        match self.stream.read_buf(&mut self.buf).await {
            Ok(0) => Ok(false),
            Ok(_) => Ok(true),
            Err(_) => Err(Bad::Gone),
        }
    }

    /// The head of the next request (its body is not read); `None` when the client
    /// closed between requests.
    async fn read_head(&mut self) -> Result<Option<Request>, Bad> {
        let mut from = 0;
        let end = loop {
            if let Some(at) = find(self.buf.get(from..).unwrap_or_default(), b"\r\n\r\n") {
                let at = from + at;
                if at > MAX_HEAD {
                    return Err(Bad::Malformed);
                }
                break at;
            }
            // A head of `MAX_HEAD` bytes may have up to three bytes of its delimiter here:
            // only a buffer that holds more without the delimiter is too long.
            if self.buf.len() >= MAX_HEAD + 4 {
                return Err(Bad::Malformed);
            }
            from = self.buf.len().saturating_sub(3);
            if !self.fill().await? {
                return if self.buf.is_empty() {
                    Ok(None)
                } else {
                    Err(Bad::Gone)
                };
            }
        };
        let head = self.buf.split_to(end + 4).freeze();
        parse_head(&head.slice(..end)).map(Some)
    }

    /// The body that `framing` announces, each piece added to the record at `index` as it
    /// comes; the trailers of a chunked body.
    async fn read_body(
        &mut self,
        framing: Framing,
        shared: &Shared,
        index: usize,
    ) -> Result<Fields, Bad> {
        match framing {
            Framing::Empty => Ok(Fields::default()),
            Framing::Length(length) => {
                let length = usize::try_from(length).map_err(|_| Bad::TooLarge)?;
                if length > MAX_REQUEST_BODY {
                    return Err(Bad::TooLarge);
                }
                self.pass(length, shared, index).await?;
                Ok(Fields::default())
            }
            Framing::Chunked => self.read_chunked(shared, index).await,
        }
    }

    /// Exactly `length` bytes of body, each piece added to the record as it comes.
    async fn pass(&mut self, mut length: usize, shared: &Shared, index: usize) -> Result<(), Bad> {
        loop {
            let take = length.min(self.buf.len());
            if take > 0 {
                let piece = self.buf.split_to(take);
                shared.append(index, &piece);
                length -= take;
            }
            if length == 0 {
                return Ok(());
            }
            if !self.fill().await? {
                return Err(Bad::Gone);
            }
        }
    }

    /// A line of chunked framing, without its CRLF.
    async fn line(&mut self) -> Result<Bytes, Bad> {
        let mut from = 0;
        loop {
            if let Some(at) = find(self.buf.get(from..).unwrap_or_default(), b"\r\n") {
                let at = from + at;
                if at > MAX_LINE {
                    return Err(Bad::Malformed);
                }
                let line = self.buf.split_to(at + 2).freeze();
                return Ok(line.slice(..at));
            }
            // A line of `MAX_LINE` bytes may have the CR of its CRLF here.
            if self.buf.len() >= MAX_LINE + 2 {
                return Err(Bad::Malformed);
            }
            from = self.buf.len().saturating_sub(1);
            if !self.fill().await? {
                return Err(Bad::Gone);
            }
        }
    }

    async fn read_chunked(&mut self, shared: &Shared, index: usize) -> Result<Fields, Bad> {
        let mut total = 0usize;
        loop {
            let line = self.line().await?;
            let size = line.split(|byte| *byte == b';').next().unwrap_or_default();
            let size = parse_hex(trim(size)).ok_or(Bad::Malformed)?;
            if size == 0 {
                return self.read_trailers().await;
            }
            let size = usize::try_from(size).map_err(|_| Bad::TooLarge)?;
            total = match total.checked_add(size) {
                Some(total) if total <= MAX_REQUEST_BODY => total,
                _ => return Err(Bad::TooLarge),
            };
            self.pass(size, shared, index).await?;
            if &self.line().await?[..] != b"" {
                return Err(Bad::Malformed);
            }
        }
    }

    async fn read_trailers(&mut self) -> Result<Fields, Bad> {
        let mut fields = Fields::default();
        let mut size = 0usize;
        loop {
            let line = self.line().await?;
            if line.is_empty() {
                return Ok(fields);
            }
            size = size.saturating_add(line.len() + 2);
            if size > MAX_HEAD {
                return Err(Bad::Malformed);
            }
            let (name, value) = parse_field(&line)?;
            fields.tidy.push((name, value));
            fields.raw.push(line);
        }
    }

    async fn write_error(&mut self, status: u16, reason: &str) -> std::io::Result<()> {
        let head =
            format!("HTTP/1.1 {status} {reason}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n");
        self.stream.write_all(head.as_bytes()).await?;
        self.stream.shutdown().await?;
        // Read what the client still sends until it closes: closing with bytes unread
        // would reset the connection, and the client could lose the answer.
        let mut unread = [0u8; 8 * 1024];
        while let Ok(1..) = self.stream.read(&mut unread).await {}
        Ok(())
    }

    /// A `599`. `true`: the connection may serve another request.
    async fn write_unmatched(&mut self, body: String, head_only: bool) -> bool {
        let head = format!(
            "HTTP/1.1 599 No Rule\r\ncontent-type: text/plain; charset=utf-8\r\ncontent-length: {}\r\n\r\n",
            body.len()
        );
        let mut bytes = head.into_bytes();
        if !head_only {
            bytes.extend_from_slice(body.as_bytes());
        }
        self.stream.write_all(&bytes).await.is_ok()
    }

    /// Write a scripted answer. `true`: the connection may serve another request.
    async fn write_reply(&mut self, spec: Spec, head_only: bool) -> bool {
        self.write_reply_inner(spec, head_only)
            .await
            .unwrap_or(false)
    }

    async fn write_reply_inner(&mut self, spec: Spec, head_only: bool) -> std::io::Result<bool> {
        let bodyless = matches!(spec.status, 100..=199 | 204 | 304) && !spec.malformed;
        let head_only = head_only && !spec.malformed;
        enum Frame {
            Declared(u64),
            None,
            Chunked,
            Length(u64),
        }
        let frame = if bodyless {
            Frame::None
        } else if let Some(length) = spec.declared {
            Frame::Declared(length)
        } else if spec.chunked || spec.silence {
            Frame::Chunked
        } else {
            let total = spec.steps.iter().fold(0u64, |total, step| match step {
                Step::Bytes(bytes) => total.saturating_add(bytes.len() as u64),
                Step::Large(size) => total.saturating_add(*size),
                Step::Pause(_) | Step::Wait(_) => total,
            });
            Frame::Length(total)
        };

        for step in spec.hold {
            match step {
                Step::Pause(duration) => tokio::time::sleep(duration).await,
                Step::Wait(gate) => gate.passed().await,
                Step::Bytes(_) | Step::Large(_) => {}
            }
        }

        let reason = http::StatusCode::from_u16(spec.status)
            .ok()
            .and_then(|status| status.canonical_reason())
            .unwrap_or("Scripted");
        let mut head = format!("HTTP/1.1 {} {reason}\r\n", spec.status);
        for (name, value) in &spec.headers {
            head.push_str(name);
            head.push_str(": ");
            head.push_str(value);
            head.push_str("\r\n");
        }
        match frame {
            Frame::Declared(length) | Frame::Length(length) => {
                head.push_str(&format!("content-length: {length}\r\n"));
            }
            Frame::Chunked => head.push_str("transfer-encoding: chunked\r\n"),
            Frame::None => {}
        }
        head.push_str("\r\n");
        self.stream.write_all(head.as_bytes()).await?;

        let chunked = matches!(frame, Frame::Chunked);
        if !head_only && !matches!(frame, Frame::None) {
            for step in spec.steps {
                match step {
                    Step::Bytes(bytes) => self.write_piece(&bytes, chunked).await?,
                    Step::Large(mut left) => {
                        while left > 0 {
                            let size = usize::try_from(left).unwrap_or(FILL_SIZE).min(FILL_SIZE);
                            self.write_piece(FILL.get(..size).unwrap_or_default(), chunked)
                                .await?;
                            left -= size as u64;
                        }
                    }
                    Step::Pause(duration) => tokio::time::sleep(duration).await,
                    Step::Wait(gate) => gate.passed().await,
                }
            }
        }
        self.stream.flush().await?;
        if spec.silence {
            std::future::pending::<()>().await;
        }
        if head_only {
            return Ok(!matches!(frame, Frame::Declared(_)));
        }
        match frame {
            Frame::Chunked => {
                self.stream.write_all(b"0\r\n\r\n").await?;
                Ok(true)
            }
            Frame::Declared(_) => Ok(false),
            Frame::None | Frame::Length(_) => Ok(true),
        }
    }

    async fn write_piece(&mut self, bytes: &[u8], chunked: bool) -> std::io::Result<()> {
        if bytes.is_empty() {
            // An empty chunk would end a chunked body.
            return Ok(());
        }
        if chunked {
            let mut framed = format!("{:x}\r\n", bytes.len()).into_bytes();
            framed.extend_from_slice(bytes);
            framed.extend_from_slice(b"\r\n");
            self.stream.write_all(&framed).await
        } else {
            self.stream.write_all(bytes).await
        }
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn trim(bytes: &[u8]) -> &[u8] {
    let is_space = |byte: &u8| *byte == b' ' || *byte == b'\t';
    let start = bytes
        .iter()
        .position(|b| !is_space(b))
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|b| !is_space(b))
        .map_or(start, |at| at + 1);
    bytes.get(start..end).unwrap_or_default()
}

fn parse_hex(bytes: &[u8]) -> Option<u64> {
    if bytes.is_empty() || bytes.len() > 16 || !bytes.iter().all(u8::is_ascii_hexdigit) {
        return None;
    }
    u64::from_str_radix(std::str::from_utf8(bytes).ok()?, 16).ok()
}

fn is_token(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte)
}

/// A part of `bytes`, shared with it; empty when out of range.
fn part(bytes: &Bytes, start: usize, end: usize) -> Bytes {
    if start <= end && end <= bytes.len() {
        bytes.slice(start..end)
    } else {
        Bytes::new()
    }
}

/// The lines of `bytes`, split at LF, each without its CR, each sharing `bytes`.
fn lines(bytes: &Bytes) -> Vec<Bytes> {
    let mut lines = Vec::new();
    let mut start = 0;
    loop {
        let rest = bytes.get(start..).unwrap_or_default();
        let end = match rest.iter().position(|byte| *byte == b'\n') {
            Some(at) => start + at,
            None => bytes.len(),
        };
        let line = part(bytes, start, end);
        lines.push(match line.strip_suffix(b"\r") {
            Some(_) => part(&line, 0, line.len() - 1),
            None => line,
        });
        if end >= bytes.len() {
            return lines;
        }
        start = end + 1;
    }
}

/// A header or trailer line: its name, and its value without the spaces around it.
fn parse_field(line: &Bytes) -> Result<(String, Bytes), Bad> {
    if line.contains(&b'\r') || line.contains(&0) {
        return Err(Bad::Malformed);
    }
    // A line that starts with a space folds the one before: not accepted.
    let colon = line
        .iter()
        .position(|byte| *byte == b':')
        .ok_or(Bad::Malformed)?;
    let name = line.get(..colon).unwrap_or_default();
    if name.is_empty() || !name.iter().all(|byte| is_token(*byte)) {
        return Err(Bad::Malformed);
    }
    let value = line.get(colon + 1..).unwrap_or_default();
    let start = colon + 1 + leading(value);
    let value = part(line, start, start + trim(value).len());
    let name = String::from_utf8(name.to_vec()).map_err(|_| Bad::Malformed)?;
    Ok((name, value))
}

/// How many spaces and tabs start `bytes`.
fn leading(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .take_while(|byte| **byte == b' ' || **byte == b'\t')
        .count()
}

fn parse_head(head: &Bytes) -> Result<Request, Bad> {
    let mut lines = lines(head).into_iter();
    let first = lines.next().ok_or(Bad::Malformed)?;
    let mut parts = first.split(|byte| *byte == b' ');
    let (Some(method), Some(target), Some(version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(Bad::Malformed);
    };
    if method.is_empty() || !method.iter().all(|byte| is_token(*byte)) {
        return Err(Bad::Malformed);
    }
    if target.is_empty() || !target.iter().all(|byte| byte.is_ascii_graphic()) {
        return Err(Bad::Malformed);
    }
    if version != b"HTTP/1.1" && version != b"HTTP/1.0" {
        return Err(Bad::Malformed);
    }
    let text = |bytes: &[u8]| String::from_utf8(bytes.to_vec()).map_err(|_| Bad::Malformed);

    let mut headers = Vec::new();
    let mut raw_headers = Vec::new();
    for line in lines {
        headers.push(parse_field(&line)?);
        raw_headers.push(line);
    }
    Ok(Request {
        method: text(method)?,
        target: text(target)?,
        version: text(version)?,
        headers,
        raw_headers,
        trailers: Vec::new(),
        raw_trailers: Vec::new(),
        body: Bytes::new(),
        body_state: BodyState::Coming,
    })
}

enum Framing {
    Empty,
    Length(u64),
    Chunked,
}

fn framing(headers: &[(String, Bytes)]) -> Result<Framing, Bad> {
    let mut encoding = None;
    let mut length = None;
    for (name, value) in headers {
        if name.eq_ignore_ascii_case("transfer-encoding") {
            encoding = Some(value);
        } else if name.eq_ignore_ascii_case("content-length") {
            let value = std::str::from_utf8(value).map_err(|_| Bad::Malformed)?;
            if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(Bad::Malformed);
            }
            let value: u64 = value.parse().map_err(|_| Bad::TooLarge)?;
            if length.is_some_and(|before| before != value) {
                return Err(Bad::Malformed);
            }
            length = Some(value);
        }
    }
    match (encoding, length) {
        (Some(_), Some(_)) => Err(Bad::Malformed),
        (Some(encoding), None) => {
            let last = encoding
                .rsplit(|byte| *byte == b',')
                .next()
                .unwrap_or_default();
            if trim(last).eq_ignore_ascii_case(b"chunked") {
                Ok(Framing::Chunked)
            } else {
                Err(Bad::Malformed)
            }
        }
        (None, Some(length)) if length > MAX_REQUEST_BODY as u64 => Err(Bad::TooLarge),
        (None, Some(length)) => Ok(Framing::Length(length)),
        (None, None) => Ok(Framing::Empty),
    }
}

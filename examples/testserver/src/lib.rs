//! A loopback HTTP/1.1 server for wire-level tests of the provider SDK examples, with owned tasks
//! and observable cleanup.
//!
//! It deliberately uses no HTTP server library, so tests observe what reqwest put on the wire (the
//! raw request target, header lines in order, the body bytes, whether the body was chunked). It
//! answers with a scripted [`Reply`] whose body can be complete, delivered in delayed chunks,
//! generated lazily (large streams are never materialized), cut off by a dropped connection
//! after a prefix, or stalled after its headers.
//!
//! # Lifecycle
//!
//! The listener and every connection belong to the server: the accept task owns a
//! [`tokio::task::JoinSet`] of connection tasks. [`TestServer::shutdown`] stops accepting, aborts
//! every connection task and **waits** for them, so when it returns every socket the server held
//! is closed, and it reports what was still open ([`Cleanup`]). Dropping a server without calling
//! `shutdown` aborts the accept task, which drops the set and aborts the connections (completed at
//! the runtime's next poll, or when a `#[tokio::test]` runtime shuts down). Nothing is detached:
//! no task outlives the server and its runtime, and no timer keeps a socket alive.
//!
//! Counters ([`TestServer::live_connections`], [`TestServer::active_bodies`],
//! [`TestServer::peak_active_bodies`]) make the server's own resource use observable in tests,
//! and [`TestServer::wait_until`] polls a condition with a bounded deadline (no fixed sleeps).

#![allow(missing_docs)]

use std::collections::VecDeque;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Notify, watch};
use tokio::task::{JoinHandle, JoinSet};

/// One request as observed on the wire.
#[derive(Debug, Clone)]
pub struct Recorded {
    pub method: String,
    /// The raw request target, e.g. `/v1/items/a%2Fb?tag=x&tag=y`.
    pub target: String,
    /// Header lines in wire order, names lowercased.
    pub headers: Vec<(String, String)>,
    /// The body (decoded if chunked), up to the server's recording limit.
    pub body: Vec<u8>,
    /// Total body bytes received (also past the recording limit).
    pub body_len: u64,
    /// Whether the body arrived with `transfer-encoding: chunked`.
    pub chunked: bool,
    /// The `Host` header, for routing assertions.
    pub host: String,
}

impl Recorded {
    pub fn path(&self) -> &str {
        self.target.split('?').next().unwrap_or("")
    }

    pub fn query(&self) -> Option<&str> {
        self.target.split_once('?').map(|(_, q)| q)
    }

    /// Decoded query pairs in order (`+` is a space).
    pub fn query_pairs(&self) -> Vec<(String, String)> {
        let Some(query) = self.query() else {
            return Vec::new();
        };
        query
            .split('&')
            .filter(|s| !s.is_empty())
            .map(|pair| {
                let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
                (decode(k), decode(v))
            })
            .collect()
    }

    /// All decoded values of one query parameter, in order.
    pub fn query_all(&self, name: &str) -> Vec<String> {
        self.query_pairs()
            .into_iter()
            .filter(|(k, _)| k == name)
            .map(|(_, v)| v)
            .collect()
    }

    /// All values of a header (case-insensitive name), in wire order.
    pub fn header_all(&self, name: &str) -> Vec<&str> {
        let name = name.to_ascii_lowercase();
        self.headers
            .iter()
            .filter(|(n, _)| *n == name)
            .map(|(_, v)| v.as_str())
            .collect()
    }

    /// The single value of a header; panics if it occurs more than once.
    pub fn header(&self, name: &str) -> Option<&str> {
        let all = self.header_all(name);
        assert!(all.len() <= 1, "header {name} occurs {} times", all.len());
        all.first().copied()
    }

    pub fn body_text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    /// Decoded `application/x-www-form-urlencoded` body pairs, in order.
    pub fn form_pairs(&self) -> Vec<(String, String)> {
        self.body_text()
            .split('&')
            .filter(|s| !s.is_empty())
            .map(|pair| {
                let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
                (decode(k), decode(v))
            })
            .collect()
    }
}

fn decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("zz");
                match u8::from_str_radix(hex, 16) {
                    Ok(b) => {
                        out.push(b);
                        i += 2;
                    }
                    Err(_) => out.push(b'%'),
                }
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A lazily generated body: called for each next chunk until it returns `None`.
pub type Generator = Box<dyn FnMut() -> Option<Bytes> + Send>;

/// How a reply's body is delivered.
pub enum Body {
    /// Complete, with `content-length`.
    Full(Bytes),
    /// `transfer-encoding: chunked`, each chunk after its delay.
    Chunks(Vec<(Duration, Bytes)>),
    /// `transfer-encoding: chunked`, chunks produced on demand (never materialized). The
    /// optional delay is awaited before each chunk.
    Generate(Generator, Option<Duration>),
    /// `transfer-encoding: chunked`: these chunks, then the connection is dropped without the
    /// terminating chunk (a transport failure after a prefix).
    FailAfter(Vec<Bytes>),
    /// The head (with `transfer-encoding: chunked`), then nothing until the server shuts down.
    Stall,
}

impl std::fmt::Debug for Body {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Body::Full(b) => write!(f, "Full({} bytes)", b.len()),
            Body::Chunks(c) => write!(f, "Chunks({})", c.len()),
            Body::Generate(..) => f.write_str("Generate"),
            Body::FailAfter(c) => write!(f, "FailAfter({})", c.len()),
            Body::Stall => f.write_str("Stall"),
        }
    }
}

/// A scripted response.
#[derive(Debug)]
pub struct Reply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Body,
    /// Wait this long before sending the head.
    pub delay: Option<Duration>,
    /// Close the connection after reading the request, without answering.
    pub hang_up: bool,
}

impl Reply {
    pub fn new(status: u16) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: Body::Full(Bytes::new()),
            delay: None,
            hang_up: false,
        }
    }

    /// No answer: the connection is closed after the request is read.
    pub fn hang_up() -> Self {
        Self {
            hang_up: true,
            ..Self::new(0)
        }
    }

    pub fn json(status: u16, body: impl AsRef<[u8]>) -> Self {
        Self::new(status)
            .header("content-type", "application/json; charset=utf-8")
            .body(body)
    }

    pub fn text(status: u16, content_type: &str, body: impl AsRef<[u8]>) -> Self {
        Self::new(status).header("content-type", content_type).body(body)
    }

    pub fn header(mut self, name: &str, value: impl Into<String>) -> Self {
        self.headers.push((name.to_owned(), value.into()));
        self
    }

    pub fn body(mut self, body: impl AsRef<[u8]>) -> Self {
        self.body = Body::Full(Bytes::copy_from_slice(body.as_ref()));
        self
    }

    pub fn with_body(mut self, body: Body) -> Self {
        self.body = body;
        self
    }

    pub fn delay(mut self, delay: Duration) -> Self {
        self.delay = Some(delay);
        self
    }
}

type Responder = dyn Fn(&Recorded) -> Reply + Send + Sync;

/// How the server reads request bodies.
#[derive(Clone, Copy, Debug)]
pub struct Receive {
    /// Body bytes kept in [`Recorded::body`]; the rest is counted, not stored.
    pub record_limit: usize,
    /// Read buffer size (smaller makes the receiver slower per poll).
    pub read_size: usize,
    /// A pause after every read of the body (a slow receiver).
    pub read_delay: Option<Duration>,
}

impl Default for Receive {
    fn default() -> Self {
        Self {
            record_limit: 16 * 1024 * 1024,
            read_size: 16 * 1024,
            read_delay: None,
        }
    }
}

#[derive(Default)]
struct Shared {
    requests: Mutex<Vec<Recorded>>,
    connections: AtomicUsize,
    live_connections: AtomicUsize,
    active_bodies: AtomicUsize,
    peak_active_bodies: AtomicUsize,
    body_bytes_sent: AtomicU64,
    request_body_bytes: AtomicU64,
    changed: Notify,
}

/// What [`TestServer::shutdown`] observed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cleanup {
    /// Connections accepted over the server's life.
    pub connections: usize,
    /// Connections still open when shutdown began (aborted by it).
    pub open_at_shutdown: usize,
    /// Connections open after shutdown returned (always 0 unless cleanup is broken).
    pub open_after: usize,
}

/// A running loopback server. See the crate documentation for its lifecycle.
pub struct TestServer {
    addr: SocketAddr,
    shared: Arc<Shared>,
    stop: watch::Sender<bool>,
    accept: Option<JoinHandle<()>>,
}

impl TestServer {
    /// Starts a server answering every request with `respond(request)`.
    pub async fn start(respond: impl Fn(&Recorded) -> Reply + Send + Sync + 'static) -> Self {
        Self::start_with(Receive::default(), respond).await
    }

    /// [`start`](Self::start) with a request-body policy.
    pub async fn start_with(
        receive: Receive,
        respond: impl Fn(&Recorded) -> Reply + Send + Sync + 'static,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback");
        let addr = listener.local_addr().expect("local addr");
        let shared = Arc::new(Shared::default());
        let respond: Arc<Responder> = Arc::new(respond);
        let (stop, stopped) = watch::channel(false);
        let accept = tokio::spawn(accept_loop(
            listener,
            Arc::clone(&shared),
            respond,
            receive,
            stopped,
        ));
        Self {
            addr,
            shared,
            stop,
            accept: Some(accept),
        }
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// `http://127.0.0.1:<port>` (no trailing slash).
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub fn requests(&self) -> Vec<Recorded> {
        self.shared.requests.lock().unwrap().clone()
    }

    /// The only request received; panics unless exactly one arrived.
    pub fn single(&self) -> Recorded {
        let all = self.requests();
        assert_eq!(all.len(), 1, "expected exactly one request, got {all:#?}");
        all.into_iter().next().unwrap()
    }

    /// Connections accepted so far.
    pub fn connections(&self) -> usize {
        self.shared.connections.load(Ordering::SeqCst)
    }

    /// Connections currently open on the server side.
    pub fn live_connections(&self) -> usize {
        self.shared.live_connections.load(Ordering::SeqCst)
    }

    /// Response bodies currently being written (head sent, body not finished).
    pub fn active_bodies(&self) -> usize {
        self.shared.active_bodies.load(Ordering::SeqCst)
    }

    /// The largest number of response bodies written at the same time.
    pub fn peak_active_bodies(&self) -> usize {
        self.shared.peak_active_bodies.load(Ordering::SeqCst)
    }

    /// Response body bytes handed to the socket so far.
    pub fn body_bytes_sent(&self) -> u64 {
        self.shared.body_bytes_sent.load(Ordering::SeqCst)
    }

    /// Request body bytes received so far (also past the recording limit).
    pub fn request_body_bytes(&self) -> u64 {
        self.shared.request_body_bytes.load(Ordering::SeqCst)
    }

    /// Waits until `condition(self)` holds, re-checking whenever the server's counters change
    /// (and at least every 20 ms); `false` if `max` elapses first.
    pub async fn wait_until(&self, max: Duration, condition: impl Fn(&Self) -> bool) -> bool {
        let deadline = tokio::time::Instant::now() + max;
        loop {
            if condition(self) {
                return true;
            }
            let now = tokio::time::Instant::now();
            if now >= deadline {
                return false;
            }
            let wait = (deadline - now).min(Duration::from_millis(20));
            let _ = tokio::time::timeout(wait, self.shared.changed.notified()).await;
        }
    }

    /// Stops accepting, aborts every connection task and waits for all of them; every socket
    /// the server held is closed when this returns.
    pub async fn shutdown(mut self) -> Cleanup {
        let open_at_shutdown = self.live_connections();
        let _ = self.stop.send(true);
        if let Some(accept) = self.accept.take() {
            let _ = accept.await;
        }
        Cleanup {
            connections: self.connections(),
            open_at_shutdown,
            open_after: self.live_connections(),
        }
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
        if let Some(accept) = self.accept.take() {
            accept.abort();
        }
    }
}

async fn accept_loop(
    listener: TcpListener,
    shared: Arc<Shared>,
    respond: Arc<Responder>,
    receive: Receive,
    mut stopped: watch::Receiver<bool>,
) {
    let mut connections = JoinSet::new();
    loop {
        tokio::select! {
            biased;
            _ = stopped.changed() => break,
            accepted = listener.accept() => {
                let Ok((stream, _)) = accepted else { break };
                shared.connections.fetch_add(1, Ordering::SeqCst);
                shared.live_connections.fetch_add(1, Ordering::SeqCst);
                shared.changed.notify_waiters();
                let guard = LiveGuard(Arc::clone(&shared));
                let respond = Arc::clone(&respond);
                connections.spawn(async move {
                    let _guard = guard;
                    let _ = serve(stream, &_guard.0, &*respond, receive).await;
                });
            }
            // Reap finished connections so the set does not grow with every request.
            Some(_) = connections.join_next(), if !connections.is_empty() => {}
        }
    }
    drop(listener);
    connections.shutdown().await;
}

/// Decrements the live-connection counter when a connection task ends or is aborted.
struct LiveGuard(Arc<Shared>);

impl Drop for LiveGuard {
    fn drop(&mut self) {
        self.0.live_connections.fetch_sub(1, Ordering::SeqCst);
        self.0.changed.notify_waiters();
    }
}

/// Counts a response body as active while it is being written.
struct BodyGuard<'a>(&'a Shared);

impl<'a> BodyGuard<'a> {
    fn new(shared: &'a Shared) -> Self {
        let now = shared.active_bodies.fetch_add(1, Ordering::SeqCst) + 1;
        shared.peak_active_bodies.fetch_max(now, Ordering::SeqCst);
        shared.changed.notify_waiters();
        Self(shared)
    }
}

impl Drop for BodyGuard<'_> {
    fn drop(&mut self) {
        self.0.active_bodies.fetch_sub(1, Ordering::SeqCst);
        self.0.changed.notify_waiters();
    }
}

async fn serve(
    mut stream: TcpStream,
    shared: &Shared,
    respond: &Responder,
    receive: Receive,
) -> std::io::Result<()> {
    let mut buf: Vec<u8> = Vec::new();
    loop {
        let head_end = loop {
            if let Some(pos) = find(&buf, b"\r\n\r\n") {
                break pos;
            }
            let mut chunk = [0u8; 4096];
            let n = stream.read(&mut chunk).await?;
            if n == 0 {
                return Ok(());
            }
            buf.extend_from_slice(&chunk[..n]);
        };
        let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
        buf.drain(..head_end + 4);
        let mut lines = head.split("\r\n");
        let mut parts = lines.next().unwrap_or_default().splitn(3, ' ');
        let method = parts.next().unwrap_or_default().to_owned();
        let target = parts.next().unwrap_or_default().to_owned();
        let headers: Vec<(String, String)> = lines
            .filter_map(|line| line.split_once(':'))
            .map(|(n, v)| (n.trim().to_ascii_lowercase(), v.trim().to_owned()))
            .collect();
        let get = |name: &str| {
            headers
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, v)| v.clone())
        };
        let chunked = get("transfer-encoding").is_some_and(|v| v.eq_ignore_ascii_case("chunked"));
        let host = get("host").unwrap_or_default();
        let mut body = Vec::new();
        let mut body_len = 0u64;
        if chunked {
            read_chunked(&mut stream, &mut buf, shared, receive, &mut body, &mut body_len).await?;
        } else if let Some(len) = get("content-length").and_then(|v| v.parse::<u64>().ok()) {
            read_exact_body(&mut stream, &mut buf, shared, receive, len, &mut body, &mut body_len)
                .await?;
        }
        let recorded = Recorded {
            method,
            target,
            headers,
            body,
            body_len,
            chunked,
            host,
        };
        let reply = respond(&recorded);
        shared.requests.lock().unwrap().push(recorded);
        shared.changed.notify_waiters();
        if reply.hang_up {
            return Ok(());
        }
        if let Some(delay) = reply.delay {
            tokio::time::sleep(delay).await;
        }
        let mut out = format!("HTTP/1.1 {} X\r\n", reply.status);
        for (n, v) in &reply.headers {
            out.push_str(&format!("{n}: {v}\r\n"));
        }
        match reply.body {
            Body::Full(bytes) => {
                out.push_str(&format!("content-length: {}\r\n\r\n", bytes.len()));
                stream.write_all(out.as_bytes()).await?;
                let _active = BodyGuard::new(shared);
                stream.write_all(&bytes).await?;
                shared
                    .body_bytes_sent
                    .fetch_add(bytes.len() as u64, Ordering::SeqCst);
            }
            body => {
                out.push_str("transfer-encoding: chunked\r\n\r\n");
                stream.write_all(out.as_bytes()).await?;
                let _active = BodyGuard::new(shared);
                let complete = write_chunked(&mut stream, shared, body).await?;
                if !complete {
                    return Ok(()); // drop the connection mid-body
                }
            }
        }
        stream.flush().await?;
    }
}

async fn write_frame(stream: &mut TcpStream, shared: &Shared, chunk: &[u8]) -> std::io::Result<()> {
    if chunk.is_empty() {
        return Ok(());
    }
    let mut frame = format!("{:x}\r\n", chunk.len()).into_bytes();
    frame.extend_from_slice(chunk);
    frame.extend_from_slice(b"\r\n");
    stream.write_all(&frame).await?;
    shared
        .body_bytes_sent
        .fetch_add(chunk.len() as u64, Ordering::SeqCst);
    shared.changed.notify_waiters();
    Ok(())
}

/// Writes a chunked body; `Ok(false)` when it deliberately ends without the final chunk.
async fn write_chunked(stream: &mut TcpStream, shared: &Shared, body: Body) -> std::io::Result<bool> {
    match body {
        Body::Full(_) => unreachable!(),
        Body::Chunks(chunks) => {
            for (delay, chunk) in chunks {
                if !delay.is_zero() {
                    tokio::time::sleep(delay).await;
                }
                write_frame(stream, shared, &chunk).await?;
            }
        }
        Body::Generate(mut next, delay) => loop {
            if let Some(delay) = delay {
                tokio::time::sleep(delay).await;
            }
            match next() {
                Some(chunk) => write_frame(stream, shared, &chunk).await?,
                None => break,
            }
        },
        Body::FailAfter(chunks) => {
            for chunk in chunks {
                write_frame(stream, shared, &chunk).await?;
            }
            stream.flush().await?;
            return Ok(false);
        }
        Body::Stall => {
            stream.flush().await?;
            std::future::pending::<()>().await;
        }
    }
    stream.write_all(b"0\r\n\r\n").await?;
    Ok(true)
}

async fn fill(stream: &mut TcpStream, buf: &mut Vec<u8>, receive: Receive) -> std::io::Result<bool> {
    let mut chunk = vec![0u8; receive.read_size.max(1)];
    let n = stream.read(&mut chunk).await?;
    if n == 0 {
        return Ok(false);
    }
    buf.extend_from_slice(&chunk[..n]);
    if let Some(delay) = receive.read_delay {
        tokio::time::sleep(delay).await;
    }
    Ok(true)
}

fn keep(body: &mut Vec<u8>, data: &[u8], receive: Receive) {
    let room = receive.record_limit.saturating_sub(body.len());
    body.extend_from_slice(&data[..data.len().min(room)]);
}

async fn read_exact_body(
    stream: &mut TcpStream,
    buf: &mut Vec<u8>,
    shared: &Shared,
    receive: Receive,
    len: u64,
    body: &mut Vec<u8>,
    body_len: &mut u64,
) -> std::io::Result<()> {
    while *body_len < len {
        if buf.is_empty() && !fill(stream, buf, receive).await? {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        let take = buf.len().min((len - *body_len) as usize);
        keep(body, &buf[..take], receive);
        buf.drain(..take);
        *body_len += take as u64;
        shared
            .request_body_bytes
            .fetch_add(take as u64, Ordering::SeqCst);
        shared.changed.notify_waiters();
    }
    Ok(())
}

async fn read_chunked(
    stream: &mut TcpStream,
    buf: &mut Vec<u8>,
    shared: &Shared,
    receive: Receive,
    body: &mut Vec<u8>,
    body_len: &mut u64,
) -> std::io::Result<()> {
    loop {
        let line_end = loop {
            if let Some(pos) = find(buf, b"\r\n") {
                break pos;
            }
            if !fill(stream, buf, receive).await? {
                return Err(std::io::ErrorKind::UnexpectedEof.into());
            }
        };
        let size_text = String::from_utf8_lossy(&buf[..line_end]).into_owned();
        let size = usize::from_str_radix(size_text.split(';').next().unwrap_or("").trim(), 16)
            .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidData))?;
        buf.drain(..line_end + 2);
        if size == 0 {
            // Trailer section: skip to the empty line.
            loop {
                if let Some(pos) = find(buf, b"\r\n") {
                    let empty = pos == 0;
                    buf.drain(..pos + 2);
                    if empty {
                        return Ok(());
                    }
                } else if !fill(stream, buf, receive).await? {
                    return Err(std::io::ErrorKind::UnexpectedEof.into());
                }
            }
        }
        let mut remaining = size;
        while remaining > 0 {
            if buf.is_empty() && !fill(stream, buf, receive).await? {
                return Err(std::io::ErrorKind::UnexpectedEof.into());
            }
            let take = buf.len().min(remaining);
            keep(body, &buf[..take], receive);
            buf.drain(..take);
            remaining -= take;
            *body_len += take as u64;
            shared
                .request_body_bytes
                .fetch_add(take as u64, Ordering::SeqCst);
            shared.changed.notify_waiters();
        }
        while buf.len() < 2 {
            if !fill(stream, buf, receive).await? {
                return Err(std::io::ErrorKind::UnexpectedEof.into());
            }
        }
        buf.drain(..2);
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// A shareable script of replies, consumed in order (for stateful flows: job submitted, then
/// running, then complete). Panics when exhausted.
#[derive(Clone, Default)]
pub struct Script(Arc<Mutex<VecDeque<Reply>>>);

impl Script {
    pub fn new(replies: impl IntoIterator<Item = Reply>) -> Self {
        Self(Arc::new(Mutex::new(replies.into_iter().collect())))
    }

    pub fn push(&self, reply: Reply) {
        self.0.lock().unwrap().push_back(reply);
    }

    pub fn next(&self) -> Reply {
        self.0
            .lock()
            .unwrap()
            .pop_front()
            .expect("the reply script is exhausted")
    }

    pub fn remaining(&self) -> usize {
        self.0.lock().unwrap().len()
    }
}

/// A lazily generated body of `count` records produced by `record(i)`, grouped into chunks of
/// about `chunk_bytes` (generated on demand, never materialized), with an optional failure
/// (connection dropped) after `fail_after` records.
pub fn generated_records(
    count: u64,
    chunk_bytes: usize,
    mut record: impl FnMut(u64) -> Vec<u8> + Send + 'static,
) -> Generator {
    let mut next = 0u64;
    Box::new(move || {
        if next >= count {
            return None;
        }
        let mut chunk = Vec::with_capacity(chunk_bytes + 256);
        while next < count && chunk.len() < chunk_bytes {
            chunk.extend_from_slice(&record(next));
            next += 1;
        }
        Some(Bytes::from(chunk))
    })
}

/// A boxed future, for responders that compute replies asynchronously in tests.
pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

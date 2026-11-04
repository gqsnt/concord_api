//! A tiny HTTP/1.1 loopback server for wire-level assertions.
//!
//! It records each request exactly as received (request target bytes, header lines in order,
//! body bytes) and answers with a scripted response. It deliberately does not use an HTTP
//! server library, so the assertions observe what reqwest put on the wire.
//!
//! Copied from `crates/concord/tests/support/mod.rs`, then extended for these recipes: a reply
//! can be delayed or replaced by dropping the connection, the server tracks its peak number of
//! requests in flight, and [`chunked`] serves a real `Transfer-Encoding: chunked` body. A request
//! body sent with `Transfer-Encoding: chunked` is decoded (and flagged).
#![allow(dead_code, missing_docs)]

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// One request as observed on the wire.
#[derive(Debug, Clone)]
pub struct Recorded {
    pub method: String,
    /// The raw request target, e.g. `/v1/items/a%2Fb?tag=x&tag=y`.
    pub target: String,
    /// Header lines in wire order, names lowercased.
    pub headers: Vec<(String, String)>,
    /// The body bytes (a chunked body decoded).
    pub body: Vec<u8>,
    /// Whether the body arrived with `transfer-encoding: chunked`.
    pub chunked: bool,
    /// The sizes of the non-empty chunks of a chunked body, in wire order (empty otherwise).
    pub chunk_sizes: Vec<usize>,
}

impl Recorded {
    pub fn path(&self) -> &str {
        self.target.split('?').next().unwrap_or("")
    }

    pub fn query(&self) -> Option<&str> {
        self.target.split_once('?').map(|(_, q)| q)
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

    pub fn header(&self, name: &str) -> Option<&str> {
        let all = self.header_all(name);
        assert!(all.len() <= 1, "header {name} occurs {} times", all.len());
        all.first().copied()
    }
}

/// A scripted response.
#[derive(Debug, Clone)]
pub struct Reply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// Wait this long before answering.
    pub delay: Option<Duration>,
    /// Close the connection after reading the request, without answering.
    pub drop_connection: bool,
}

impl Reply {
    pub fn new(status: u16) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: Vec::new(),
            delay: None,
            drop_connection: false,
        }
    }

    /// No answer: the connection is closed after the request is read.
    pub fn hang_up() -> Self {
        Self {
            drop_connection: true,
            ..Self::new(0)
        }
    }

    pub fn delay(mut self, delay: Duration) -> Self {
        self.delay = Some(delay);
        self
    }

    pub fn json(status: u16, body: &str) -> Self {
        Self::new(status)
            .header("content-type", "application/json")
            .body(body)
    }

    pub fn text(status: u16, body: &str) -> Self {
        Self::new(status)
            .header("content-type", "text/plain; charset=utf-8")
            .body(body)
    }

    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_owned(), value.to_owned()));
        self
    }

    pub fn body(mut self, body: impl AsRef<[u8]>) -> Self {
        self.body = body.as_ref().to_vec();
        self
    }
}

type Responder = dyn Fn(&Recorded) -> Reply + Send + Sync;

/// A running loopback server. Dropping it stops accepting new connections.
pub struct Server {
    pub addr: std::net::SocketAddr,
    requests: Arc<Mutex<Vec<Recorded>>>,
    load: Arc<Load>,
    task: tokio::task::JoinHandle<()>,
}

#[derive(Default)]
struct Load {
    in_flight: AtomicUsize,
    peak: AtomicUsize,
    connections: AtomicUsize,
}

impl Server {
    /// Starts a server answering every request with `respond(request)`.
    pub async fn start(respond: impl Fn(&Recorded) -> Reply + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback");
        let addr = listener.local_addr().expect("local addr");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let load = Arc::new(Load::default());
        let respond: Arc<Responder> = Arc::new(respond);
        let log = Arc::clone(&requests);
        let counters = Arc::clone(&load);
        let task = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    return;
                };
                counters.connections.fetch_add(1, Ordering::SeqCst);
                let log = Arc::clone(&log);
                let respond = Arc::clone(&respond);
                let counters = Arc::clone(&counters);
                tokio::spawn(async move {
                    let _ = serve_connection(stream, log, respond, counters).await;
                });
            }
        });
        Self {
            addr,
            requests,
            load,
            task,
        }
    }

    /// Starts a server answering every request with the same reply.
    pub async fn reply(reply: Reply) -> Self {
        Self::start(move |_| reply.clone()).await
    }

    /// `http://127.0.0.1:<port>` (no trailing slash).
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// The largest number of requests that were being answered at the same time.
    pub fn peak_in_flight(&self) -> usize {
        self.load.peak.load(Ordering::SeqCst)
    }

    /// TCP connections accepted so far.
    pub fn connections(&self) -> usize {
        self.load.connections.load(Ordering::SeqCst)
    }

    pub fn requests(&self) -> Vec<Recorded> {
        self.requests.lock().unwrap().clone()
    }

    /// The only request received; panics unless exactly one arrived.
    pub fn single(&self) -> Recorded {
        let all = self.requests();
        assert_eq!(all.len(), 1, "expected exactly one request, got {all:#?}");
        all.into_iter().next().unwrap()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn serve_connection(
    mut stream: tokio::net::TcpStream,
    log: Arc<Mutex<Vec<Recorded>>>,
    respond: Arc<Responder>,
    load: Arc<Load>,
) -> std::io::Result<()> {
    let mut buf = Vec::new();
    loop {
        // Read one request head.
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
        let request_line = lines.next().unwrap_or_default();
        let mut parts = request_line.splitn(3, ' ');
        let method = parts.next().unwrap_or_default().to_owned();
        let target = parts.next().unwrap_or_default().to_owned();
        let headers: Vec<(String, String)> = lines
            .filter_map(|line| line.split_once(':'))
            .map(|(n, v)| (n.trim().to_ascii_lowercase(), v.trim().to_owned()))
            .collect();
        let chunked = headers
            .iter()
            .any(|(n, v)| n == "transfer-encoding" && v.eq_ignore_ascii_case("chunked"));
        let mut chunk_sizes = Vec::new();
        let body: Vec<u8> = if chunked {
            // `<hex size>[;ext]\r\n<data>\r\n` ... `0\r\n\r\n` (no trailers).
            let mut body = Vec::new();
            loop {
                let line_end = loop {
                    if let Some(pos) = find(&buf, b"\r\n") {
                        break pos;
                    }
                    if !read_more(&mut stream, &mut buf).await? {
                        return Ok(());
                    }
                };
                let size_line = String::from_utf8_lossy(&buf[..line_end]).into_owned();
                buf.drain(..line_end + 2);
                let size =
                    usize::from_str_radix(size_line.split(';').next().unwrap_or("").trim(), 16)
                        .expect("chunk size");
                while buf.len() < size + 2 {
                    if !read_more(&mut stream, &mut buf).await? {
                        return Ok(());
                    }
                }
                body.extend(buf.drain(..size));
                assert_eq!(buf.drain(..2).as_slice(), b"\r\n", "chunk terminator");
                if size == 0 {
                    break body;
                }
                chunk_sizes.push(size);
            }
        } else {
            let len: usize = headers
                .iter()
                .find(|(n, _)| n == "content-length")
                .and_then(|(_, v)| v.parse().ok())
                .unwrap_or(0);
            while buf.len() < len {
                if !read_more(&mut stream, &mut buf).await? {
                    return Ok(());
                }
            }
            buf.drain(..len).collect()
        };
        let recorded = Recorded {
            method,
            target,
            headers,
            body,
            chunked,
            chunk_sizes,
        };
        let reply = respond(&recorded);
        log.lock().unwrap().push(recorded);
        if reply.drop_connection {
            return Ok(());
        }
        let now = load.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        load.peak.fetch_max(now, Ordering::SeqCst);
        if let Some(delay) = reply.delay {
            tokio::time::sleep(delay).await;
        }
        load.in_flight.fetch_sub(1, Ordering::SeqCst);

        let mut out = format!("HTTP/1.1 {} X\r\n", reply.status);
        for (n, v) in &reply.headers {
            out.push_str(&format!("{n}: {v}\r\n"));
        }
        out.push_str(&format!("content-length: {}\r\n\r\n", reply.body.len()));
        stream.write_all(out.as_bytes()).await?;
        stream.write_all(&reply.body).await?;
        stream.flush().await?;
    }
}

async fn read_more(stream: &mut tokio::net::TcpStream, buf: &mut Vec<u8>) -> std::io::Result<bool> {
    let mut chunk = [0u8; 4096];
    let n = stream.read(&mut chunk).await?;
    buf.extend_from_slice(&chunk[..n]);
    Ok(n > 0)
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// What the chunked server observed.
#[derive(Default)]
pub struct Observed {
    pub chunks_written: AtomicUsize,
    pub client_went_away: AtomicBool,
    pub finished: AtomicBool,
}

impl Observed {
    /// Waits (bounded) until the server noticed the client closing the connection.
    pub async fn wait_for_close(&self, max: Duration) -> bool {
        let deadline = tokio::time::Instant::now() + max;
        while tokio::time::Instant::now() < deadline {
            if self.client_went_away.load(Ordering::SeqCst) {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        self.client_went_away.load(Ordering::SeqCst)
    }
}

/// Serves ONE request with `200` and a real `Transfer-Encoding: chunked` body of `total` 1 KiB
/// chunks, `gap` apart. With `stall_after = Some(n)`, it sends `n` chunks and then holds the
/// connection open silently. A failed write means the client went away.
pub async fn chunked(
    total: usize,
    gap: Duration,
    stall_after: Option<usize>,
) -> (std::net::SocketAddr, Arc<Observed>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let observed = Arc::new(Observed::default());
    let obs = Arc::clone(&observed);
    tokio::spawn(async move {
        let (mut s, _) = listener.accept().await.unwrap();
        let mut head = Vec::new();
        let mut buf = [0u8; 1024];
        while find(&head, b"\r\n\r\n").is_none() {
            let n = s.read(&mut buf).await.unwrap();
            if n == 0 {
                return;
            }
            head.extend_from_slice(&buf[..n]);
        }
        s.write_all(
            b"HTTP/1.1 200 OK\r\ncontent-type: application/octet-stream\r\ntransfer-encoding: chunked\r\n\r\n",
        )
        .await
        .unwrap();
        let chunk = vec![b'z'; 1024];
        for i in 0..total {
            if stall_after == Some(i) {
                tokio::time::sleep(Duration::from_secs(30)).await;
                return;
            }
            let mut frame = format!("{:x}\r\n", chunk.len()).into_bytes();
            frame.extend_from_slice(&chunk);
            frame.extend_from_slice(b"\r\n");
            if s.write_all(&frame).await.is_err() || s.flush().await.is_err() {
                obs.client_went_away.store(true, Ordering::SeqCst);
                return;
            }
            obs.chunks_written.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(gap).await;
        }
        let _ = s.write_all(b"0\r\n\r\n").await;
        obs.finished.store(true, Ordering::SeqCst);
    });
    (addr, observed)
}

/// The example API pointed at a loopback address, with authorization `key`.
pub fn inventory_at(
    addr: std::net::SocketAddr,
    key: &str,
) -> concord_example_native::api::Inventory {
    use concord_example_native::api::{Inventory, InventoryConfig};
    Inventory::new(
        concord::reqwest::Client::new(),
        InventoryConfig {
            key: key.to_owned(),
        },
    )
    .with_base_url(format!("http://{addr}/v1"))
    .unwrap()
}

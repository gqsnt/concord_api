//! A tiny HTTP/1.1 loopback server for wire-level assertions.
//!
//! It records each request exactly as received (request target bytes, header lines in order,
//! body bytes) and answers with a scripted response. It deliberately does not use an HTTP
//! server library, so the assertions observe what reqwest put on the wire.
#![allow(dead_code, missing_docs)]

use std::sync::atomic::{AtomicUsize, Ordering};
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
    /// Header lines in wire order, names lowercased (values decoded lossily as UTF-8).
    pub headers: Vec<(String, String)>,
    /// The same header lines with the value bytes exactly as received.
    pub raw_headers: Vec<(String, Vec<u8>)>,
    pub body: Vec<u8>,
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

    /// All values of a header as raw bytes, in wire order.
    pub fn header_bytes_all(&self, name: &str) -> Vec<&[u8]> {
        let name = name.to_ascii_lowercase();
        self.raw_headers
            .iter()
            .filter(|(n, _)| *n == name)
            .map(|(_, v)| v.as_slice())
            .collect()
    }
}

/// A scripted response.
#[derive(Debug, Clone)]
pub struct Reply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Reply {
    pub fn new(status: u16) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: Vec::new(),
        }
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
    task: tokio::task::JoinHandle<()>,
}

impl Server {
    /// Starts a server answering every request with `respond(request)`.
    pub async fn start(respond: impl Fn(&Recorded) -> Reply + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback");
        let addr = listener.local_addr().expect("local addr");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let respond: Arc<Responder> = Arc::new(respond);
        let log = Arc::clone(&requests);
        let task = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    return;
                };
                let log = Arc::clone(&log);
                let respond = Arc::clone(&respond);
                tokio::spawn(async move {
                    let _ = serve_connection(stream, log, respond).await;
                });
            }
        });
        Self {
            addr,
            requests,
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
        let raw_headers: Vec<(String, Vec<u8>)> = buf[..head_end]
            .split(|&b| b == b'\n')
            .skip(1)
            .filter_map(|line| {
                let line = line.strip_suffix(b"\r").unwrap_or(line);
                let colon = line.iter().position(|&b| b == b':')?;
                let name = String::from_utf8_lossy(&line[..colon])
                    .trim()
                    .to_ascii_lowercase();
                let value = line[colon + 1..].trim_ascii().to_vec();
                Some((name, value))
            })
            .collect();
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
        let len: usize = headers
            .iter()
            .find(|(n, _)| n == "content-length")
            .and_then(|(_, v)| v.parse().ok())
            .unwrap_or(0);
        while buf.len() < len {
            let mut chunk = [0u8; 4096];
            let n = stream.read(&mut chunk).await?;
            if n == 0 {
                return Ok(());
            }
            buf.extend_from_slice(&chunk[..n]);
        }
        let body: Vec<u8> = buf.drain(..len).collect();
        let recorded = Recorded {
            method,
            target,
            headers,
            raw_headers,
            body,
        };
        let reply = respond(&recorded);
        log.lock().unwrap().push(recorded);

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

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// A server that sends a response head announcing a body of `body.len()` bytes, then withholds the
/// body until `release` fires. Returns its URL and a counter of body bytes written.
pub async fn withholding(
    head: &'static str,
    body: &'static [u8],
) -> (String, tokio::sync::oneshot::Sender<()>, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (release, released) = tokio::sync::oneshot::channel::<()>();
    let written = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&written);
    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        let mut buf = [0u8; 1024];
        while !request.windows(4).any(|w| w == b"\r\n\r\n") {
            let n = stream.read(&mut buf).await.unwrap();
            request.extend_from_slice(&buf[..n]);
        }
        let head = format!("{head}\r\ncontent-length: {}\r\n\r\n", body.len());
        stream.write_all(head.as_bytes()).await.unwrap();
        stream.flush().await.unwrap();
        let _ = released.await;
        stream.write_all(body).await.unwrap();
        counter.fetch_add(body.len(), Ordering::SeqCst);
        stream.flush().await.unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
    });
    (url, release, written)
}

/// A loopback address whose port refuses every TCP connection for the rest of the process: a
/// socket is bound to it and never listens, so a connection is refused and no other socket can
/// take the port meanwhile. (A port freed by dropping a listener can be given to a concurrent
/// test's server before the request connects.) The same helper as
/// `concord_example_testserver::refused_addr`, whose tests check these guarantees per platform.
pub fn refused_addr() -> std::net::SocketAddr {
    static REFUSED: std::sync::OnceLock<(tokio::net::TcpSocket, std::net::SocketAddr)> =
        std::sync::OnceLock::new();
    REFUSED
        .get_or_init(|| {
            let socket = tokio::net::TcpSocket::new_v4().expect("create a TCP socket");
            socket
                .bind(std::net::SocketAddr::from(([127, 0, 0, 1], 0)))
                .expect("bind a loopback port");
            let addr = socket.local_addr().expect("local addr");
            (socket, addr)
        })
        .1
}

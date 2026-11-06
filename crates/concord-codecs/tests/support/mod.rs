//! Shared helpers of the JSON Lines and JSON values tests: chunk partitions, counting byte
//! sources, a manual poller, and a one-connection loopback HTTP/1.1 peer owned by each test.
//!
//! The loopback peer accepts exactly one connection and runs the test's script on it in one
//! task; the test joins it (with a timeout) or the task is aborted when the handle drops. No
//! detached task outlives the test.
#![allow(dead_code, missing_docs)]

use std::collections::VecDeque;
use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use concord_codecs::stream::{ByteSource, SourceError};
use concord_core::bytes::Bytes;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

/// Every way to cut `input` once (including the cuts at both ends, which make an empty chunk),
/// one chunk per byte, and deterministic pseudo-random partitions with empty chunks mixed in.
pub fn partitions(input: &[u8]) -> Vec<Vec<Vec<u8>>> {
    let mut out = Vec::new();
    for i in 0..=input.len() {
        out.push(vec![input[..i].to_vec(), input[i..].to_vec()]);
    }
    out.push(input.iter().map(|b| vec![*b]).collect());
    let mut seed = 0x9E37_79B9_u32 ^ input.len() as u32;
    for _ in 0..24 {
        let mut parts = Vec::new();
        let mut at = 0;
        while at < input.len() {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let size = (seed >> 24) as usize % 6; // 0..=5: empty chunks too
            let end = (at + size).min(input.len());
            parts.push(input[at..end].to_vec());
            at = end;
        }
        out.push(parts);
    }
    out
}

/// Every way to cut `input` twice.
pub fn double_splits(input: &[u8]) -> Vec<Vec<Vec<u8>>> {
    let mut out = Vec::new();
    for i in 0..=input.len() {
        for j in i..=input.len() {
            out.push(vec![
                input[..i].to_vec(),
                input[i..j].to_vec(),
                input[j..].to_vec(),
            ]);
        }
    }
    out
}

/// A [`ByteSource`] over chunks that counts how often it was polled.
pub struct Counting {
    chunks: VecDeque<Bytes>,
    polls: Arc<AtomicUsize>,
}

impl Counting {
    pub fn new<I: IntoIterator<Item = Vec<u8>>>(chunks: I) -> (Self, Arc<AtomicUsize>) {
        let polls = Arc::new(AtomicUsize::new(0));
        (
            Self {
                chunks: chunks.into_iter().map(Bytes::from).collect(),
                polls: Arc::clone(&polls),
            },
            polls,
        )
    }
}

impl ByteSource for Counting {
    async fn next_chunk(&mut self) -> Result<Option<Bytes>, SourceError> {
        self.polls.fetch_add(1, Ordering::SeqCst);
        Ok(self.chunks.pop_front())
    }
}

/// A [`ByteSource`] that generates its input on demand, one chunk of `chunk` bytes per poll, from
/// `records` lines made by `line(i)`: nothing is materialized beyond the current chunk.
pub struct Generated<F> {
    line: F,
    next: u64,
    records: u64,
    pending: Vec<u8>,
    chunk: usize,
    polls: Arc<AtomicUsize>,
    produced: Arc<AtomicUsize>,
}

impl<F: FnMut(u64) -> Vec<u8> + Send> Generated<F> {
    pub fn new(records: u64, chunk: usize, line: F) -> (Self, Arc<AtomicUsize>, Arc<AtomicUsize>) {
        let polls = Arc::new(AtomicUsize::new(0));
        let produced = Arc::new(AtomicUsize::new(0));
        (
            Self {
                line,
                next: 0,
                records,
                pending: Vec::new(),
                chunk,
                polls: Arc::clone(&polls),
                produced: Arc::clone(&produced),
            },
            polls,
            produced,
        )
    }
}

impl<F: FnMut(u64) -> Vec<u8> + Send> ByteSource for Generated<F> {
    async fn next_chunk(&mut self) -> Result<Option<Bytes>, SourceError> {
        self.polls.fetch_add(1, Ordering::SeqCst);
        while self.pending.len() < self.chunk && self.next < self.records {
            let line = (self.line)(self.next);
            self.pending.extend_from_slice(&line);
            self.next += 1;
        }
        if self.pending.is_empty() {
            return Ok(None);
        }
        let take = self.pending.len().min(self.chunk);
        let chunk: Vec<u8> = self.pending.drain(..take).collect();
        self.produced.fetch_add(chunk.len(), Ordering::SeqCst);
        Ok(Some(Bytes::from(chunk)))
    }
}

/// Polls `future` to completion on the current thread with a no-op waker, counting the
/// `Pending` results (with in-memory sources the only `Pending` is the readers' cooperative
/// yield).
pub fn poll_counting<F: Future>(future: F) -> (F::Output, usize) {
    let mut future = pin!(future);
    let mut cx = Context::from_waker(Waker::noop());
    let mut pendings = 0;
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(output) => return (output, pendings),
            Poll::Pending => {
                pendings += 1;
                assert!(pendings < 1_000_000, "the future never completes");
            }
        }
    }
}

/// A loopback peer that accepts one connection and runs a script on it.
pub struct Peer<T> {
    pub addr: std::net::SocketAddr,
    task: Option<JoinHandle<T>>,
}

impl<T: Send + 'static> Peer<T> {
    pub async fn start<F, Fut>(script: F) -> Self
    where
        F: FnOnce(TcpStream) -> Fut + Send + 'static,
        Fut: Future<Output = T> + Send + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback");
        let addr = listener.local_addr().expect("local addr");
        let task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            drop(listener);
            script(stream).await
        });
        Self {
            addr,
            task: Some(task),
        }
    }

    pub fn url(&self) -> String {
        format!("http://{}/", self.addr)
    }

    /// Waits for the script to finish (at most 10 s).
    pub async fn join(mut self) -> T {
        let task = self.task.take().expect("joined once");
        tokio::time::timeout(Duration::from_secs(10), task)
            .await
            .expect("the loopback script finished in time")
            .expect("the loopback script did not panic")
    }
}

impl<T> Drop for Peer<T> {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

/// Reads a request head; returns it and the bytes read past it.
pub async fn read_head(stream: &mut TcpStream) -> (String, Vec<u8>) {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    loop {
        if let Some(end) = find(&buf, b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&buf[..end]).into_owned();
            return (head, buf[end + 4..].to_vec());
        }
        let n = stream.read(&mut tmp).await.expect("read head");
        assert!(n > 0, "the client closed before sending a head");
        buf.extend_from_slice(&tmp[..n]);
    }
}

pub fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Reads a `Transfer-Encoding: chunked` body, calling `on_data` with everything decoded so far
/// after each chunk. Returns the decoded bytes and whether the terminating chunk arrived (`false`:
/// the connection ended first).
pub async fn read_chunked(
    stream: &mut TcpStream,
    mut buf: Vec<u8>,
    mut on_data: impl FnMut(&[u8]),
) -> (Vec<u8>, bool) {
    let mut body = Vec::new();
    let mut tmp = [0u8; 8192];
    loop {
        while let Some(line_end) = find(&buf, b"\r\n") {
            let size_text = String::from_utf8_lossy(&buf[..line_end]).into_owned();
            let size = usize::from_str_radix(size_text.split(';').next().unwrap_or("").trim(), 16)
                .expect("chunk size");
            if size == 0 {
                return (body, true);
            }
            if buf.len() < line_end + 2 + size + 2 {
                break;
            }
            body.extend_from_slice(&buf[line_end + 2..line_end + 2 + size]);
            buf.drain(..line_end + 2 + size + 2);
            on_data(&body);
        }
        match stream.read(&mut tmp).await {
            Ok(0) | Err(_) => return (body, false),
            Ok(n) => buf.extend_from_slice(&tmp[..n]),
        }
    }
}

/// Writes a response head (`HTTP/1.1 <status>`, the given header lines, a blank line).
pub async fn write_head(stream: &mut TcpStream, status: &str, headers: &[&str]) {
    let mut head = format!("HTTP/1.1 {status}\r\n");
    for h in headers {
        head.push_str(h);
        head.push_str("\r\n");
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes()).await.expect("write head");
}

/// Writes one chunk of a chunked body.
pub async fn write_chunk(stream: &mut TcpStream, data: &[u8]) {
    let mut out = format!("{:x}\r\n", data.len()).into_bytes();
    out.extend_from_slice(data);
    out.extend_from_slice(b"\r\n");
    stream.write_all(&out).await.expect("write chunk");
    stream.flush().await.expect("flush");
}

/// Ends a chunked body.
pub async fn end_chunks(stream: &mut TcpStream) {
    stream.write_all(b"0\r\n\r\n").await.expect("end chunks");
}

/// Waits until the client closes the connection (a read returns 0 or fails); reports how long
/// that took. Anything the client sends meanwhile is discarded.
pub async fn until_closed(stream: &mut TcpStream) -> Duration {
    let started = std::time::Instant::now();
    let mut tmp = [0u8; 1024];
    loop {
        match stream.read(&mut tmp).await {
            Ok(0) | Err(_) => return started.elapsed(),
            Ok(_) => {}
        }
    }
}

/// A plain reqwest client (no pool reuse across tests: each test owns its client).
pub fn client() -> concord_core::reqwest::Client {
    concord_core::reqwest::Client::builder()
        .build()
        .expect("client")
}

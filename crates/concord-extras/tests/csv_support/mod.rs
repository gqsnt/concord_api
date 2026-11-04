//! Helpers shared by the CSV tests: outcomes of a read on the finite and incremental routes,
//! chunk partitions, a counting byte source and an owned loopback HTTP server.

#![allow(dead_code)]

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use concord_core::bytes::Bytes;
use concord_core::{Decode, reqwest};
use concord_extras::csv::{CsvContext, CsvReader, RawTable};
use concord_extras::stream::{ByteSource, ReadErrorKind, SourceError};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

/// What reading a table produced: the header, the records delivered (as text, or lossy text for
/// raw bytes), and the terminal error's kind (incremental route only) and static label.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub headers: Option<Vec<String>>,
    pub records: Vec<Vec<String>>,
    pub error: Option<(Option<ReadErrorKind>, &'static str)>,
}

impl Outcome {
    pub fn ok(headers: Option<&[&str]>, records: &[&[&str]]) -> Self {
        Self {
            headers: headers.map(|h| h.iter().map(|s| (*s).to_owned()).collect()),
            records: records
                .iter()
                .map(|r| r.iter().map(|s| (*s).to_owned()).collect())
                .collect(),
            error: None,
        }
    }

    /// The same outcome with this terminal error.
    pub fn failing(mut self, kind: ReadErrorKind, label: &'static str) -> Self {
        self.error = Some((Some(kind), label));
        self
    }

    /// Without the error kind (to compare with the finite route, which has only labels).
    pub fn labels_only(mut self) -> Self {
        if let Some((_, label)) = self.error {
            self.error = Some((None, label));
        }
        self
    }
}

fn lossy(field: &[u8]) -> String {
    String::from_utf8_lossy(field).into_owned()
}

/// Typed `Vec<String>` rows through the incremental reader, from these chunks.
pub async fn streamed<S: ByteSource>(reader: &CsvReader, source: S) -> Outcome {
    let mut rows = reader.read::<Vec<String>, _>(source);
    let mut records = Vec::new();
    let mut error = None;
    while let Some(next) = rows.next().await {
        match next {
            Ok(row) => records.push(row),
            Err(e) => {
                let label = e.codec().map_or("", |c| c.label());
                error = Some((Some(e.kind()), label));
                assert_eq!(e.position().records, records.len() as u64);
            }
        }
    }
    // Fused after the end or an error.
    assert!(rows.next().await.is_none());
    Outcome {
        headers: rows.headers().map(|h| h.iter().map(lossy).collect()),
        records,
        error,
    }
}

/// Raw records through the incremental reader.
pub async fn streamed_raw<S: ByteSource>(reader: &CsvReader, source: S) -> Outcome {
    let mut rows = reader.read_raw(source);
    let mut records = Vec::new();
    let mut error = None;
    while let Some(next) = rows.next().await {
        match next {
            Ok(row) => records.push(row.iter().map(lossy).collect()),
            Err(e) => error = Some((Some(e.kind()), e.codec().map_or("", |c| c.label()))),
        }
    }
    Outcome {
        headers: rows.headers().map(|h| h.iter().map(lossy).collect()),
        records,
        error,
    }
}

/// Typed `Vec<String>` rows through the finite `Decode` route of the format `F` (no head check).
pub fn finite<F>(body: &[u8]) -> Outcome
where
    F: Decode<Vec<Vec<String>>, Context = CsvContext> + Decode<RawTable, Context = CsvContext>,
{
    let context = <F as Decode<Vec<Vec<String>>>>::context(&empty_head()).unwrap();
    let headers = <F as Decode<RawTable>>::decode_body(
        <F as Decode<RawTable>>::context(&empty_head()).unwrap(),
        Bytes::copy_from_slice(body),
    )
    .ok()
    .and_then(|t| t.headers)
    .map(|h| h.iter().map(lossy).collect());
    match <F as Decode<Vec<Vec<String>>>>::decode_body(context, Bytes::copy_from_slice(body)) {
        Ok(records) => Outcome {
            headers,
            records,
            error: None,
        },
        Err(e) => Outcome {
            headers: None,
            records: Vec::new(),
            error: Some((None, e.label())),
        },
    }
}

fn empty_head() -> concord_core::ResponseHead<'static> {
    static HEADERS: std::sync::OnceLock<reqwest::header::HeaderMap> = std::sync::OnceLock::new();
    concord_core::ResponseHead::new(
        reqwest::StatusCode::OK,
        reqwest::Version::HTTP_11,
        HEADERS.get_or_init(reqwest::header::HeaderMap::new),
    )
}

/// Every partition of `bytes` into two chunks (split points `0..=len`).
pub fn single_splits(bytes: &[u8]) -> Vec<Vec<Vec<u8>>> {
    (0..=bytes.len())
        .map(|i| vec![bytes[..i].to_vec(), bytes[i..].to_vec()])
        .collect()
}

/// Every partition into three chunks (two split points), for small inputs.
pub fn double_splits(bytes: &[u8]) -> Vec<Vec<Vec<u8>>> {
    let mut out = Vec::new();
    for i in 0..=bytes.len() {
        for j in i..=bytes.len() {
            out.push(vec![
                bytes[..i].to_vec(),
                bytes[i..j].to_vec(),
                bytes[j..].to_vec(),
            ]);
        }
    }
    out
}

/// One byte per chunk.
pub fn byte_chunks(bytes: &[u8]) -> Vec<Vec<u8>> {
    bytes.iter().map(|b| vec![*b]).collect()
}

/// Deterministic pseudo-random partitions (chunk sizes 0..=7), `count` of them.
pub fn random_partitions(bytes: &[u8], count: usize, seed: u64) -> Vec<Vec<Vec<u8>>> {
    let mut state = seed | 1;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    (0..count)
        .map(|_| {
            let mut chunks = Vec::new();
            let mut at = 0;
            while at < bytes.len() {
                let len = (next() % 8) as usize;
                let end = (at + len).min(bytes.len());
                chunks.push(bytes[at..end].to_vec());
                at = end;
            }
            chunks
        })
        .collect()
}

/// A byte source over chunks that counts how many times it was asked for one.
pub struct Counting {
    chunks: std::collections::VecDeque<Bytes>,
    polls: Arc<AtomicUsize>,
    fail_at_end: Option<&'static str>,
}

impl Counting {
    pub fn new<I, B>(chunks: I) -> (Self, Arc<AtomicUsize>)
    where
        I: IntoIterator<Item = B>,
        B: Into<Bytes>,
    {
        let polls = Arc::new(AtomicUsize::new(0));
        (
            Self {
                chunks: chunks.into_iter().map(Into::into).collect(),
                polls: Arc::clone(&polls),
                fail_at_end: None,
            },
            polls,
        )
    }

    pub fn then_fail(mut self, label: &'static str) -> Self {
        self.fail_at_end = Some(label);
        self
    }
}

impl ByteSource for Counting {
    async fn next_chunk(&mut self) -> Result<Option<Bytes>, SourceError> {
        self.polls.fetch_add(1, Ordering::SeqCst);
        match self.chunks.pop_front() {
            Some(chunk) => Ok(Some(chunk)),
            None => match self.fail_at_end.take() {
                Some(label) => Err(SourceError::new(label)),
                None => Ok(None),
            },
        }
    }
}

/// A byte source generated on the fly (never collected): `make(i)` is chunk `i`, `None` ends.
pub struct Generated<F> {
    make: F,
    index: usize,
    pub produced: Arc<AtomicUsize>,
}

impl<F> Generated<F> {
    pub fn new(make: F) -> (Self, Arc<AtomicUsize>) {
        let produced = Arc::new(AtomicUsize::new(0));
        (
            Self {
                make,
                index: 0,
                produced: Arc::clone(&produced),
            },
            produced,
        )
    }
}

impl<F> ByteSource for Generated<F>
where
    F: FnMut(usize) -> Option<Bytes> + Send,
{
    async fn next_chunk(&mut self) -> Result<Option<Bytes>, SourceError> {
        let chunk = (self.make)(self.index);
        self.index += 1;
        if let Some(chunk) = &chunk {
            self.produced.fetch_add(chunk.len(), Ordering::SeqCst);
        }
        Ok(chunk)
    }
}

// ---------------------------------------------------------------------------------------------
// An owned loopback HTTP/1.1 server: one task per test, aborted and joined by `stop`.

pub struct Loopback {
    pub url: String,
    task: JoinHandle<()>,
}

impl Loopback {
    /// Serves connections with `handler` (each connection on its own task, owned by the
    /// server task).
    pub async fn start<F, Fut>(handler: F) -> Self
    where
        F: Fn(TcpStream) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let mut connections = tokio::task::JoinSet::new();
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                connections.spawn(handler(stream));
            }
        });
        Self { url, task }
    }

    /// Stops accepting and ends every connection task (dropping the `JoinSet` aborts them).
    pub async fn stop(self) {
        self.task.abort();
        let _ = self.task.await;
    }
}

/// Reads a request head (through the blank line); returns it as text.
pub async fn read_head(stream: &mut TcpStream) -> String {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if stream.read(&mut byte).await.unwrap_or(0) == 0 {
            break;
        }
        head.push(byte[0]);
    }
    String::from_utf8_lossy(&head).into_owned()
}

/// Writes a 200 head with a chunked body and these extra header lines.
pub async fn write_head(stream: &mut TcpStream, headers: &[(&str, &str)]) -> std::io::Result<()> {
    let mut head = String::from("HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n");
    for (name, value) in headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes()).await?;
    stream.flush().await
}

/// Writes one body chunk.
pub async fn write_chunk(stream: &mut TcpStream, data: &[u8]) -> std::io::Result<()> {
    stream
        .write_all(format!("{:x}\r\n", data.len()).as_bytes())
        .await?;
    stream.write_all(data).await?;
    stream.write_all(b"\r\n").await?;
    stream.flush().await
}

/// Writes the last chunk.
pub async fn write_end(stream: &mut TcpStream) -> std::io::Result<()> {
    stream.write_all(b"0\r\n\r\n").await?;
    stream.flush().await
}

/// Reads a chunked request body until its last chunk or the end of the connection. Returns the
/// data and whether the last chunk arrived (a complete body).
pub async fn read_chunked_body(stream: &mut TcpStream) -> (Vec<u8>, bool) {
    let mut raw = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        if let Some(parsed) = parse_chunked(&raw) {
            return (parsed, true);
        }
        match stream.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(n) => raw.extend_from_slice(&buf[..n]),
        }
    }
    (decode_partial_chunked(&raw), false)
}

/// The data of a complete chunked body, if `raw` holds one.
fn parse_chunked(raw: &[u8]) -> Option<Vec<u8>> {
    let mut data = Vec::new();
    let mut at = 0;
    loop {
        let line_end = find(&raw[at..], b"\r\n")? + at;
        let size = usize::from_str_radix(std::str::from_utf8(&raw[at..line_end]).ok()?, 16).ok()?;
        at = line_end + 2;
        if size == 0 {
            return raw[at..].starts_with(b"\r\n").then_some(data);
        }
        if raw.len() < at + size + 2 {
            return None;
        }
        data.extend_from_slice(&raw[at..at + size]);
        at += size + 2;
    }
}

/// The data of the complete chunks received so far.
pub fn chunked_data(raw: &[u8]) -> Vec<u8> {
    parse_chunked(raw).unwrap_or_else(|| decode_partial_chunked(raw))
}

/// The data of the complete chunks of a truncated chunked body.
fn decode_partial_chunked(raw: &[u8]) -> Vec<u8> {
    let mut data = Vec::new();
    let mut at = 0;
    while let Some(line) = find(&raw[at..], b"\r\n") {
        let Ok(size) =
            usize::from_str_radix(std::str::from_utf8(&raw[at..at + line]).unwrap_or("x"), 16)
        else {
            break;
        };
        at += line + 2;
        if size == 0 || raw.len() < at + size {
            break;
        }
        data.extend_from_slice(&raw[at..at + size]);
        at += size + 2;
        if at > raw.len() {
            break;
        }
    }
    data
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// A client without proxies.
pub fn client() -> reqwest::Client {
    reqwest::Client::builder().no_proxy().build().unwrap()
}

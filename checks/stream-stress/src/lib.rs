//! Generated large-read and large-upload workloads for the incremental formats, with scoped
//! measurements.
//!
//! Every workload runs through Concord's public paths: an `api!` declaration prepares the request,
//! `send_raw` returns the native response, a `concord::jsonl`/`concord::csv` reader opens it and
//! parses records incrementally; uploads send a one-shot generated source through the static
//! `Encode<StreamRecords<_>>`/`Encode<StreamRows<_>>` implementations. The server
//! (`concord-example-testserver`) generates bodies chunk by chunk and counts what it received;
//! nothing is materialized on either side.
//!
//! What is measured, and its scope:
//!
//! - **records / bytes**: records delivered by the reader and decoded body bytes it consumed;
//!   bytes the generator produced; bytes the server handed to its socket or received.
//! - **first record**: time from sending the request to the first record, compared with the time
//!   the server finished producing the body (a whole-body buffering reader could not deliver a
//!   record before the server finished).
//! - **library-owned retention**: the reader's `peak_retained_bytes()` (unfinished-record buffer
//!   owned by Concord) — not reqwest's buffers, not the socket, not the caller's sink.
//! - **in-flight window**: the largest observed difference between bytes the generator produced
//!   and bytes the reader consumed (transport + socket buffering between them; generator chunks
//!   are 64 KiB).
//! - **process memory**: resident set size sampled every 20 ms (Linux `/proc/self/statm`; `None`
//!   elsewhere). It covers the whole process (runtime, reqwest, server, generator); it is reported
//!   as a peak and as the peak minus the baseline before the workload, never as a library bound.
//! - **upload source polls**: polls of the one-shot source before sending (must be 0) and in total.

#![allow(missing_docs)]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use concord::csv::{Csv, CsvReader, CsvRecords, StreamRows};
use concord::jsonl::{
    JsonLines, JsonLinesOptions, JsonLinesReader, JsonLinesRecords, StreamRecords,
};
use concord::stream::{FromIter, ReadErrorKind, from_iter};
use concord_example_testserver::{Body, Receive, Reply, TestServer};
use serde::{Deserialize, Serialize};

/// Bytes of one generated record line (fixed, so N/2N differ only in count).
pub const RECORD_BYTES: usize = 128;
/// Generator chunk size.
pub const CHUNK_BYTES: usize = 64 * 1024;
/// The reader's per-record limit in every workload: small records, so the library window is small.
pub const MAX_RECORD_BYTES: usize = 4 * 1024;

/// One generated record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rec {
    pub id: u64,
    pub tenant: String,
    pub payload: String,
}

/// The deterministic record `i` (no randomness: the seed is the index).
pub fn rec(i: u64) -> Rec {
    let tenant = format!("t-{}", i % 7);
    // `{"id":…,"tenant":"…","payload":"…"}\n` padded to RECORD_BYTES.
    let fixed = format!("{{\"id\":{i},\"tenant\":\"{tenant}\",\"payload\":\"\"}}\n").len();
    Rec {
        id: i,
        tenant,
        payload: "x".repeat(RECORD_BYTES.saturating_sub(fixed)),
    }
}

/// The JSON Lines bytes of record `i`.
pub fn jsonl_line(i: u64) -> Vec<u8> {
    let mut line = serde_json::to_vec(&rec(i)).unwrap();
    line.push(b'\n');
    line
}

/// The CSV header of the generated table (`HasHeader`, comma, CRLF).
pub const CSV_HEADER: &[u8] = b"id,tenant,payload\r\n";

/// The CSV bytes of record `i` (same record as [`jsonl_line`], padded to `RECORD_BYTES`).
pub fn csv_line(i: u64) -> Vec<u8> {
    let tenant = format!("t-{}", i % 7);
    let fixed = format!("{i},{tenant},\r\n").len();
    format!("{i},{tenant},{}\r\n", "x".repeat(RECORD_BYTES - fixed)).into_bytes()
}

/// Bytes of one uploaded record line: the JSON Lines serialization of [`rec`] is `RECORD_BYTES` by
/// construction; its CSV serialization (`id,tenant,payload` + CRLF) is a constant 99 bytes, because
/// the payload shrinks by one byte per id digit.
pub fn upload_record_bytes(format: Format) -> u64 {
    match format {
        Format::Jsonl => RECORD_BYTES as u64,
        Format::Csv => {
            let len = |i: u64| {
                let r = rec(i);
                format!("{},{},{}\r\n", r.id, r.tenant, r.payload).len() as u64
            };
            assert_eq!(
                len(0),
                len(1_234_567_890),
                "CSV upload lines have a constant size"
            );
            len(0)
        }
    }
}

/// A streaming format under test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Jsonl,
    Csv,
}

impl Format {
    pub fn name(self) -> &'static str {
        match self {
            Format::Jsonl => "jsonl",
            Format::Csv => "csv",
        }
    }

    fn line(self, i: u64) -> Vec<u8> {
        match self {
            Format::Jsonl => jsonl_line(i),
            Format::Csv => csv_line(i),
        }
    }

    fn content_type(self) -> &'static str {
        match self {
            Format::Jsonl => "application/jsonl",
            Format::Csv => "text/csv; header=present",
        }
    }
}

/// The two incremental readers behind one interface (the harness's own convenience).
enum Records {
    Jsonl(JsonLinesRecords<Rec>),
    Csv(CsvRecords<Rec>),
}

impl Records {
    async fn next(&mut self) -> Option<Result<Rec, concord::stream::ReadError>> {
        match self {
            Records::Jsonl(r) => r.next().await,
            Records::Csv(r) => r.next().await,
        }
    }

    fn position(&self) -> concord::stream::Position {
        match self {
            Records::Jsonl(r) => r.position(),
            Records::Csv(r) => r.position(),
        }
    }

    fn peak_retained_bytes(&self) -> usize {
        match self {
            Records::Jsonl(r) => r.peak_retained_bytes(),
            Records::Csv(r) => r.peak_retained_bytes(),
        }
    }
}

/// A JSON measurement record.
#[derive(Debug, Clone, Serialize, Default)]
pub struct Measurement {
    pub scenario: String,
    pub format: String,
    pub direction: String,
    pub records_requested: u64,
    pub records_delivered: u64,
    pub bytes_generated: u64,
    pub bytes_consumed: u64,
    pub max_record_bytes: usize,
    pub chunk_bytes: usize,
    pub first_record_ms: Option<f64>,
    pub body_finished_ms: Option<f64>,
    pub total_ms: f64,
    pub throughput_mib_s: f64,
    pub peak_retained_bytes: usize,
    pub peak_in_flight_bytes: u64,
    pub rss_baseline_bytes: Option<u64>,
    pub rss_peak_bytes: Option<u64>,
    pub rss_peak_over_baseline_bytes: Option<u64>,
    pub source_polls_before_send: Option<u64>,
    pub source_polls_total: Option<u64>,
    pub error_kind: Option<String>,
    pub error_after_records: Option<u64>,
    /// After a cancellation: generated bytes (read) or source polls (upload) at the cancellation
    /// and 200 ms later; equal values mean no progress after the cancellation.
    pub progress_at_cancel: Option<u64>,
    pub progress_after_cancel: Option<u64>,
    pub notes: Vec<String>,
}

/// Resident set size of this process, when the platform exposes it cheaply.
pub fn rss_bytes() -> Option<u64> {
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
    Some(pages * 4096)
}

/// Samples RSS every 20 ms until dropped; reports the peak.
pub struct RssSampler {
    peak: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl RssSampler {
    pub fn start() -> Self {
        let peak = Arc::new(AtomicU64::new(rss_bytes().unwrap_or(0)));
        let stop = Arc::new(AtomicBool::new(false));
        let (p, s) = (Arc::clone(&peak), Arc::clone(&stop));
        let task = tokio::spawn(async move {
            while !s.load(Ordering::SeqCst) {
                if let Some(rss) = rss_bytes() {
                    p.fetch_max(rss, Ordering::SeqCst);
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        });
        Self {
            peak,
            stop,
            task: Some(task),
        }
    }

    pub async fn finish(mut self) -> Option<u64> {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
        rss_bytes().map(|_| self.peak.load(Ordering::SeqCst))
    }
}

impl Drop for RssSampler {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

/// A deliberate failure of the generated upload source.
#[derive(Debug)]
pub struct GenError(pub u64);

impl std::fmt::Display for GenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "the generated source failed at record {}", self.0)
    }
}

impl std::error::Error for GenError {}

/// The source type of generated uploads: a boxed lazy iterator (one record per poll).
pub type Generated = FromIter<Box<dyn Iterator<Item = Result<Rec, GenError>> + Send>>;

concord::api! {
    /// The loopback workload service.
    pub client Workload() {
        base "http://workload.test";

        /// A generated export (any format; the reader checks the media type).
        GET export(query rows: u64) { path "export"; }

        /// A JSON Lines upload of a one-shot generated source.
        POST upload_jsonl(body records: StreamRecords<Generated>) { path "upload"; request JsonLines; }

        /// A CSV upload (`Csv` = `HasHeader`, comma; header inferred from the rows' field names).
        POST upload_csv(body rows: StreamRows<Generated>) { path "upload"; request Csv; }
    }
}

fn client(server: &TestServer) -> Workload {
    let http = concord::reqwest::Client::builder()
        .no_proxy()
        .build()
        .unwrap();
    Workload::new(http).with_base_url(server.url()).unwrap()
}

/// How a read workload's body ends.
#[derive(Clone, Copy, Debug)]
pub enum Ending {
    /// All requested records, then the end of the body.
    Complete,
    /// The connection drops after this many records (a transport failure after a prefix).
    FailAfter(u64),
}

/// A generated body served lazily, counting generated bytes.
fn server_reply(
    format: Format,
    rows: u64,
    ending: Ending,
    generated: Arc<AtomicU64>,
    finished: Arc<AtomicU64>,
    t0: Instant,
) -> Reply {
    let limit = match ending {
        Ending::Complete => rows,
        Ending::FailAfter(n) => n.min(rows),
    };
    let mut next = 0u64;
    let mut header = format == Format::Csv;
    let body = move || {
        if header {
            header = false;
            generated.fetch_add(CSV_HEADER.len() as u64, Ordering::SeqCst);
            return Some(concord::bytes::Bytes::from_static(CSV_HEADER));
        }
        if next >= limit {
            finished.store(t0.elapsed().as_micros() as u64, Ordering::SeqCst);
            return None;
        }
        let mut chunk = Vec::with_capacity(CHUNK_BYTES + RECORD_BYTES);
        while next < limit && chunk.len() < CHUNK_BYTES {
            chunk.extend_from_slice(&format.line(next));
            next += 1;
        }
        generated.fetch_add(chunk.len() as u64, Ordering::SeqCst);
        Some(chunk.into())
    };
    let body = match ending {
        Ending::Complete => Body::Generate(Box::new(body), None),
        Ending::FailAfter(_) => {
            // Generate the prefix lazily into chunks, then drop the connection.
            let mut gen_body = body;
            let mut chunks = Vec::new();
            while let Some(c) = gen_body() {
                chunks.push(c);
            }
            Body::FailAfter(chunks)
        }
    };
    Reply::new(200)
        .header("content-type", format.content_type())
        .with_body(body)
}

/// Options of a read workload.
#[derive(Clone, Copy, Debug)]
pub struct ReadPlan {
    pub rows: u64,
    pub ending: Ending,
    /// Pause after every `n` records (a slow consumer), if any.
    pub slow: Option<(u64, Duration)>,
    /// Stop (drop the reader) after this many records, if any.
    pub cancel_after: Option<u64>,
}

/// Reads a generated JSON Lines export through the public reader.
pub async fn jsonl_read(scenario: &str, plan: ReadPlan) -> Measurement {
    read(Format::Jsonl, scenario, plan).await
}

/// Reads a generated CSV export through the public reader (`Csv` policies, small record limit).
pub async fn csv_read(scenario: &str, plan: ReadPlan) -> Measurement {
    read(Format::Csv, scenario, plan).await
}

/// Reads a generated export of `format` through its public incremental reader.
pub async fn read(format: Format, scenario: &str, plan: ReadPlan) -> Measurement {
    let generated = Arc::new(AtomicU64::new(0));
    let finished = Arc::new(AtomicU64::new(0));
    let t0 = Instant::now();
    let (g, f) = (Arc::clone(&generated), Arc::clone(&finished));
    let ending = plan.ending;
    let server = TestServer::start(move |r| {
        let rows = r
            .query_all("rows")
            .first()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        server_reply(format, rows, ending, Arc::clone(&g), Arc::clone(&f), t0)
    })
    .await;
    let api = client(&server);
    let baseline = rss_bytes();
    let sampler = RssSampler::start();
    let start = Instant::now();
    let response = api
        .export(plan.rows)
        .prepare()
        .unwrap()
        .send_raw()
        .await
        .unwrap();
    let mut records = match format {
        Format::Jsonl => Records::Jsonl(
            JsonLinesReader::new(JsonLinesOptions::json_lines().max_record_bytes(MAX_RECORD_BYTES))
                .open_for::<Rec>(response, "stress: jsonl export")
                .unwrap(),
        ),
        Format::Csv => Records::Csv(
            CsvReader::new(
                Csv::<concord::csv::HasHeader>::options().max_record_bytes(MAX_RECORD_BYTES),
            )
            .unwrap()
            .open_for::<Rec>(response, "stress: csv export")
            .unwrap(),
        ),
    };
    let mut m = Measurement {
        scenario: scenario.into(),
        format: format.name().into(),
        direction: "read".into(),
        records_requested: plan.rows,
        max_record_bytes: MAX_RECORD_BYTES,
        chunk_bytes: CHUNK_BYTES,
        ..Default::default()
    };
    let mut expected = 0u64;
    let mut peak_in_flight = 0u64;
    while let Some(next) = records.next().await {
        match next {
            Ok(r) => {
                assert_eq!(r.id, expected, "records arrive in order, unchanged");
                expected += 1;
                if expected == 1 {
                    m.first_record_ms = Some(start.elapsed().as_secs_f64() * 1e3);
                }
                let consumed = records.position().bytes;
                peak_in_flight =
                    peak_in_flight.max(generated.load(Ordering::SeqCst).saturating_sub(consumed));
                if let Some((every, pause)) = plan.slow
                    && expected % every == 0
                {
                    tokio::time::sleep(pause).await;
                }
                if plan.cancel_after == Some(expected) {
                    break;
                }
            }
            Err(e) => {
                m.error_kind = Some(format!("{:?}", e.kind()));
                m.error_after_records = Some(e.position().records);
                if e.kind() == ReadErrorKind::Transport {
                    m.notes.push(
                        "transport failure after a prefix: prefix delivered, reader fused".into(),
                    );
                }
                break;
            }
        }
    }
    m.records_delivered = expected;
    m.bytes_consumed = records.position().bytes;
    m.peak_retained_bytes = records.peak_retained_bytes();
    drop(records);
    m.total_ms = start.elapsed().as_secs_f64() * 1e3;
    if plan.cancel_after.is_some() {
        // After the drop, the server must notice the closed connection and stop generating.
        let closed = server
            .wait_until(Duration::from_secs(10), |s| s.active_bodies() == 0)
            .await;
        let at_drop = generated.load(Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(200)).await;
        let later = generated.load(Ordering::SeqCst);
        m.progress_at_cancel = Some(at_drop);
        m.progress_after_cancel = Some(later);
        m.notes.push(format!(
            "cancelled after {} records: server body ended {} ; generated bytes {} at drop, {} 200 ms later",
            expected,
            if closed { "promptly" } else { "NOT within 10 s" },
            at_drop,
            later
        ));
    }
    let f = finished.load(Ordering::SeqCst);
    m.body_finished_ms = (f > 0).then(|| f as f64 / 1e3 - (start - t0).as_secs_f64() * 1e3);
    m.bytes_generated = generated.load(Ordering::SeqCst);
    m.peak_in_flight_bytes = peak_in_flight;
    m.throughput_mib_s = m.bytes_consumed as f64 / (1024.0 * 1024.0) / (m.total_ms / 1e3);
    m.rss_peak_bytes = sampler.finish().await;
    m.rss_baseline_bytes = baseline;
    m.rss_peak_over_baseline_bytes = match (m.rss_peak_bytes, baseline) {
        (Some(p), Some(b)) => Some(p.saturating_sub(b)),
        _ => None,
    };
    let cleanup = server.shutdown().await;
    assert_eq!(cleanup.open_after, 0);
    m
}

/// Options of an upload workload.
#[derive(Clone, Copy, Debug)]
pub struct UploadPlan {
    pub rows: u64,
    /// The source fails at this record (a late source error after a received prefix), if any.
    pub fail_at: Option<u64>,
    /// A slow receiver: pause after every read of the body.
    pub receiver_delay: Option<Duration>,
    /// Cancel the send after this long, if any.
    pub cancel_after: Option<Duration>,
}

/// Sends a generated one-shot JSON Lines source through the static `Encode<StreamRecords<_>>`.
pub async fn jsonl_upload(scenario: &str, plan: UploadPlan) -> Measurement {
    upload(Format::Jsonl, scenario, plan).await
}

/// Sends a generated one-shot CSV source through the static `Encode<StreamRows<_>>`.
pub async fn csv_upload(scenario: &str, plan: UploadPlan) -> Measurement {
    upload(Format::Csv, scenario, plan).await
}

/// Sends a generated one-shot source of `format` through its static streaming `Encode`.
pub async fn upload(format: Format, scenario: &str, plan: UploadPlan) -> Measurement {
    let receive = Receive {
        record_limit: 0,
        read_size: 16 * 1024,
        read_delay: plan.receiver_delay,
    };
    let server = TestServer::start_with(receive, |_| Reply::new(204)).await;
    let api = client(&server);
    let polls = Arc::new(AtomicU64::new(0));
    let p = Arc::clone(&polls);
    let rows = plan.rows;
    let fail_at = plan.fail_at;
    let iter: Box<dyn Iterator<Item = Result<Rec, GenError>> + Send> =
        Box::new((0..rows).map(move |i| {
            p.fetch_add(1, Ordering::SeqCst);
            if fail_at == Some(i) {
                return Err(GenError(i));
            }
            Ok(rec(i))
        }));
    let baseline = rss_bytes();
    let sampler = RssSampler::start();
    let source = from_iter(iter);
    let prepared = match format {
        Format::Jsonl => api
            .upload_jsonl(StreamRecords::new(source))
            .prepare()
            .unwrap(),
        Format::Csv => api.upload_csv(StreamRows::new(source)).prepare().unwrap(),
    };
    let polls_before = polls.load(Ordering::SeqCst);
    let start = Instant::now();
    let mut m = Measurement {
        scenario: scenario.into(),
        format: format.name().into(),
        direction: "upload".into(),
        records_requested: rows,
        chunk_bytes: 16 * 1024,
        source_polls_before_send: Some(polls_before),
        ..Default::default()
    };
    let send = prepared.send();
    let result = match plan.cancel_after {
        Some(after) => match tokio::time::timeout(after, send).await {
            Ok(r) => Some(r),
            Err(_) => None,
        },
        None => Some(send.await),
    };
    m.total_ms = start.elapsed().as_secs_f64() * 1e3;
    match result {
        Some(Ok(())) => {}
        Some(Err(e)) => m.error_kind = Some(format!("{:?}", e.kind())),
        None => {
            let at_cancel = polls.load(Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(200)).await;
            let later = polls.load(Ordering::SeqCst);
            m.progress_at_cancel = Some(at_cancel);
            m.progress_after_cancel = Some(later);
            m.notes.push(format!(
                "send cancelled after {:?}: source polls {at_cancel} at cancel, {later} 200 ms later",
                plan.cancel_after.unwrap()
            ));
        }
    }
    m.source_polls_total = Some(polls.load(Ordering::SeqCst));
    // Record lines have the same size in both formats; CSV adds its header line first.
    let header = if format == Format::Csv {
        CSV_HEADER.len() as u64
    } else {
        0
    };
    m.bytes_consumed = server.request_body_bytes();
    let per_record = upload_record_bytes(format);
    m.bytes_generated = header + m.source_polls_total.unwrap_or(0) * per_record;
    m.records_delivered = m.bytes_consumed.saturating_sub(header) / per_record;
    m.throughput_mib_s = m.bytes_consumed as f64 / (1024.0 * 1024.0) / (m.total_ms / 1e3);
    m.rss_peak_bytes = sampler.finish().await;
    m.rss_baseline_bytes = baseline;
    m.rss_peak_over_baseline_bytes = match (m.rss_peak_bytes, baseline) {
        (Some(p), Some(b)) => Some(p.saturating_sub(b)),
        _ => None,
    };
    let cleanup = server.shutdown().await;
    assert_eq!(cleanup.open_after, 0);
    m
}

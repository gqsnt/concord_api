//! Work per poll of the three incremental readers, observed with always-ready sources and a
//! no-op waker, so that every `Pending` is one of the readers' cooperative yields:
//!
//! - a run of ready empty chunks costs at most 64 source polls per poll of `next()` (the shared
//!   driver yields after every 64th empty chunk), an endless run never holds one poll forever,
//!   cancelling a `next()` in the middle of a run loses nothing, and an empty chunk is never
//!   taken for the end of input (CSV, JSON Lines, JSON values);
//! - one large chunk is framed at most 64 KiB per poll, whether the record completes in it or
//!   not, and the record read in many steps is the same record (JSON Lines, JSON values; CSV as
//!   the control that already sliced its input).
#![cfg(any(feature = "csv", feature = "jsonl"))]

use std::cmp::Ordering;
use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
use std::task::{Context, Poll, Waker};

use concord_core::bytes::Bytes;
use concord_extras::stream::{ByteSource, Chunks, ReadError, SourceError};

/// The documented bound: a reader yields after every 64th empty chunk of its source.
const EMPTY_CHUNKS_PER_POLL: usize = 64;
/// The documented framing budget of one poll.
const BUDGET: u64 = 64 * 1024;
/// Ready empty chunks before the record in the bounded runs.
const EMPTY_RUN: usize = 100_000;
/// Polls of `next()` given to an endless run of empty chunks.
const ENDLESS_POLLS: usize = 1_000;

/// `empty` ready empty chunks, then `payload`, then the end of input; counts every poll.
struct EmptyRun {
    polls: Arc<AtomicUsize>,
    empty: usize,
    payload: &'static [u8],
}

impl EmptyRun {
    fn new(empty: usize, payload: &'static [u8]) -> (Self, Arc<AtomicUsize>) {
        let polls = Arc::new(AtomicUsize::new(0));
        let source = Self {
            polls: Arc::clone(&polls),
            empty,
            payload,
        };
        (source, polls)
    }
}

impl ByteSource for EmptyRun {
    async fn next_chunk(&mut self) -> Result<Option<Bytes>, SourceError> {
        let n = self.polls.fetch_add(1, SeqCst);
        Ok(match n.cmp(&self.empty) {
            Ordering::Less => Some(Bytes::new()),
            Ordering::Equal => Some(Bytes::from_static(self.payload)),
            Ordering::Greater => None,
        })
    }
}

/// Ready empty chunks forever. A safety stop (a source error after a million polls) turns a
/// reader that never yields into a failed assertion instead of a hung test.
struct Endless {
    polls: Arc<AtomicUsize>,
}

impl Endless {
    fn new() -> (Self, Arc<AtomicUsize>) {
        let polls = Arc::new(AtomicUsize::new(0));
        (
            Self {
                polls: Arc::clone(&polls),
            },
            polls,
        )
    }
}

impl ByteSource for Endless {
    async fn next_chunk(&mut self) -> Result<Option<Bytes>, SourceError> {
        if self.polls.fetch_add(1, SeqCst) >= 1_000_000 {
            return Err(SourceError::new("safety stop: one poll never yielded"));
        }
        Ok(Some(Bytes::new()))
    }
}

/// One run of a future to completion with a no-op waker.
struct Run<T> {
    output: T,
    pendings: usize,
    /// The most source polls made during one poll of the future.
    max_source_polls: usize,
}

fn run<F: Future>(future: F, polls: &AtomicUsize) -> Run<F::Output> {
    let mut future = pin!(future);
    let mut cx = Context::from_waker(Waker::noop());
    let (mut pendings, mut max_source_polls) = (0, 0);
    loop {
        let before = polls.load(SeqCst);
        let poll = future.as_mut().poll(&mut cx);
        max_source_polls = max_source_polls.max(polls.load(SeqCst) - before);
        match poll {
            Poll::Ready(output) => {
                return Run {
                    output,
                    pendings,
                    max_source_polls,
                };
            }
            Poll::Pending => {
                pendings += 1;
                assert!(pendings < 10_000_000, "the future never completes");
            }
        }
    }
}

fn poll_once<F: Future>(future: F) -> Poll<F::Output> {
    pin!(future)
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
}

/// The record after `EMPTY_RUN` empty chunks: delivered once, every chunk polled exactly once,
/// at most 64 empty chunks (plus the chunk ending the run) per poll.
fn assert_empty_run<T: std::fmt::Debug + PartialEq>(
    name: &str,
    run: Run<Option<Result<T, ReadError>>>,
    polls: &AtomicUsize,
    expected: T,
) {
    println!(
        "{name}: {EMPTY_RUN} empty chunks: {} pendings, at most {} source polls per poll",
        run.pendings, run.max_source_polls
    );
    assert_eq!(run.output.expect("a record").expect("no error"), expected);
    assert!(
        run.max_source_polls <= EMPTY_CHUNKS_PER_POLL + 1,
        "{name}: {} source polls in one poll",
        run.max_source_polls
    );
    assert!(
        run.pendings >= EMPTY_RUN / EMPTY_CHUNKS_PER_POLL,
        "{name}: {} pendings",
        run.pendings
    );
    assert_eq!(polls.load(SeqCst), EMPTY_RUN + 1, "{name}: polls");
}

/// Every poll of `next()` over an endless run of empty chunks returns `Pending` after at most
/// 64 source polls.
fn assert_endless<R: Reader>(name: &str, reader: &mut R, polls: &AtomicUsize) {
    for i in 0..ENDLESS_POLLS {
        let before = polls.load(SeqCst);
        assert!(
            poll_once(reader.step()).is_pending(),
            "{name}: poll {i} returned after {} source polls",
            polls.load(SeqCst) - before
        );
        assert!(polls.load(SeqCst) - before <= EMPTY_CHUNKS_PER_POLL);
    }
    println!(
        "{name}: endless empty chunks: {ENDLESS_POLLS} polls, {} source polls",
        polls.load(SeqCst)
    );
}

#[cfg(feature = "csv")]
#[test]
fn csv_empty_chunk_runs_are_budgeted() {
    use concord_extras::csv::{CsvOptions, CsvReader, HeaderMode};
    let reader = CsvReader::new(CsvOptions::new(HeaderMode::Absent, b',')).unwrap();

    let (source, polls) = EmptyRun::new(EMPTY_RUN, b"1\n");
    let mut rows = reader.read::<(u64,), _>(source);
    assert_empty_run("csv", run(rows.next(), &polls), &polls, (1,));
    // The empty chunks were not an end; the true end is.
    assert!(run(rows.next(), &polls).output.is_none());
    assert_eq!(polls.load(SeqCst), EMPTY_RUN + 2);

    let (source, polls) = Endless::new();
    let mut rows = reader.read::<(u64,), _>(source);
    assert_endless("csv", &mut rows, &polls);

    // A `next()` dropped while it yields in the middle of a run loses nothing.
    let (source, polls) = EmptyRun::new(1000, b"7\n");
    let mut rows = reader.read::<(u64,), _>(source);
    assert!(poll_once(rows.next()).is_pending());
    assert_eq!(run(rows.next(), &polls).output.unwrap().unwrap(), (7,));
    assert_eq!(polls.load(SeqCst), 1001);
}

#[cfg(feature = "jsonl")]
#[test]
fn jsonl_empty_chunk_runs_are_budgeted() {
    use concord_extras::jsonl::{JsonLinesOptions, JsonLinesReader};
    let reader = JsonLinesReader::new(JsonLinesOptions::json_lines());

    let (source, polls) = EmptyRun::new(EMPTY_RUN, b"1\n");
    let mut records = reader.read::<u64, _>(source);
    assert_empty_run("jsonl", run(records.next(), &polls), &polls, 1);
    assert!(run(records.next(), &polls).output.is_none());
    assert_eq!(polls.load(SeqCst), EMPTY_RUN + 2);

    let (source, polls) = Endless::new();
    let mut records = reader.read::<u64, _>(source);
    assert_endless("jsonl", &mut records, &polls);

    let (source, polls) = EmptyRun::new(1000, b"7\n");
    let mut records = reader.read::<u64, _>(source);
    assert!(poll_once(records.next()).is_pending());
    assert_eq!(run(records.next(), &polls).output.unwrap().unwrap(), 7);
    assert_eq!(polls.load(SeqCst), 1001);
}

#[cfg(feature = "jsonl")]
#[test]
fn json_values_empty_chunk_runs_are_budgeted() {
    use concord_extras::jsonl::JsonValuesReader;
    let reader = JsonValuesReader::new();

    let (source, polls) = EmptyRun::new(EMPTY_RUN, b"1\n");
    let mut values = reader.read::<u64, _>(source);
    assert_empty_run("values", run(values.next(), &polls), &polls, 1);
    assert!(run(values.next(), &polls).output.is_none());
    assert_eq!(polls.load(SeqCst), EMPTY_RUN + 2);

    let (source, polls) = Endless::new();
    let mut values = reader.read::<u64, _>(source);
    assert_endless("values", &mut values, &polls);

    let (source, polls) = EmptyRun::new(1000, b"7 ");
    let mut values = reader.read::<u64, _>(source);
    assert!(poll_once(values.next()).is_pending());
    assert_eq!(run(values.next(), &polls).output.unwrap().unwrap(), 7);
    assert_eq!(polls.load(SeqCst), 1001);
}

/// The readers under one interface, to read a record one poll at a time.
trait Reader {
    type Item;
    fn step(&mut self) -> impl Future<Output = Option<Result<Self::Item, ReadError>>>;
    fn bytes(&self) -> u64;
}

#[cfg(feature = "jsonl")]
impl<T: serde::de::DeserializeOwned, S: ByteSource> Reader
    for concord_extras::jsonl::JsonLinesRecords<T, S>
{
    type Item = T;
    fn step(&mut self) -> impl Future<Output = Option<Result<T, ReadError>>> {
        self.next()
    }
    fn bytes(&self) -> u64 {
        self.position().bytes
    }
}

#[cfg(feature = "jsonl")]
impl<T: serde::de::DeserializeOwned, S: ByteSource> Reader
    for concord_extras::jsonl::JsonValues<T, S>
{
    type Item = T;
    fn step(&mut self) -> impl Future<Output = Option<Result<T, ReadError>>> {
        self.next()
    }
    fn bytes(&self) -> u64 {
        self.position().bytes
    }
}

#[cfg(feature = "csv")]
impl<T, S: ByteSource> Reader for concord_extras::csv::CsvRecords<T, S> {
    type Item = T;
    fn step(&mut self) -> impl Future<Output = Option<Result<T, ReadError>>> {
        self.next()
    }
    fn bytes(&self) -> u64 {
        self.position().bytes
    }
}

/// Reads the next record giving each poll a fresh `next()` (cancel-safe: dropping a pending one
/// loses nothing) and recording the input bytes consumed during each poll.
fn per_poll<R: Reader>(reader: &mut R) -> (Option<Result<R::Item, ReadError>>, Vec<u64>) {
    let mut steps = Vec::new();
    loop {
        let before = reader.bytes();
        let poll = poll_once(reader.step());
        steps.push(reader.bytes() - before);
        if let Poll::Ready(output) = poll {
            return (output, steps);
        }
        assert!(steps.len() < 1_000_000, "the reader never completes");
    }
}

/// An unfinished 8 MiB string in one ready chunk: the first poll frames at most 64 KiB, then
/// yields (the rest stays pending in the reader).
#[cfg(feature = "jsonl")]
fn unfinished_string() -> Bytes {
    let mut data = vec![b'x'; 8 * 1024 * 1024];
    data[0] = b'"';
    Bytes::from(data)
}

/// A complete 4 MiB string record (`"xx..x"` and a LF) in one ready chunk.
fn large_record() -> (Bytes, usize) {
    let len = 4 * 1024 * 1024;
    let mut data = Vec::with_capacity(len + 3);
    data.push(b'"');
    data.resize(len + 1, b'x');
    data.extend_from_slice(b"\"\n");
    (Bytes::from(data), len)
}

/// The record from one large chunk, read one poll at a time: every poll frames at most 64 KiB,
/// all input up to the record is consumed exactly once, and the record is intact.
#[cfg(feature = "jsonl")]
fn assert_large_record<R: Reader<Item = String>>(name: &str, mut reader: R, len: usize) {
    let (record, steps) = per_poll(&mut reader);
    let largest = steps.iter().copied().max().unwrap_or(0);
    println!(
        "{name}: a {len}-byte record in one chunk: {} polls, at most {largest} bytes per poll",
        steps.len()
    );
    assert_eq!(record.expect("a record").expect("no error").len(), len);
    assert!(largest <= BUDGET, "{name}: {largest} bytes in one poll");
    assert!(
        steps.len() as u64 >= len as u64 / BUDGET,
        "{name}: {steps:?}"
    );
    assert!(steps.iter().sum::<u64>() >= len as u64, "{name}");
}

#[cfg(feature = "jsonl")]
#[test]
fn jsonl_frames_a_large_chunk_in_bounded_steps() {
    use concord_extras::jsonl::{JsonLinesOptions, JsonLinesReader};
    let reader = JsonLinesReader::new(JsonLinesOptions::json_lines());

    let mut records = reader.read::<String, _>(Chunks::new([unfinished_string()]));
    assert!(poll_once(records.next()).is_pending());
    let first = records.position().bytes;
    println!("jsonl: unfinished 8 MiB string: {first} bytes consumed by the first poll");
    assert!(first <= BUDGET, "{first}");
    assert_eq!(
        records.pending_input_bytes() as u64,
        8 * 1024 * 1024 - first
    );

    let (chunk, len) = large_record();
    assert_large_record(
        "jsonl",
        reader.read::<String, _>(Chunks::new([chunk.clone()])),
        len,
    );
    // One future polled to completion yields at least once per 64 KiB as well.
    let mut records = reader.read::<String, _>(Chunks::new([chunk]));
    let polls = AtomicUsize::new(0);
    let whole = run(records.next(), &polls);
    assert_eq!(whole.output.unwrap().unwrap().len(), len);
    assert!(
        whole.pendings as u64 >= len as u64 / BUDGET - 1,
        "{}",
        whole.pendings
    );

    // A line over a small limit in one 32 MiB chunk is still refused by the first poll, before
    // any of it is buffered.
    let mut records = JsonLinesReader::new(JsonLinesOptions::json_lines().max_record_bytes(64))
        .read::<String, _>(Chunks::new([Bytes::from(vec![b'x'; 32 * 1024 * 1024])]));
    match poll_once(records.next()) {
        Poll::Ready(Some(Err(e))) => {
            assert_eq!(e.kind(), concord_extras::stream::ReadErrorKind::Limit);
            assert!(e.is_terminal());
        }
        other => panic!(
            "expected a limit error, got {:?}",
            other.map(|o| o.map(|r| r.is_ok()))
        ),
    }
    assert_eq!(records.peak_retained_bytes(), 0);
}

#[cfg(feature = "jsonl")]
#[test]
fn json_values_frame_a_large_chunk_in_bounded_steps() {
    use concord_extras::jsonl::JsonValuesReader;
    let reader = JsonValuesReader::new();

    let mut values = reader.read::<String, _>(Chunks::new([unfinished_string()]));
    assert!(poll_once(values.next()).is_pending());
    let first = values.position().bytes;
    println!("values: unfinished 8 MiB string: {first} bytes consumed by the first poll");
    assert!(first <= BUDGET, "{first}");
    assert_eq!(values.pending_input_bytes() as u64, 8 * 1024 * 1024 - first);

    let (chunk, len) = large_record();
    assert_large_record(
        "values",
        reader.read::<String, _>(Chunks::new([chunk.clone()])),
        len,
    );
    let mut values = reader.read::<String, _>(Chunks::new([chunk]));
    let polls = AtomicUsize::new(0);
    let whole = run(values.next(), &polls);
    assert_eq!(whole.output.unwrap().unwrap().len(), len);
    assert!(
        whole.pendings as u64 >= len as u64 / BUDGET - 1,
        "{}",
        whole.pendings
    );
}

/// The control: the CSV reader already framed at most 64 KiB per poll.
#[cfg(feature = "csv")]
#[test]
fn csv_frames_a_large_chunk_in_bounded_steps() {
    use concord_extras::csv::{CsvOptions, CsvReader, HeaderMode};
    let reader = CsvReader::new(
        CsvOptions::new(HeaderMode::Absent, b',').max_record_bytes(16 * 1024 * 1024),
    )
    .unwrap();
    let (chunk, len) = large_record();
    let (record, steps) = per_poll(&mut reader.read::<(String,), _>(Chunks::new([chunk])));
    let largest = steps.iter().copied().max().unwrap_or(0);
    println!(
        "csv: a {len}-byte field in one chunk: {} polls, at most {largest} bytes per poll",
        steps.len()
    );
    assert_eq!(record.unwrap().unwrap().0.len(), len);
    assert!(largest <= BUDGET, "{largest}");
}

//! Resource behaviour of the incremental reader (CSV.md §4 limits, §7, §8 items 12, 14 partly;
//! STREAMING.md resource contract): record/header/field limits act on generated input that never
//! ends, before the owned buffers pass them, with explicit `Limit` errors; total limits are not
//! clean ends; long skipped runs and large records yield to the executor; a stopped consumer
//! does not advance the source; `retained_bytes` reports the bounded buffers.

#![cfg(feature = "csv")]

mod csv_support;

use std::future::Future;
use std::pin::pin;
use std::sync::atomic::Ordering;
use std::task::{Context, Poll, Waker};

use concord_codecs::csv::{CsvOptions, CsvReader, HeaderMode};
use concord_codecs::stream::{Chunks, ReadErrorKind};
use concord_core::bytes::Bytes;
use csv_support::{Counting, Generated};

const CHUNK: usize = 4096;

/// Polls `future` to completion with a no-op waker, counting the `Pending`s (with a source that
/// is always ready, every `Pending` is a deliberate yield).
fn run_counting_yields<F: Future>(future: F) -> (F::Output, usize) {
    let mut future = pin!(future);
    let mut cx = Context::from_waker(Waker::noop());
    let mut pending = 0;
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(output) => return (output, pending),
            Poll::Pending => pending += 1,
        }
    }
}

/// An endless source: `prefix`, then `fill` repeated in 4 KiB chunks forever.
fn endless(
    prefix: &'static [u8],
    fill: u8,
) -> (
    Generated<impl FnMut(usize) -> Option<Bytes>>,
    std::sync::Arc<std::sync::atomic::AtomicUsize>,
) {
    Generated::new(move |i| {
        Some(if i == 0 {
            Bytes::from_static(prefix)
        } else {
            Bytes::from(vec![fill; CHUNK])
        })
    })
}

#[tokio::test]
async fn an_endless_record_stops_at_the_record_limit() {
    let limit = 64 * 1024;
    let reader =
        CsvReader::new(CsvOptions::new(HeaderMode::Present, b',').max_record_bytes(limit)).unwrap();
    let (source, produced) = endless(b"id,blob\n1,ok\n2,", b'a');
    let mut rows = reader.read::<(u32, String), _>(source);
    assert_eq!(rows.next().await.unwrap().unwrap(), (1, "ok".to_owned()));
    let error = rows.next().await.unwrap().unwrap_err();
    assert_eq!(error.kind(), ReadErrorKind::Limit);
    let codec = error.codec().unwrap();
    assert_eq!(codec.label(), "a CSV record exceeds the configured size");
    assert_eq!(codec.context(), [("record", 2), ("limit", limit as u64)]);
    assert_eq!(error.position().records, 1);
    assert!(rows.next().await.is_none());

    // The source was read only as far as the limit (plus the chunk in hand), and the field
    // buffer never passed the limit.
    let produced = produced.load(Ordering::SeqCst);
    assert!(produced <= limit + 2 * CHUNK + 64, "{produced}");
    let ends = 16 * std::mem::size_of::<usize>();
    assert!(
        rows.retained_bytes() <= limit + ends,
        "{}",
        rows.retained_bytes()
    );
    assert!(rows.peak_retained_bytes() <= limit + ends);
    assert!(rows.peak_retained_bytes() >= limit);
}

#[tokio::test]
async fn an_endless_header_stops_at_the_header_limit() {
    let reader =
        CsvReader::new(CsvOptions::new(HeaderMode::Present, b',').max_header_bytes(10_000))
            .unwrap();
    let (source, produced) = endless(b"name", b'x');
    let mut rows = reader.read::<Vec<String>, _>(source);
    let error = rows.next().await.unwrap().unwrap_err();
    assert_eq!(error.kind(), ReadErrorKind::Limit);
    assert_eq!(
        error.codec().unwrap().label(),
        "the CSV header exceeds the configured size"
    );
    assert!(produced.load(Ordering::SeqCst) <= 10_000 + 2 * CHUNK);
    assert!(rows.peak_retained_bytes() <= 10_000 + 16 * std::mem::size_of::<usize>());
}

#[tokio::test]
async fn endless_fields_stop_at_the_field_limit() {
    let reader = CsvReader::new(CsvOptions::new(HeaderMode::Absent, b',').max_fields(100)).unwrap();
    let (source, produced) = endless(b"", b',');
    let mut rows = reader.read_raw(source);
    let error = rows.next().await.unwrap().unwrap_err();
    assert_eq!(error.kind(), ReadErrorKind::Limit);
    assert_eq!(
        error.codec().unwrap().label(),
        "a CSV record has more fields than the configured limit"
    );
    assert!(produced.load(Ordering::SeqCst) <= 2 * CHUNK);
    assert!(rows.peak_retained_bytes() <= 256 + 100 * std::mem::size_of::<usize>());
}

#[tokio::test]
async fn records_exactly_at_the_limits_are_accepted_on_every_split() {
    let options = CsvOptions::new(HeaderMode::Present, b',')
        .max_header_bytes(5)
        .max_record_bytes(6)
        .max_fields(3);
    let reader = CsvReader::new(options).unwrap();
    // Header: 5 field bytes, 3 fields. Record: 6 field bytes (quotes and escapes not counted).
    let body = b"ab,c,d\r\n\"a\"\"\",bc,d\r\n";
    for chunks in csv_support::single_splits(body) {
        let got = csv_support::streamed(&reader, Chunks::new(chunks)).await;
        assert_eq!(
            got,
            csv_support::Outcome::ok(Some(&["ab", "c", "d"]), &[&["a\"", "bc", "d"]])
        );
    }
    let over = b"ab,c,d\r\n\"a\"\"\",bcde,f\r\n"; // 7 field bytes
    let got = csv_support::streamed(&reader, Chunks::new([over.to_vec()])).await;
    assert_eq!(
        got.error,
        Some((
            Some(ReadErrorKind::Limit),
            "a CSV record exceeds the configured size"
        ))
    );
}

#[tokio::test]
async fn total_limits_are_limit_errors_not_ends() {
    let body = b"a\nb\nc\n";
    // Exactly the totals: a clean end.
    let exact = CsvReader::new(
        CsvOptions::new(HeaderMode::Absent, b',')
            .max_total_bytes(6)
            .max_records(3),
    )
    .unwrap();
    let got = csv_support::streamed(&exact, Chunks::new([&body[..2], &body[2..]])).await;
    assert_eq!(
        got,
        csv_support::Outcome::ok(None, &[&["a"], &["b"], &["c"]])
    );

    let bytes =
        CsvReader::new(CsvOptions::new(HeaderMode::Absent, b',').max_total_bytes(5)).unwrap();
    let got = csv_support::streamed(&bytes, Chunks::new([&body[..4], &body[4..]])).await;
    assert_eq!(got.records, [["a"], ["b"]]);
    assert_eq!(
        got.error,
        Some((
            Some(ReadErrorKind::Limit),
            "the input exceeds the configured total bytes"
        ))
    );

    let records = CsvReader::new(CsvOptions::new(HeaderMode::Absent, b',').max_records(2)).unwrap();
    let got = csv_support::streamed(&records, Chunks::new([body.to_vec()])).await;
    assert_eq!(got.records, [["a"], ["b"]]);
    assert_eq!(
        got.error,
        Some((
            Some(ReadErrorKind::Limit),
            "the input exceeds the configured total records"
        ))
    );
}

#[test]
fn a_long_comment_run_yields_and_allocates_nothing() {
    let reader =
        CsvReader::new(CsvOptions::new(HeaderMode::Absent, b',').comment(Some(b'#'))).unwrap();
    let line = b"# a skipped comment line of a provider export\n";
    let lines_per_chunk = 1024;
    let chunks = 400; // ~18 MiB of comments, generated, never collected
    let (source, produced) = Generated::new(move |i| match i {
        i if i < chunks => Some(Bytes::from(line.repeat(lines_per_chunk))),
        i if i == chunks => Some(Bytes::from_static(b"x,y\n")),
        _ => None,
    });
    let mut rows = reader.read_raw(source);
    let (first, yields) = run_counting_yields(rows.next());
    assert_eq!(first.unwrap().unwrap().len(), 2);
    let skipped = produced.load(Ordering::SeqCst);
    assert!(skipped > 18_000_000, "{skipped}");
    // At most 64 KiB of skipped input per poll.
    assert!(
        yields >= skipped / (64 * 1024) - 1,
        "{yields} yields for {skipped} bytes"
    );
    assert!(rows.peak_retained_bytes() <= 256 + 16 * std::mem::size_of::<usize>());

    // Blank lines, likewise (no comment byte needed).
    let reader = CsvReader::new(CsvOptions::new(HeaderMode::Absent, b',')).unwrap();
    let (source, _) = Generated::new(move |i| match i {
        0..100 => Some(Bytes::from(vec![b'\n'; 100_000])),
        100 => Some(Bytes::from_static(b"x\r\n")),
        _ => None,
    });
    let mut rows = reader.read_raw(source);
    let (first, yields) = run_counting_yields(rows.next());
    assert_eq!(first.unwrap().unwrap().len(), 1);
    assert!(yields >= 10_000_000 / (64 * 1024) - 1, "{yields}");
}

#[test]
fn a_large_record_in_one_chunk_is_parsed_in_bounded_steps() {
    let reader = CsvReader::new(CsvOptions::new(HeaderMode::Absent, b',')).unwrap();
    let mut body = vec![b'z'; 4 * 1024 * 1024];
    body.push(b'\n');
    let mut rows = reader.read_raw(Chunks::new([body]));
    let (record, yields) = run_counting_yields(rows.next());
    assert_eq!(
        record.unwrap().unwrap().get(0).unwrap().len(),
        4 * 1024 * 1024
    );
    assert!(yields >= 60, "{yields}");
}

#[tokio::test]
async fn a_stopped_consumer_does_not_advance_the_source() {
    let reader = CsvReader::new(CsvOptions::new(HeaderMode::Present, b',')).unwrap();
    let mut chunks = vec![b"id,name\n".to_vec()];
    chunks.extend((0..1000).map(|i| format!("{i},row\n").into_bytes()));
    let (source, polls) = Counting::new(chunks);
    let mut rows = reader.read::<(u32, String), _>(source);
    assert_eq!(polls.load(Ordering::SeqCst), 0, "opening reads nothing");
    for i in 0..3 {
        assert_eq!(rows.next().await.unwrap().unwrap().0, i);
    }
    // One chunk per record (plus the header's).
    assert_eq!(polls.load(Ordering::SeqCst), 4);
    tokio::task::yield_now().await;
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    assert_eq!(
        polls.load(Ordering::SeqCst),
        4,
        "nothing runs while the consumer waits"
    );
    assert_eq!(rows.position().records, 3);
    assert_eq!(rows.pending_input_bytes(), 0);
    // Dropping the reader drops the source.
    drop(rows);
    assert_eq!(std::sync::Arc::strong_count(&polls), 1);
}

#[tokio::test]
async fn a_late_source_error_keeps_the_prefix_and_fuses() {
    let reader = CsvReader::new(CsvOptions::new(HeaderMode::Present, b',')).unwrap();
    let (source, _) = Counting::new([&b"id\n1\n2\n3"[..]]);
    let mut rows = reader.read::<(u32,), _>(source.then_fail("synthetic transport failure"));
    assert_eq!(rows.next().await.unwrap().unwrap(), (1,));
    assert_eq!(rows.next().await.unwrap().unwrap(), (2,));
    // `3` is unfinished when the source fails: not delivered as a record.
    let error = rows.next().await.unwrap().unwrap_err();
    assert_eq!(error.kind(), ReadErrorKind::Transport);
    assert_eq!(error.position().records, 2);
    assert_eq!(
        error.to_string(),
        "reading failed: synthetic transport failure (after 2 records, 8 bytes)"
    );
    assert!(rows.next().await.is_none());
    assert!(rows.next().await.is_none());
}

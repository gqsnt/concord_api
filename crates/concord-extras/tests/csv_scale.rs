//! Generated large tables in both directions (CSV.md §8 item 12; STREAMING.md scale evidence):
//! input produced one chunk at a time by a generator (never collected), typed rows checked by a
//! running checksum, the framing buffers' peak, and a lazy upload body drained frame by frame.
//!
//! `measure_throughput` (ignored by default) reports throughput, first-record latency and the
//! peak retained bytes for N and 2N rows; run it in release:
//! `cargo test --release -p concord-extras --features csv --test csv_scale -- --ignored --nocapture`.

#![cfg(feature = "csv")]

mod csv_support;

use std::pin::Pin;
use std::sync::atomic::Ordering;
use std::task::{Context, Poll, Waker};
use std::time::Instant;

use concord_core::Encode;
use concord_core::bytes::Bytes;
use concord_extras::csv::{Csv, CsvOptions, CsvReader, HeaderMode, StreamRows};
use concord_extras::stream::from_iter;
use csv_support::Generated;
use http_body::Body as _;

#[derive(serde::Deserialize, serde::Serialize)]
struct Row {
    id: u64,
    name: String,
    amount: f64,
    note: String,
}

/// Row `i` as CSV text (with a quoted field every 7th row).
fn line(i: u64) -> String {
    if i.is_multiple_of(7) {
        format!(
            "{i},name-{i},{}.25,\"a, quoted \"\"note\"\"\"\r\n",
            i % 1000
        )
    } else {
        format!("{i},name-{i},{}.5,plain note\r\n", i % 1000)
    }
}

/// A generator of `rows` rows in chunks of about `chunk` bytes.
fn table(rows: u64, chunk: usize) -> impl FnMut(usize) -> Option<Bytes> + Send {
    let mut next = 0u64;
    let mut header = true;
    move |_| {
        let mut out = String::with_capacity(chunk + 128);
        if header {
            out.push_str("id,name,amount,note\r\n");
            header = false;
        }
        while out.len() < chunk && next < rows {
            out.push_str(&line(next));
            next += 1;
        }
        (!out.is_empty()).then(|| Bytes::from(out))
    }
}

struct Measured {
    rows: u64,
    checksum: u64,
    bytes: usize,
    first: std::time::Duration,
    total: std::time::Duration,
    peak: usize,
}

async fn read_generated(rows: u64) -> Measured {
    let reader = CsvReader::new(CsvOptions::new(HeaderMode::Present, b',')).unwrap();
    let (source, produced) = Generated::new(table(rows, 16 * 1024));
    let start = Instant::now();
    let mut records = reader.read::<Row, _>(source);
    let mut first = None;
    let (mut count, mut checksum) = (0u64, 0u64);
    while let Some(row) = records.next().await {
        let row = row.unwrap();
        first.get_or_insert_with(|| start.elapsed());
        assert_eq!(row.id, count);
        checksum = checksum
            .wrapping_mul(31)
            .wrapping_add(row.id ^ row.note.len() as u64);
        count += 1;
    }
    Measured {
        rows: count,
        checksum,
        bytes: produced.load(Ordering::SeqCst),
        first: first.unwrap_or_default(),
        total: start.elapsed(),
        peak: records.peak_retained_bytes(),
    }
}

fn expected_checksum(rows: u64) -> u64 {
    (0..rows).fold(0u64, |acc, i| {
        let note = if i.is_multiple_of(7) {
            "a, quoted \"note\"".len()
        } else {
            "plain note".len()
        };
        acc.wrapping_mul(31).wrapping_add(i ^ note as u64)
    })
}

/// Drains a body frame by frame (no network), returning the bytes and the largest frame.
fn drain(body: concord_core::reqwest::Body) -> (usize, usize) {
    let mut body = body;
    let mut cx = Context::from_waker(Waker::noop());
    let (mut total, mut largest) = (0, 0);
    loop {
        match Pin::new(&mut body).poll_frame(&mut cx) {
            Poll::Ready(Some(Ok(frame))) => {
                let n = frame.into_data().map(|d| d.len()).unwrap_or(0);
                total += n;
                largest = largest.max(n);
            }
            Poll::Ready(Some(Err(e))) => panic!("{e}"),
            Poll::Ready(None) => return (total, largest),
            Poll::Pending => unreachable!("an always-ready source"),
        }
    }
}

fn rows_source(rows: u64) -> impl futures_core::Stream<Item = Result<Row, std::io::Error>> + Send {
    from_iter((0..rows).map(|i| {
        Ok(Row {
            id: i,
            name: format!("name-{i}"),
            amount: (i % 1000) as f64 + 0.5,
            note: "plain note".into(),
        })
    }))
}

#[tokio::test]
async fn a_generated_table_reads_in_bounded_buffers() {
    let rows = 100_000;
    let m = read_generated(rows).await;
    assert_eq!(m.rows, rows);
    assert_eq!(m.checksum, expected_checksum(rows));
    assert!(m.bytes > 3_500_000, "{}", m.bytes); // about 36 bytes per row
    // Records are under 256 bytes: the buffers never grew past their first allocation.
    assert!(m.peak <= 256 + 16 * 8, "{}", m.peak);
}

#[test]
fn a_generated_upload_is_produced_in_bounded_frames() {
    let rows = 100_000;
    let body = <Csv as Encode<StreamRows<_>>>::encode(StreamRows::new(rows_source(rows))).unwrap();
    let (total, largest) = drain(body);
    assert!(total > 3_000_000, "{total}");
    // One frame holds about 16 KiB plus at most one record.
    assert!(largest < 16 * 1024 + 256, "{largest}");
}

#[tokio::test]
#[ignore = "measurement; run in release with --ignored --nocapture"]
async fn measure_throughput() {
    for rows in [1_000_000u64, 2_000_000] {
        let m = read_generated(rows).await;
        assert_eq!(m.checksum, expected_checksum(rows));
        let secs = m.total.as_secs_f64();
        println!(
            "read  N={rows}: {} bytes in {:.3}s = {:.1} MB/s, {:.0} rows/s; first record after {:?}; peak retained {} bytes",
            m.bytes,
            secs,
            m.bytes as f64 / secs / 1e6,
            rows as f64 / secs,
            m.first,
            m.peak
        );
        let start = Instant::now();
        let body =
            <Csv as Encode<StreamRows<_>>>::encode(StreamRows::new(rows_source(rows))).unwrap();
        let (total, largest) = drain(body);
        let secs = start.elapsed().as_secs_f64();
        println!(
            "write N={rows}: {total} bytes in {secs:.3}s = {:.1} MB/s; largest frame {largest} bytes",
            total as f64 / secs / 1e6
        );
    }
}

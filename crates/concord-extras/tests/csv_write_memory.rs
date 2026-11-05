//! ASTRA-2 (4.4 review), as a process observation: refusing a record over the record (or header)
//! limit does not grow the process's peak resident memory with that record. Before the fix the
//! writer serialized the whole record into an unbounded buffer and copied it before comparing it
//! with the limit: about twice the field's size (a 16 MiB field with a 64-byte limit grew the
//! peak by about 32 MiB on both routes).
//!
//! Linux only (`VmHWM` in `/proc/self/status`), and alone in its test binary so that no other
//! test allocates during the measurement. The portable, exact bound (the encoder's buffers never
//! exceed the limit plus the terminator) is a unit test of the encoder.

#![cfg(all(feature = "csv", target_os = "linux"))]

use std::convert::Infallible;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

use concord_extras::csv::{CsvOptions, CsvWriter, HeaderMode, Rows, StreamRows};
use concord_extras::stream::from_iter;
use http_body::Body as _;

const FIELD: usize = 16 * 1024 * 1024;
/// Far below the old growth (about 32 MiB) and far above allocator noise.
const ALLOWED_GROWTH_KIB: u64 = 4 * 1024;

/// The process's peak resident set size, in KiB.
fn peak_kib() -> u64 {
    let status = std::fs::read_to_string("/proc/self/status").unwrap();
    status
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|kib| kib.parse().ok())
        .expect("VmHWM in /proc/self/status")
}

/// Runs `refuse` and returns how much the peak grew (KiB).
fn growth(refuse: impl FnOnce()) -> u64 {
    let before = peak_kib();
    refuse();
    peak_kib().saturating_sub(before)
}

#[test]
fn refusing_an_oversized_record_does_not_grow_the_peak_with_it() {
    let writer =
        CsvWriter::new(CsvOptions::new(HeaderMode::Absent, b',').max_record_bytes(64)).unwrap();

    // Finite route. The input is allocated (and touched) before the baseline.
    let field = "x".repeat(FIELD);
    let finite = growth(|| {
        let error = writer.to_vec(Rows::new([(&field,)])).unwrap_err();
        assert_eq!(error.get("limit"), Some(64));
    });
    drop(field);

    // Lazy route, polled directly (no transport buffering).
    let source = from_iter([Ok::<_, Infallible>(("x".repeat(FIELD),))]);
    let lazy = growth(|| {
        let (mut body, _) = writer.upload(StreamRows::new(source)).unwrap().into_parts();
        let mut cx = Context::from_waker(Waker::noop());
        match Pin::new(&mut body).poll_frame(&mut cx) {
            Poll::Ready(Some(Err(_))) => {}
            other => panic!("expected the record to be refused, got {other:?}"),
        }
    });

    // The header, from explicit columns (the payload, with its long name, built before).
    let writer =
        CsvWriter::new(CsvOptions::new(HeaderMode::Present, b',').max_header_bytes(64)).unwrap();
    let rows = Rows::new([(1,)]).with_columns(["h".repeat(FIELD)]);
    let header = growth(|| {
        let error = writer.to_vec(rows).unwrap_err();
        assert_eq!(error.get("limit"), Some(64));
    });

    println!(
        "field={FIELD} bytes, limit=64 bytes: peak growth finite={finite} KiB, lazy={lazy} KiB, header={header} KiB"
    );
    for (route, kib) in [("finite", finite), ("lazy", lazy), ("header", header)] {
        assert!(
            kib < ALLOWED_GROWTH_KIB,
            "{route}: the peak grew by {kib} KiB refusing a {FIELD}-byte record"
        );
    }
}

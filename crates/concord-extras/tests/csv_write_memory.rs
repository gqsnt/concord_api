//! ASTRA-2 (4.4 review), as a process observation: refusing a record over the record limit, or a
//! header over the header limit, does not grow the process's peak resident memory with it.
//!
//! Before the fix the writer serialized the whole record into an unbounded buffer and copied it
//! before comparing it with the limit (with the fix reverted, this test observed a 16 MiB field
//! with a 64-byte limit grow the peak by about 32 MiB on the record and header routes). After that
//! fix the review's re-check found header names still copied before the header limit: inferred
//! names twice (about 32 MiB) and configured columns once (about 16 MiB), on both routes.
//!
//! Linux only (`VmHWM` in `/proc/self/status`, reset per measurement), and alone in its test
//! binary so that no other test allocates during a measurement. Every input (including the
//! configuration and a leaked field name) is allocated and touched before its baseline. The
//! portable checks (buffer capacities, no kept copies) are unit tests of the encoder.

#![cfg(all(feature = "csv", target_os = "linux"))]

use std::convert::Infallible;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

use concord_extras::csv::{CsvOptions, CsvWriter, HeaderMode, Rows, StreamRows};
use concord_extras::stream::{Upload, UploadError, from_iter};
use http_body::Body as _;
use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

const FIELD: usize = 16 * 1024 * 1024;
/// Far below the old growth (16 to 32 MiB) and far above allocator noise.
const ALLOWED_GROWTH_KIB: u64 = 4 * 1024;
const HEADER_REFUSED: &str = "the CSV header exceeds the configured size";
const RECORD_REFUSED: &str = "a CSV record exceeds the configured size";

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

/// Runs `refuse` and returns how much the peak grew (KiB). The peak is first reset to the current
/// resident size (`5` to `/proc/self/clear_refs`, this process only), so that an earlier
/// measurement's high-water mark cannot hide this one's growth.
fn growth(refuse: impl FnOnce()) -> u64 {
    std::fs::write("/proc/self/clear_refs", "5").expect("reset this process's VmHWM");
    let before = peak_kib();
    refuse();
    peak_kib().saturating_sub(before)
}

/// Polls a lazy body once, directly (no transport buffering): the refusal's label and `limit`.
fn refused_lazily(upload: Upload) -> (&'static str, Option<u64>) {
    let (mut body, _) = upload.into_parts();
    let mut cx = Context::from_waker(Waker::noop());
    let error = match Pin::new(&mut body).poll_frame(&mut cx) {
        Poll::Ready(Some(Err(error))) => error,
        other => panic!("expected a refusal, got {other:?}"),
    };
    let mut current: &(dyn std::error::Error + 'static) = &error;
    loop {
        if let Some(upload) = current.downcast_ref::<UploadError>() {
            let codec = upload.codec().expect("a codec refusal");
            return (codec.label(), codec.get("limit"));
        }
        current = current.source().expect("an UploadError in the chain");
    }
}

/// A struct row with one field, named at run time.
struct Named(&'static str);

impl Serialize for Named {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut row = serializer.serialize_struct("Named", 1)?;
        row.serialize_field(self.0, &1)?;
        row.end()
    }
}

fn headed() -> CsvOptions {
    CsvOptions::new(HeaderMode::Present, b',').max_header_bytes(64)
}

#[test]
fn refusing_an_oversized_record_or_header_does_not_grow_the_peak_with_it() {
    let mut routes: Vec<(&str, u64)> = Vec::new();
    let records =
        CsvWriter::new(CsvOptions::new(HeaderMode::Absent, b',').max_record_bytes(64)).unwrap();

    // A data field over the record limit, finite and lazy.
    let field = "x".repeat(FIELD);
    routes.push((
        "record, finite",
        growth(|| {
            let error = records.to_vec(Rows::new([(&field,)])).unwrap_err();
            assert_eq!(
                (error.label(), error.get("limit")),
                (RECORD_REFUSED, Some(64))
            );
        }),
    ));
    drop(field);
    let source = from_iter([Ok::<_, Infallible>(("x".repeat(FIELD),))]);
    routes.push((
        "record, lazy",
        growth(|| {
            let upload = records.upload(StreamRows::new(source)).unwrap();
            assert_eq!(refused_lazily(upload), (RECORD_REFUSED, Some(64)));
        }),
    ));

    // A header name over the header limit: from the payload's columns, ...
    let writer = CsvWriter::new(headed()).unwrap();
    let rows = Rows::new([(1,)]).with_columns(["h".repeat(FIELD)]);
    routes.push((
        "payload columns, finite",
        growth(|| {
            let error = writer.to_vec(rows).unwrap_err();
            assert_eq!(
                (error.label(), error.get("limit")),
                (HEADER_REFUSED, Some(64))
            );
        }),
    ));

    // ... inferred from the row's field name, ...
    let name: &'static str = Box::leak("h".repeat(FIELD).into_boxed_str());
    routes.push((
        "inferred names, finite",
        growth(|| {
            let error = writer.to_vec(Rows::new([Named(name)])).unwrap_err();
            assert_eq!(
                (error.label(), error.get("limit")),
                (HEADER_REFUSED, Some(64))
            );
        }),
    ));
    let source = from_iter([Ok::<_, Infallible>(Named(name))]);
    routes.push((
        "inferred names, lazy",
        growth(|| {
            let upload = writer.upload(StreamRows::new(source)).unwrap();
            assert_eq!(refused_lazily(upload), (HEADER_REFUSED, Some(64)));
        }),
    ));

    // ... and from the configured columns (the writer, with its configuration, built before).
    let configured = CsvWriter::new(headed().columns([name])).unwrap();
    routes.push((
        "configured columns, finite",
        growth(|| {
            let error = configured.to_vec(Rows::new([(1,)])).unwrap_err();
            assert_eq!(
                (error.label(), error.get("limit")),
                (HEADER_REFUSED, Some(64))
            );
        }),
    ));
    let source = from_iter([Ok::<_, Infallible>((1,))]);
    routes.push((
        "configured columns, lazy",
        growth(|| {
            let upload = configured.upload(StreamRows::new(source)).unwrap();
            assert_eq!(refused_lazily(upload), (HEADER_REFUSED, Some(64)));
        }),
    ));

    for (route, kib) in &routes {
        println!("field or name of {FIELD} bytes, limit 64 bytes: {route}: peak growth {kib} KiB");
    }
    for (route, kib) in routes {
        assert!(
            kib < ALLOWED_GROWTH_KIB,
            "{route}: the peak grew by {kib} KiB refusing a {FIELD}-byte value"
        );
    }
}

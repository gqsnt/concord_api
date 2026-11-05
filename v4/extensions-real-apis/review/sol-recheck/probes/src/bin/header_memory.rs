#[cfg(feature = "before")]
use concord_extras_before as extras;
#[cfg(not(feature = "before"))]
use concord_extras as extras;
use std::convert::Infallible;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

use extras::csv::{CsvOptions, CsvWriter, HeaderMode, Rows, StreamRows};
use extras::stream::{Upload, UploadError, from_iter};
use http_body::Body as _;
use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

fn field() -> usize { std::env::args().nth(2).unwrap().parse::<usize>().unwrap() * 1024 * 1024 }
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

/// One route's growth: its input (and writer) prepared first, then the refusal measured.
fn measure(route: &str) -> u64 {
    let records =
        CsvWriter::new(CsvOptions::new(HeaderMode::Absent, b',').max_record_bytes(64)).unwrap();
    let writer = CsvWriter::new(headed()).unwrap();
    let record_refused = |error: concord_core::CodecError| {
        assert_eq!(
            (error.label(), error.get("limit")),
            (RECORD_REFUSED, Some(64))
        );
    };
    let header_refused = |error: concord_core::CodecError| {
        assert_eq!(
            (error.label(), error.get("limit")),
            (HEADER_REFUSED, Some(64))
        );
    };
    match route {
        // A data field over the record limit.
        "record, finite" => {
            let field = "x".repeat(field());
            growth(|| record_refused(records.to_vec(Rows::new([(&field,)])).unwrap_err()))
        }
        "record, lazy" => {
            let source = from_iter([Ok::<_, Infallible>(("x".repeat(field()),))]);
            growth(|| {
                let upload = records.upload(StreamRows::new(source)).unwrap();
                assert_eq!(refused_lazily(upload), (RECORD_REFUSED, Some(64)));
            })
        }
        // A header name over the header limit: from the payload's columns, ...
        "payload columns, finite" => {
            let rows = Rows::new([(1,)]).with_columns(["h".repeat(field())]);
            growth(|| header_refused(writer.to_vec(rows).unwrap_err()))
        }
        "payload columns, lazy" => {
            let source = from_iter([Ok::<_, Infallible>((1,))]);
            let rows = StreamRows::new(source).with_columns(["h".repeat(field())]);
            growth(|| {
                let upload = writer.upload(rows).unwrap();
                assert_eq!(refused_lazily(upload), (HEADER_REFUSED, Some(64)));
            })
        }
        // ... inferred from the row's field name, ...
        "inferred names, finite" => {
            let name: &'static str = Box::leak("h".repeat(field()).into_boxed_str());
            growth(|| header_refused(writer.to_vec(Rows::new([Named(name)])).unwrap_err()))
        }
        "inferred names, lazy" => {
            let name: &'static str = Box::leak("h".repeat(field()).into_boxed_str());
            let source = from_iter([Ok::<_, Infallible>(Named(name))]);
            growth(|| {
                let upload = writer.upload(StreamRows::new(source)).unwrap();
                assert_eq!(refused_lazily(upload), (HEADER_REFUSED, Some(64)));
            })
        }
        // ... and from the configured columns (the writer, with its configuration, built first).
        "configured columns, finite" => {
            let configured = CsvWriter::new(headed().columns(["h".repeat(field())])).unwrap();
            growth(|| header_refused(configured.to_vec(Rows::new([(1,)])).unwrap_err()))
        }
        "configured columns, lazy" => {
            let configured = CsvWriter::new(headed().columns(["h".repeat(field())])).unwrap();
            let source = from_iter([Ok::<_, Infallible>((1,))]);
            growth(|| {
                let upload = configured.upload(StreamRows::new(source)).unwrap();
                assert_eq!(refused_lazily(upload), (HEADER_REFUSED, Some(64)));
            })
        }
        other => panic!("unknown route {other}"),
    }
}

fn main() { let route = std::env::args().nth(1).unwrap(); let kib = measure(&route); println!("{route}; size={} MiB; growth={kib} KiB", field()/1024/1024); }

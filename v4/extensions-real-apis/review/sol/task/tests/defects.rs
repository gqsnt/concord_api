//! Assertions of intended behavior. These tests deliberately fail on the reviewed checkout.
use concord::{
    bytes::Bytes,
    csv::{CsvOptions, CsvWriter, HeaderMode, Rows},
    jsonl::{JsonLinesOptions, JsonLinesReader, JsonValuesReader, RecordErrors},
    stream::{ByteSource, Chunks, SourceError},
};
use concord_pagination::{
    Budget, JobOptions, JobsError, Manifest, OnError, Options, Outcome, Resume, manifest_jobs,
};
use serde::{
    Serialize,
    ser::{SerializeStruct, Serializer},
};
use std::{
    cell::Cell,
    convert::Infallible,
    future::{Future, pending},
    num::NonZeroUsize,
    pin::pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Wake, Waker},
};

#[test]
#[ignore = "expected invariant failure on reviewed commit; run explicitly with --ignored"]
fn chunks_debug_does_not_print_body() {
    let source = Chunks::new(["TOP_SECRET_BODY"]);
    let shown = format!("{source:?}");
    println!("observed Debug: {shown}");
    assert!(!shown.contains("TOP_SECRET_BODY"));
}

struct ChangingRow(Cell<usize>);
impl Serialize for ChangingRow {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let call = self.0.get();
        self.0.set(call + 1);
        let mut row = serializer.serialize_struct("Row", 1)?;
        row.serialize_field(if call % 2 == 0 { "id" } else { "other" }, &7)?;
        row.end()
    }
}
#[test]
#[ignore = "expected invariant failure on reviewed commit; run explicitly with --ignored"]
fn csv_does_not_emit_a_header_for_different_fields() {
    let writer = CsvWriter::new(CsvOptions::new(HeaderMode::Present, b',')).unwrap();
    let row = ChangingRow(Cell::new(0));
    let result = writer.to_vec(Rows::new([&row]));
    println!("Serialize calls={}, result={result:?}", row.0.get());
    assert!(
        result.is_err() || row.0.get() == 1,
        "expected one consistent serialization, or refusal of the second pass whose field is 'other'"
    );
}

struct EmptySource {
    calls: Arc<AtomicUsize>,
}
impl ByteSource for EmptySource {
    async fn next_chunk(&mut self) -> Result<Option<Bytes>, SourceError> {
        let n = self.calls.fetch_add(1, Ordering::Relaxed);
        Ok(Some(if n < 100_000 {
            Bytes::new()
        } else {
            Bytes::from_static(b"1\n")
        }))
    }
}
struct Noop;
impl Wake for Noop {
    fn wake(self: Arc<Self>) {}
}
#[test]
#[ignore = "expected invariant failure on reviewed commit; run explicitly with --ignored"]
fn empty_chunks_yield_with_bounded_source_polls() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut rows =
        JsonLinesReader::new(JsonLinesOptions::json_lines()).read::<u32, _>(EmptySource {
            calls: calls.clone(),
        });
    let mut next = pin!(rows.next());
    let waker = Waker::from(Arc::new(Noop));
    let result = next.as_mut().poll(&mut Context::from_waker(&waker));
    println!(
        "single poll={result:?}, source calls={}",
        calls.load(Ordering::Relaxed)
    );
    assert!(
        result.is_pending(),
        "a poll consumed 100,000 empty source chunks without yielding"
    );
}

#[tokio::test]
#[ignore = "expected invariant failure on reviewed commit; run explicitly with --ignored"]
async fn final_delimited_by_eof_bad_line_matches_yield_docs() {
    let mut rows =
        JsonLinesReader::new(JsonLinesOptions::json_lines().record_errors(RecordErrors::Yield))
            .read::<u32, _>(Chunks::new(["1\nnot-json"]));
    assert_eq!(rows.next().await.unwrap().unwrap(), 1);
    let error = rows.next().await.unwrap().unwrap_err();
    println!("EOF line error terminal={}: {error}", error.is_terminal());
    assert!(
        !error.is_terminal(),
        "policy rustdoc says true EOF delimits an isolated bad line"
    );
}

#[tokio::test]
#[ignore = "expected invariant failure on reviewed commit; run explicitly with --ignored"]
async fn finish_preserves_discovery_failure_outcome() {
    let n = |n| NonZeroUsize::new(n).unwrap();
    let options = JobOptions::new(
        Budget::new(n(10)),
        Options::new(Budget::new(n(10))).on_error(OnError::Continue),
    );
    let mut jobs = manifest_jobs(
        Resume::Start,
        options,
        |key: Option<u32>| async move {
            if key.is_none() {
                Ok(Manifest::new(vec![1u32], Some(1)))
            } else {
                Err("manifest failed")
            }
        },
        |_| pending::<Result<(), Infallible>>(),
    );
    assert!(matches!(
        jobs.next().await,
        Some(Err(JobsError::Discovery("manifest failed")))
    ));
    let report = jobs.finish();
    println!(
        "finish outcome={:?}, discovery.failed={:?}, cancelled={}",
        report.outcome, report.discovery.failed, report.progress.jobs.cancelled
    );
    assert_eq!(report.outcome, Outcome::DiscoveryFailed);
}

#[tokio::test]
async fn counter_example_values_across_every_split() {
    let input = "12 {\"x\":\"é\\\"y\"}[true,false] null\"last\"";
    let expected = vec![
        serde_json::json!(12),
        serde_json::json!({"x":"é\"y"}),
        serde_json::json!([true, false]),
        serde_json::Value::Null,
        serde_json::json!("last"),
    ];
    for split in 0..=input.len() {
        let mut values = JsonValuesReader::new().read::<serde_json::Value, _>(Chunks::new([
            input.as_bytes()[..split].to_vec(),
            input.as_bytes()[split..].to_vec(),
        ]));
        let mut got = Vec::new();
        while let Some(value) = values.next().await {
            got.push(value.unwrap());
        }
        assert_eq!(got, expected, "split {split}");
    }
}

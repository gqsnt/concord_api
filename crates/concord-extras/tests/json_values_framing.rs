//! Consecutive JSON values without a network: the handoff seeds, primitive-token boundaries
//! against chunk boundaries and the true end of input, multiline values, every single and
//! double split of small inputs, truncation, limits and cooperative yielding.
#![cfg(feature = "jsonl")]

mod support;

use std::sync::atomic::Ordering;

use concord_extras::jsonl::JsonValuesReader;
use concord_extras::stream::{Chunks, ReadErrorKind};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use support::{Counting, double_splits, partitions, poll_counting};

const ENDS_INSIDE: &str = "the input ends inside a JSON value";
const INCOMPLETE: &str = "a JSON value is incomplete";
const NOT_JSON: &str = "a JSON value is not valid JSON";
const WRONG_TYPE: &str = "a JSON value does not match the expected type";
const LIMIT: &str = "a JSON value exceeds max_value_bytes";

type Item<T> = Result<T, (ReadErrorKind, &'static str)>;

fn ok(v: Value) -> Item<Value> {
    Ok(v)
}

fn err<T>(kind: ReadErrorKind, label: &'static str) -> Item<T> {
    Err((kind, label))
}

async fn read_all<T: DeserializeOwned>(
    reader: &JsonValuesReader,
    chunks: Vec<Vec<u8>>,
) -> Vec<Item<T>> {
    let mut values = reader.read::<T, _>(Chunks::new(chunks));
    let mut out = Vec::new();
    while let Some(item) = values.next().await {
        out.push(item.map_err(|e| (e.kind(), e.codec().expect("a codec description").label())));
        assert!(out.len() < 100_000, "runaway reader");
    }
    assert!(values.next().await.is_none(), "fused");
    out
}

async fn every_partition(input: &[u8], expected: &[Item<Value>]) {
    let reader = JsonValuesReader::new();
    for parts in partitions(input) {
        assert_eq!(
            read_all::<Value>(&reader, parts.clone()).await,
            expected,
            "partition {parts:?}"
        );
    }
}

async fn every_double_split(input: &[u8], expected: &[Item<Value>]) {
    let reader = JsonValuesReader::new();
    for parts in double_splits(input) {
        assert_eq!(
            read_all::<Value>(&reader, parts.clone()).await,
            expected,
            "split {parts:?}"
        );
    }
}

const MULTILINE: &[u8] = include_bytes!("data/json_values/multiline.json");
const SCALARS: &[u8] = include_bytes!("data/json_values/scalars.json");
const TRUNCATED: &[u8] = include_bytes!("data/json_values/truncated.json");

/// V01–V03 of the handoff seeds (`tests/data/json_values/PROVENANCE.md`), under every single
/// split, every pair of split points and representative multi-split partitions.
#[tokio::test]
async fn seeds_match_documented_expectations() {
    assert_eq!(
        MULTILINE,
        b"{\n  \"type\": \"ADDED\",\n  \"object\": {\"id\": \"x\"}\n}\n{ \"type\": \"DELETED\", \"object\": {\"id\": \"x\"} }"
    );
    assert_eq!(SCALARS, "12 true \"東京\" [1,2] null {\"k\":3}".as_bytes());
    assert_eq!(TRUNCATED, b"12 {\"id\":");

    let v01 = [
        ok(json!({"type": "ADDED", "object": {"id": "x"}})),
        ok(json!({"type": "DELETED", "object": {"id": "x"}})),
    ];
    every_partition(MULTILINE, &v01).await;
    every_double_split(MULTILINE, &v01).await;

    let v02 = [
        ok(json!(12)),
        ok(json!(true)),
        ok(json!("東京")),
        ok(json!([1, 2])),
        ok(Value::Null),
        ok(json!({"k": 3})),
    ];
    every_partition(SCALARS, &v02).await;
    every_double_split(SCALARS, &v02).await;

    let v03 = [ok(json!(12)), err(ReadErrorKind::Format, ENDS_INSIDE)];
    every_partition(TRUNCATED, &v03).await;
    every_double_split(TRUNCATED, &v03).await;
}

/// A number or literal ends only at whitespace, at the start of a self-delimiting value or at
/// the true end of input: chunks `1` and `2 ` are `12`, and a chunk ending in a token does not
/// end it.
#[tokio::test]
async fn primitive_tokens_span_chunks_until_a_delimiter_or_true_eof() {
    let reader = JsonValuesReader::new();
    assert_eq!(
        read_all::<Value>(&reader, vec![b"1".to_vec(), b"2 ".to_vec()]).await,
        [ok(json!(12))]
    );
    assert_eq!(
        read_all::<Value>(&reader, vec![b"1".to_vec(), b"2".to_vec()]).await,
        [ok(json!(12))]
    );
    assert_eq!(
        read_all::<Value>(
            &reader,
            vec![
                b"tr".to_vec(),
                Vec::new(),
                b"ue".to_vec(),
                b" fal".to_vec(),
                b"se".to_vec()
            ]
        )
        .await,
        [ok(json!(true)), ok(json!(false))]
    );
    assert_eq!(
        read_all::<Value>(
            &reader,
            vec![b"-1".to_vec(), b".5e".to_vec(), b"3\n".to_vec()]
        )
        .await,
        [ok(json!(-1500.0))]
    );
    // Self-delimiting values need no whitespace; a primitive ends where one starts.
    every_partition(
        b"12{\"a\":1}[2]\"s\"null\"t\"true{}",
        &[
            ok(json!(12)),
            ok(json!({"a": 1})),
            ok(json!([2])),
            ok(json!("s")),
            ok(Value::Null),
            ok(json!("t")),
            ok(json!(true)),
            ok(json!({})),
        ],
    )
    .await;
}

/// `}{` and `][` boundaries, braces and quotes inside strings, escapes (including an escaped
/// backslash before the closing quote) and multibyte characters.
#[tokio::test]
async fn structure_inside_strings_does_not_end_values() {
    let input = r#"{"a":"}{"}{"b":"\"]["}["\\",{"c":"東\n"}]"\\""x\"y""#.as_bytes();
    let expected = [
        ok(json!({"a": "}{"})),
        ok(json!({"b": "\"]["})),
        ok(json!(["\\", {"c": "東\n"}])),
        ok(json!("\\")),
        ok(json!("x\"y")),
    ];
    every_partition(input, &expected).await;
    every_double_split(input, &expected).await;
}

/// A top-level array is one value, whatever its elements.
#[tokio::test]
async fn a_top_level_array_is_one_value() {
    every_partition(
        b" [ {\"a\":1},\n {\"b\":2} ]\n[]",
        &[ok(json!([{"a": 1}, {"b": 2}])), ok(json!([]))],
    )
    .await;
}

/// Input that ends inside an object, array, string or escape is a `Format` error after the
/// values before it; a literal cut at the true end is incomplete JSON; whitespace alone is an
/// empty stream.
#[tokio::test]
async fn truncation_and_end_of_input() {
    use ReadErrorKind::Format;
    for input in [
        &b"1 {\"a\":[1,"[..],
        b"1 [",
        b"1 \"abc",
        b"1 \"ab\\",
        b"1 {\"a\":\"}",
    ] {
        every_partition(input, &[ok(json!(1)), err(Format, ENDS_INSIDE)]).await;
    }
    every_partition(b"1 tru", &[ok(json!(1)), err(Format, INCOMPLETE)]).await;
    every_partition(b"1 -", &[ok(json!(1)), err(Format, INCOMPLETE)]).await;
    every_partition(b" \n\t\r\n ", &[]).await;
    every_partition(b"", &[]).await;
}

/// What is not a stream of JSON values is a terminal `Format` error when the value it belongs
/// to is parsed: separators, a byte order mark, two literals without whitespace.
#[tokio::test]
async fn non_json_is_a_format_error() {
    use ReadErrorKind::Format;
    every_partition(b"1 2,3 4", &[ok(json!(1)), err(Format, NOT_JSON)]).await;
    every_partition(b"\xef\xbb\xbf{\"a\":1}", &[err(Format, NOT_JSON)]).await;
    every_partition(b"truefalse 1", &[err(Format, NOT_JSON)]).await;
    every_partition(
        b"{\"a\":1} } 2",
        &[ok(json!({"a": 1})), err(Format, NOT_JSON)],
    )
    .await;
    every_partition(
        b"\"\xff\" 1",
        &[err(Format, "a JSON value is not valid UTF-8")],
    )
    .await;
}

#[derive(Debug, PartialEq, serde::Deserialize)]
struct Event {
    #[serde(rename = "type")]
    kind: String,
}

/// A value of the wrong shape is a terminal `Decode` error.
#[tokio::test]
async fn wrong_shape_is_a_decode_error() {
    let reader = JsonValuesReader::new();
    assert_eq!(
        read_all::<Event>(
            &reader,
            vec![b"{\"type\":\"ADDED\"}{\"type\":7}{\"type\":\"X\"}".to_vec()]
        )
        .await,
        [
            Ok(Event {
                kind: "ADDED".into()
            }),
            err(ReadErrorKind::Decode, WRONG_TYPE)
        ]
    );
}

/// A value longer than `max_value_bytes` is a terminal `Limit` error found before buffering past
/// it: with 1-byte chunks the source is not polled beyond the limit.
#[tokio::test]
async fn oversized_value_is_a_limit_before_growth() {
    let max = 64;
    let mut input = b"{\"a\":1}\n".to_vec();
    input.extend_from_slice(format!("{{\"big\":\"{}\"}}\n", "x".repeat(200)).as_bytes());
    input.extend(b"{\"a\":2}\n".repeat(500));
    let (source, polls) = Counting::new(input.iter().map(|b| vec![*b]));
    let mut values = JsonValuesReader::new()
        .max_value_bytes(max)
        .read::<Value, _>(source);
    assert_eq!(values.next().await.unwrap().unwrap(), json!({"a": 1}));
    let error = values.next().await.unwrap().unwrap_err();
    assert_eq!(error.kind(), ReadErrorKind::Limit);
    assert_eq!(error.codec().unwrap().label(), LIMIT);
    assert_eq!(error.codec().unwrap().get("limit"), Some(max as u64));
    assert!(values.next().await.is_none());
    let polled = polls.load(Ordering::SeqCst);
    assert!(
        polled <= 8 + max + 2,
        "polled {polled} of {} chunks",
        input.len()
    );

    // Chunk-independent outcome; exactly `max` bytes pass (a number too).
    let reader = JsonValuesReader::new().max_value_bytes(4);
    for parts in partitions(b"1234 [12] 12345") {
        assert_eq!(
            read_all::<Value>(&reader, parts.clone()).await,
            [
                ok(json!(1234)),
                ok(json!([12])),
                err(ReadErrorKind::Limit, LIMIT)
            ],
            "{parts:?}"
        );
    }
}

/// Optional totals are `Limit` errors, never a clean end.
#[tokio::test]
async fn totals_are_limit_errors() {
    let reader = JsonValuesReader::new().max_values(2);
    assert_eq!(
        read_all::<Value>(&reader, vec![b"1 2".to_vec()]).await,
        [ok(json!(1)), ok(json!(2))]
    );
    assert_eq!(
        read_all::<Value>(&reader, vec![b"1 2 3".to_vec()]).await,
        [
            ok(json!(1)),
            ok(json!(2)),
            err(
                ReadErrorKind::Limit,
                "the input exceeds the configured total records"
            )
        ]
    );
    let reader = JsonValuesReader::new().max_total_bytes(3);
    assert_eq!(
        read_all::<Value>(&reader, vec![b"1 ".to_vec(), b"2 ".to_vec()]).await,
        [
            ok(json!(1)),
            err(
                ReadErrorKind::Limit,
                "the input exceeds the configured total bytes"
            )
        ]
    );
}

/// A long whitespace run between values (1 MiB here, from an always-ready source) is skipped in
/// bounded steps: the reader yields to the executor about once per 64 KiB.
#[test]
fn whitespace_runs_yield_cooperatively() {
    let mut input = vec![b' '; 1 << 20];
    input.extend_from_slice(b"{\"a\":1}");
    // One large chunk: the budget applies inside a chunk too.
    let mut values = JsonValuesReader::new().read::<Value, _>(Chunks::new([input]));
    let (first, pendings) = poll_counting(values.next());
    assert_eq!(first.unwrap().unwrap(), json!({"a": 1}));
    assert!(pendings >= 15, "yielded {pendings} times");
}

/// Values and position: the position counts delivered values and consumed bytes.
#[tokio::test]
async fn position_counts_values_and_bytes() {
    let mut values =
        JsonValuesReader::new().read::<Value, _>(Chunks::new([&b"{\"a\":1}  [2]"[..]]));
    assert_eq!(values.next().await.unwrap().unwrap(), json!({"a": 1}));
    assert_eq!((values.position().records, values.position().bytes), (1, 7));
    assert_eq!(values.next().await.unwrap().unwrap(), json!([2]));
    assert_eq!(
        (values.position().records, values.position().bytes),
        (2, 12)
    );
    assert!(values.next().await.is_none());
}

#[test]
fn values_readers_and_futures_are_send() {
    fn send<T: Send>(_: &T) {}
    fn send_sync<T: Send + Sync + Clone>() {}
    send_sync::<JsonValuesReader>();
    fn future(v: &mut concord_extras::jsonl::JsonValues<Value>) {
        send(&v.next());
    }
    let _ = future;
}

//! JSON Lines / NDJSON framing without a network: the handoff seeds, every value kind, every
//! single split and representative multi-split partitions of small canonical inputs, blank/BOM/
//! final-line policies, record-error modes, limits, finite/streamed equivalence and encoding.
#![cfg(feature = "jsonl")]

mod support;

use std::collections::BTreeMap;
use std::fmt::Debug;
use std::sync::atomic::Ordering;

use concord_core::bytes::Bytes;
use concord_core::reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderValue};
use concord_core::reqwest::{StatusCode, Version};
use concord_core::{Decode, Encode, ResponseHead};
use concord_extras::IgnoreContentType;
use concord_extras::jsonl::{
    BlankLines, BomPolicy, JsonLines, JsonLinesOptions, JsonLinesReader, JsonLinesWriter, Ndjson,
    RecordErrors, StreamRecords,
};
use concord_extras::stream::{Chunks, ReadErrorKind, from_iter};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use support::{Counting, partitions, poll_counting};

const BLANK: &str = "a line is blank, which this profile refuses";
const BOM: &str = "the input starts with a byte order mark, which this profile refuses";
const ENDS_INSIDE: &str = "a line ends inside a JSON value";
const NOT_JSON: &str = "a line is not one valid JSON value";
const NOT_UTF8: &str = "a line is not valid UTF-8";
const WRONG_TYPE: &str = "a record does not match the expected type";
const LIMIT: &str = "a line exceeds max_record_bytes";

/// One item from `next()`: a record, or an error's kind, label and `line`.
type Item<T> = Result<T, (ReadErrorKind, &'static str, Option<u64>)>;

fn ok(v: Value) -> Item<Value> {
    Ok(v)
}

fn err<T>(kind: ReadErrorKind, label: &'static str, line: u64) -> Item<T> {
    Err((kind, label, Some(line)))
}

/// Reads all items from `chunks`, then checks the reader stays fused.
async fn read_all<T: DeserializeOwned>(
    options: &JsonLinesOptions,
    chunks: Vec<Vec<u8>>,
) -> Vec<Item<T>> {
    let mut records = JsonLinesReader::new(options.clone()).read::<T, _>(Chunks::new(chunks));
    let mut out = Vec::new();
    while let Some(item) = records.next().await {
        out.push(item.map_err(|e| {
            let codec = e.codec().expect("a codec description");
            (e.kind(), codec.label(), codec.get("line"))
        }));
        assert!(out.len() < 100_000, "runaway reader");
    }
    assert!(records.next().await.is_none(), "fused after the end");
    assert!(records.next().await.is_none(), "still fused");
    out
}

/// The streamed result is the same for every partition of `input` and equals `expected`.
async fn every_partition<T: DeserializeOwned + PartialEq + Debug>(
    options: &JsonLinesOptions,
    input: &[u8],
    expected: &[Item<T>],
) {
    for parts in partitions(input) {
        let got = read_all::<T>(options, parts.clone()).await;
        assert_eq!(got, expected, "partition {parts:?}");
    }
}

/// The finite result for `F`, in the streamed shape (records, then at most one error).
fn finite<F: Decode<Vec<Value>, Context = ()>>(input: &[u8]) -> Vec<Item<Value>> {
    match F::decode_body((), Bytes::copy_from_slice(input)) {
        Ok(records) => records.into_iter().map(Ok).collect(),
        Err(e) => vec![Err((
            ReadErrorKind::Format, // the kind is not part of a CodecError; compared by label
            e.label(),
            e.get("line"),
        ))],
    }
}

/// The finite result equals the streamed one: same records; the streamed prefix before an error
/// is not returned by the finite path, which reports the same label and line instead.
fn assert_finite_agrees(finite: Vec<Item<Value>>, streamed: &[Item<Value>]) {
    match streamed.last() {
        Some(Err((_, label, line))) => {
            assert_eq!(finite.len(), 1, "{finite:?}");
            let Err((_, f_label, f_line)) = &finite[0] else {
                panic!("finite succeeded where streaming failed: {finite:?}");
            };
            assert_eq!((f_label, f_line), (label, line));
        }
        _ => assert_eq!(finite, streamed),
    }
}

async fn check_both(
    options: &JsonLinesOptions,
    finite_result: Vec<Item<Value>>,
    input: &[u8],
    expected: &[Item<Value>],
) {
    every_partition(options, input, expected).await;
    assert_finite_agrees(finite_result, expected);
}

const VALUES: &[u8] = include_bytes!("data/jsonl/values.jsonl");
const NO_FINAL_LF: &[u8] = include_bytes!("data/jsonl/no_final_lf.jsonl");
const BLANK_LINE: &[u8] = include_bytes!("data/jsonl/blank_line.jsonl");
const BLANK_LINE_LENIENT: &[u8] = include_bytes!("data/jsonl/blank_line_lenient.ndjson");
const TRUNCATED: &[u8] = include_bytes!("data/jsonl/truncated.jsonl");
const WITH_BOM: &[u8] = include_bytes!("data/jsonl/bom.jsonl");
const EMPTY: &[u8] = include_bytes!("data/jsonl/empty.jsonl");

/// J01–J07 of the handoff seeds (`tests/data/jsonl/PROVENANCE.md`).
#[tokio::test]
async fn seeds_match_documented_expectations() {
    // The bytes are exactly the seeds' (a line-ending conversion would change them).
    assert_eq!(
        VALUES,
        b"{\"name\":\"Zo\xc3\xab\"}\r\nnull\n[1,2]\n\"a\\nb\"\n12\ntrue\n"
    );
    assert_eq!(NO_FINAL_LF, b"{\"id\":1}\n{\"id\":2}");
    assert_eq!(BLANK_LINE, b"{\"id\":1}\n\n{\"id\":2}\n");
    assert_eq!(BLANK_LINE_LENIENT, BLANK_LINE);
    assert_eq!(TRUNCATED, b"{\"id\":1}\n{\"id\":");
    assert_eq!(WITH_BOM, b"\xef\xbb\xbf{\"id\":1}\n");
    assert_eq!(EMPTY, b"");

    let strict = JsonLinesOptions::json_lines();
    let lenient = JsonLinesOptions::ndjson();
    use ReadErrorKind::Format;

    // J01: every value kind, the first line CRLF-terminated.
    let j01 = [
        ok(json!({"name": "Zoë"})),
        ok(Value::Null),
        ok(json!([1, 2])),
        ok(json!("a\nb")),
        ok(json!(12)),
        ok(json!(true)),
    ];
    check_both(&strict, finite::<JsonLines>(VALUES), VALUES, &j01).await;
    check_both(&lenient, finite::<Ndjson>(VALUES), VALUES, &j01).await;

    // J02: a final line without LF at the true end of input.
    let j02 = [ok(json!({"id": 1})), ok(json!({"id": 2}))];
    check_both(&strict, finite::<JsonLines>(NO_FINAL_LF), NO_FINAL_LF, &j02).await;
    check_both(&lenient, finite::<Ndjson>(NO_FINAL_LF), NO_FINAL_LF, &j02).await;

    // J03: strict JSON Lines refuses the blank line (`blank_record`) after the first record.
    let j03 = [ok(json!({"id": 1})), err(Format, BLANK, 2)];
    check_both(&strict, finite::<JsonLines>(BLANK_LINE), BLANK_LINE, &j03).await;

    // J04: the NDJSON leniency (`ignore_blank`) skips it.
    let j04 = [ok(json!({"id": 1})), ok(json!({"id": 2}))];
    check_both(
        &lenient,
        finite::<Ndjson>(BLANK_LINE_LENIENT),
        BLANK_LINE_LENIENT,
        &j04,
    )
    .await;
    let ignoring = JsonLinesOptions::json_lines().blank_lines(BlankLines::Ignore);
    every_partition(&ignoring, BLANK_LINE_LENIENT, &j04).await;

    // J05: input ending inside the second record (`invalid_json_record`).
    let j05 = [ok(json!({"id": 1})), err(Format, ENDS_INSIDE, 2)];
    check_both(&strict, finite::<JsonLines>(TRUNCATED), TRUNCATED, &j05).await;
    check_both(&lenient, finite::<Ndjson>(TRUNCATED), TRUNCATED, &j05).await;

    // J06: a byte order mark is refused by both presets (`bom_rejected`), unless skipped.
    let j06 = [err(Format, BOM, 1)];
    check_both(&strict, finite::<JsonLines>(WITH_BOM), WITH_BOM, &j06).await;
    check_both(&lenient, finite::<Ndjson>(WITH_BOM), WITH_BOM, &j06).await;
    let skipping = JsonLinesOptions::json_lines().bom(BomPolicy::Skip);
    every_partition(&skipping, WITH_BOM, &[ok(json!({"id": 1}))]).await;

    // J07: an empty body has no records.
    check_both(&strict, finite::<JsonLines>(EMPTY), EMPTY, &[]).await;
    check_both(&lenient, finite::<Ndjson>(EMPTY), EMPTY, &[]).await;
}

/// Objects, arrays, strings (with escapes and a surrogate pair), numbers, booleans and null are
/// all records, under every partition and in both presets.
#[tokio::test]
async fn every_json_value_kind_is_a_record() {
    let input = concat!(
        "{\"a\":[1,{\"b\":null}],\"c\":\"\\u00e9\"}\n",
        "[]\n",
        "[1,\"two\",3.5,false]\n",
        "\"q\\\"uote \\\\ \\ud83d\\ude00\"\n",
        "-1.5e3\n",
        "0\n",
        "true\n",
        "false\n",
        "null\n",
        "{}\n",
    )
    .as_bytes();
    let expected = [
        ok(json!({"a": [1, {"b": null}], "c": "é"})),
        ok(json!([])),
        ok(json!([1, "two", 3.5, false])),
        ok(json!("q\"uote \\ 😀")),
        ok(json!(-1500.0)),
        ok(json!(0)),
        ok(json!(true)),
        ok(json!(false)),
        ok(Value::Null),
        ok(json!({})),
    ];
    check_both(
        &JsonLinesOptions::json_lines(),
        finite::<JsonLines>(input),
        input,
        &expected,
    )
    .await;
    check_both(
        &JsonLinesOptions::ndjson(),
        finite::<Ndjson>(input),
        input,
        &expected,
    )
    .await;
}

/// Small canonical inputs whose chunk boundaries can fall inside a multibyte UTF-8 character,
/// a CRLF, a number token or a string escape: every partition gives the same records.
#[tokio::test]
async fn every_split_of_canonical_inputs() {
    let cases: [(&[u8], Vec<Item<Value>>); 5] = [
        (
            "{\"k\":\"東京🎉\"}\r\n[\"é\",\"ß\"]\n\"Zoë\"".as_bytes(),
            vec![
                ok(json!({"k": "東京🎉"})),
                ok(json!(["é", "ß"])),
                ok(json!("Zoë")),
            ],
        ),
        (
            b"1\r\n[2]\r\n\"x\"\r\n",
            vec![ok(json!(1)), ok(json!([2])), ok(json!("x"))],
        ),
        (
            b"12\n-0\n3.25e-2\n123456789012345678\n",
            vec![
                ok(json!(12)),
                ok(json!(-0.0)),
                ok(json!(0.0325)),
                ok(json!(123_456_789_012_345_678_u64)),
            ],
        ),
        (
            br#""a\"b\n"
{"k\u0022":"\\"}
"#,
            vec![ok(json!("a\"b\n")), ok(json!({"k\"": "\\"}))],
        ),
        (
            b" {\"a\":1} \t\r\n\t[ ]\n",
            vec![ok(json!({"a": 1})), ok(json!([]))],
        ),
    ];
    for (input, expected) in cases {
        check_both(
            &JsonLinesOptions::json_lines(),
            finite::<JsonLines>(input),
            input,
            &expected,
        )
        .await;
    }
}

/// JSON Lines refuses blank lines wherever they are; NDJSON ignores them, including a final
/// whitespace-only fragment without LF.
#[tokio::test]
async fn blank_line_policies() {
    use ReadErrorKind::Format;
    /// Input, strict outcome, lenient outcome.
    type Case = (&'static [u8], Vec<Item<Value>>, Vec<Item<Value>>);
    let cases: [Case; 6] = [
        (b"\n1\n", vec![err(Format, BLANK, 1)], vec![ok(json!(1))]),
        (
            b"1\n\n",
            vec![ok(json!(1)), err(Format, BLANK, 2)],
            vec![ok(json!(1))],
        ),
        (
            b"1\n \t \n2\n",
            vec![ok(json!(1)), err(Format, BLANK, 2)],
            vec![ok(json!(1)), ok(json!(2))],
        ),
        (
            b"1\r\n\r\n2",
            vec![ok(json!(1)), err(Format, BLANK, 2)],
            vec![ok(json!(1)), ok(json!(2))],
        ),
        (
            b"1\n   ",
            vec![ok(json!(1)), err(Format, BLANK, 2)],
            vec![ok(json!(1))],
        ),
        (b"\n\n\n", vec![err(Format, BLANK, 1)], vec![]),
    ];
    for (input, strict, lenient) in cases {
        check_both(
            &JsonLinesOptions::json_lines(),
            finite::<JsonLines>(input),
            input,
            &strict,
        )
        .await;
        check_both(
            &JsonLinesOptions::ndjson(),
            finite::<Ndjson>(input),
            input,
            &lenient,
        )
        .await;
    }
}

/// A byte order mark is recognized only at the very start, also when split; elsewhere (or
/// partial) it is content, which is not JSON.
#[tokio::test]
async fn byte_order_mark_policies() {
    use ReadErrorKind::Format;
    let skip = JsonLinesOptions::json_lines().bom(BomPolicy::Skip);
    every_partition(
        &JsonLinesOptions::ndjson(),
        b"\xef\xbb\xbf1\n2\n",
        &[err::<Value>(Format, BOM, 1)],
    )
    .await;
    every_partition(&skip, b"\xef\xbb\xbf1\n2\n", &[ok(json!(1)), ok(json!(2))]).await;
    every_partition(
        &skip,
        b"1\n\xef\xbb\xbf2\n",
        &[ok(json!(1)), err(Format, NOT_JSON, 2)],
    )
    .await;
    every_partition(
        &JsonLinesOptions::json_lines(),
        b"\xef\xbb1\n",
        &[err::<Value>(Format, NOT_UTF8, 1)],
    )
    .await;
}

/// A chunk boundary is never an end of input: only the true end accepts a final line without
/// LF.
#[tokio::test]
async fn final_line_only_at_the_true_end() {
    let options = JsonLinesOptions::json_lines();
    let got = read_all::<Value>(
        &options,
        vec![
            b"1\n2".to_vec(),
            b"3".to_vec(),
            Vec::new(),
            b"4\n5".to_vec(),
        ],
    )
    .await;
    assert_eq!(got, [ok(json!(1)), ok(json!(234)), ok(json!(5))]);
}

/// UTF-8 is validated for the whole line, whatever the target type reads: even a type that
/// ignores the value refuses invalid bytes.
#[tokio::test]
async fn utf8_is_validated_whatever_the_type_reads() {
    let input = b"{\"skip\":\"\xff\"}\n";
    let got =
        read_all::<serde::de::IgnoredAny>(&JsonLinesOptions::json_lines(), vec![input.to_vec()])
            .await;
    assert_eq!(got.len(), 1);
    assert_eq!(
        got[0].as_ref().map(|_| ()),
        Err(&(ReadErrorKind::Format, NOT_UTF8, Some(1)))
    );
    let error = JsonLinesReader::new(JsonLinesOptions::json_lines())
        .read::<Value, _>(Chunks::new([&input[..]]))
        .next()
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(error.codec().unwrap().get("column"), Some(10));
}

#[derive(Debug, PartialEq, serde::Deserialize, serde::Serialize)]
struct Rec {
    id: u32,
}

/// Valid JSON of the wrong shape is a `Decode` error; invalid JSON is `Format`.
#[tokio::test]
async fn wrong_shape_is_decode_invalid_json_is_format() {
    let options = JsonLinesOptions::json_lines();
    assert_eq!(
        read_all::<Rec>(
            &options,
            vec![b"{\"id\":1}\n{\"id\":\"x\"}\n{\"id\":3}\n".to_vec()]
        )
        .await,
        [Ok(Rec { id: 1 }), err(ReadErrorKind::Decode, WRONG_TYPE, 2)]
    );
    assert_eq!(
        read_all::<Rec>(&options, vec![b"{\"id\":1} {\"id\":2}\n".to_vec()]).await,
        [err(ReadErrorKind::Format, NOT_JSON, 1)]
    );
}

/// `RecordErrors::Yield` reports each fully delimited bad line as an `Err` item and continues;
/// `Stop` (the default) ends at the first one. Positions count delivered records only.
#[tokio::test]
async fn yield_mode_isolates_bad_lines_and_continues() {
    use ReadErrorKind::{Decode, Format};
    let input: &[u8] = b"{\"id\":1}\n{\"id\":\n\n{\"id\":\"x\"}\n{\"id\":\"\xff\"}\n{\"id\":6} x\n{\"id\":7}\r\n{\"id\"";
    let yield_options = JsonLinesOptions::json_lines().record_errors(RecordErrors::Yield);
    let expected = [
        Ok(Rec { id: 1 }),
        err(Format, ENDS_INSIDE, 2),
        err(Format, BLANK, 3),
        err(Decode, WRONG_TYPE, 4),
        err(Format, NOT_UTF8, 5),
        err(Format, NOT_JSON, 6),
        Ok(Rec { id: 7 }),
        err(Format, ENDS_INSIDE, 8),
    ];
    every_partition(&yield_options, input, &expected).await;
    every_partition(
        &JsonLinesOptions::json_lines(),
        input,
        &[Ok(Rec { id: 1 }), err(Format, ENDS_INSIDE, 2)],
    )
    .await;

    // A yielded error does not fuse; its position counts delivered records.
    let mut records = JsonLinesReader::new(yield_options).read::<Rec, _>(Chunks::new([input]));
    assert_eq!(records.next().await.unwrap().unwrap(), Rec { id: 1 });
    let bad = records.next().await.unwrap().unwrap_err();
    assert_eq!(bad.position().records, 1);
    assert_eq!(bad.position().bytes, 16, "through the bad line's LF");
    for _ in 0..4 {
        assert!(records.next().await.unwrap().is_err());
    }
    assert_eq!(records.next().await.unwrap().unwrap(), Rec { id: 7 });
    assert!(records.next().await.unwrap().is_err());
    assert_eq!(records.position().records, 2);
    assert_eq!(records.position().bytes, input.len() as u64);
    assert!(records.next().await.is_none());
}

/// A line longer than `max_record_bytes` is a terminal `Limit` error found before the line is
/// buffered past the limit: with 1-byte chunks the reader stops polling the source right there,
/// far before the end of the input. Yield mode does not make it recoverable.
#[tokio::test]
async fn oversized_line_is_a_limit_before_growth() {
    let max = 32;
    let mut input = b"{\"id\":1}\n\"".to_vec();
    input.extend(std::iter::repeat_n(b'x', 40));
    input.extend_from_slice(b"\"\n");
    for i in 0..1000 {
        input.extend_from_slice(format!("{{\"id\":{i}}}\n").as_bytes());
    }
    for errors in [RecordErrors::Stop, RecordErrors::Yield] {
        let options = JsonLinesOptions::json_lines()
            .max_record_bytes(max)
            .record_errors(errors);
        let (source, polls) = Counting::new(input.iter().map(|b| vec![*b]));
        let mut records = JsonLinesReader::new(options.clone()).read::<Value, _>(source);
        assert_eq!(records.next().await.unwrap().unwrap(), json!({"id": 1}));
        let error = records.next().await.unwrap().unwrap_err();
        assert_eq!(error.kind(), ReadErrorKind::Limit);
        assert_eq!(error.codec().unwrap().label(), LIMIT);
        assert_eq!(error.codec().unwrap().get("limit"), Some(max as u64));
        assert!(records.next().await.is_none(), "terminal in {errors:?}");
        // 9 bytes of line 1, then at most `max + 1` bytes of line 2 (and the one that crosses).
        let polled = polls.load(Ordering::SeqCst);
        assert!(
            polled <= 9 + max + 2,
            "polled {polled} of {} chunks",
            input.len()
        );
        assert!(error.position().bytes <= 9 + max as u64 + 1);
    }
    // The outcome does not depend on chunking.
    let small = b"1\n\"0123456789\"\n2\n";
    every_partition(
        &JsonLinesOptions::json_lines().max_record_bytes(8),
        small,
        &[ok(json!(1)), err(ReadErrorKind::Limit, LIMIT, 2)],
    )
    .await;
    // Exactly at the limit (CRLF excluded) passes.
    every_partition(
        &JsonLinesOptions::json_lines().max_record_bytes(12),
        b"\"0123456789\"\r\n",
        &[ok(json!("0123456789"))],
    )
    .await;
}

/// Optional totals end with a `Limit` error, never a clean end; an input exactly at the record
/// total ends cleanly.
#[tokio::test]
async fn totals_are_limit_errors() {
    let records = JsonLinesOptions::json_lines().max_records(2);
    assert_eq!(
        read_all::<Value>(&records, vec![b"1\n2\n".to_vec()]).await,
        [ok(json!(1)), ok(json!(2))]
    );
    assert_eq!(
        read_all::<Value>(&records, vec![b"1\n2\n3\n".to_vec()]).await,
        [
            ok(json!(1)),
            ok(json!(2)),
            Err((
                ReadErrorKind::Limit,
                "the input exceeds the configured total records",
                None
            ))
        ]
    );
    // The byte total is checked per received chunk.
    let bytes = JsonLinesOptions::json_lines().max_total_bytes(4);
    assert_eq!(
        read_all::<Value>(&bytes, vec![b"1\n".to_vec(), b"2\n".to_vec()]).await,
        [ok(json!(1)), ok(json!(2))]
    );
    assert_eq!(
        read_all::<Value>(
            &bytes,
            vec![b"1\n".to_vec(), b"2\n".to_vec(), b"3\n".to_vec()]
        )
        .await,
        [
            ok(json!(1)),
            ok(json!(2)),
            Err((
                ReadErrorKind::Limit,
                "the input exceeds the configured total bytes",
                None
            ))
        ]
    );
}

/// A long run of ignored blank lines (here 1 MiB from an always-ready source) is consumed in
/// bounded steps: the reader yields to the executor about once per 64 KiB.
#[test]
fn ignored_blank_runs_yield_cooperatively() {
    let mut input = vec![b'\n'; 1 << 20];
    input.extend_from_slice(b"{\"id\":1}\n");
    let chunks: Vec<Vec<u8>> = input.chunks(4096).map(<[u8]>::to_vec).collect();
    let mut records =
        JsonLinesReader::new(JsonLinesOptions::ndjson()).read::<Value, _>(Chunks::new(chunks));
    let (first, pendings) = poll_counting(records.next());
    assert_eq!(first.unwrap().unwrap(), json!({"id": 1}));
    assert!(pendings >= 15, "yielded {pendings} times");
}

/// The finite encoders write compact JSON with an LF after every record, the last included;
/// both presets write the same bytes under their own `Content-Type`.
#[test]
fn finite_encoding_is_compact_with_lf_after_every_record() {
    let records = vec![
        json!({"id": 1, "name": "a b"}),
        json!([1, 2]),
        json!("line\nbreak"),
        Value::Null,
        json!(1.5),
        json!(false),
    ];
    let expected: &[u8] =
        b"{\"id\":1,\"name\":\"a b\"}\n[1,2]\n\"line\\nbreak\"\nnull\n1.5\nfalse\n";
    let body = <JsonLines as Encode<Vec<Value>>>::encode(records.clone()).unwrap();
    assert_eq!(body.as_bytes(), Some(expected));
    let body = <Ndjson as Encode<Vec<Value>>>::encode(records.clone()).unwrap();
    assert_eq!(body.as_bytes(), Some(expected));
    assert_eq!(
        <JsonLines as Encode<Vec<Value>>>::CONTENT_TYPE,
        Some("application/jsonl")
    );
    assert_eq!(
        <Ndjson as Encode<Vec<Value>>>::CONTENT_TYPE,
        Some("application/x-ndjson")
    );
    assert_eq!(
        <JsonLines as Decode<Vec<Value>>>::ACCEPT,
        Some("application/jsonl")
    );
    assert_eq!(
        <Ndjson as Decode<Vec<Value>>>::ACCEPT,
        Some("application/x-ndjson")
    );
    let writer = JsonLinesWriter::new(JsonLinesOptions::json_lines());
    assert_eq!(writer.to_vec(&records).unwrap(), expected);
    assert_eq!(writer.content_type(), "application/jsonl");
    assert_eq!(
        JsonLinesWriter::new(JsonLinesOptions::ndjson()).content_type(),
        "application/x-ndjson"
    );
    // Round trip through the finite decoder.
    let decoded =
        <JsonLines as Decode<Vec<Value>>>::decode_body((), Bytes::from_static(expected)).unwrap();
    assert_eq!(decoded, records);
    // No records, no bytes.
    let empty = <JsonLines as Encode<Vec<Value>>>::encode(Vec::new()).unwrap();
    assert_eq!(empty.as_bytes(), Some(&b""[..]));
}

/// An unencodable record names its position; a raw JSON value spanning lines would split the
/// record and is refused.
#[test]
fn encoding_errors_name_the_record() {
    let mut bad = BTreeMap::new();
    bad.insert((1, 2), 3);
    let error =
        <JsonLines as Encode<Vec<BTreeMap<(i32, i32), i32>>>>::encode(vec![BTreeMap::new(), bad])
            .unwrap_err();
    assert_eq!(error.label(), "a record could not be encoded as JSON");
    assert_eq!(error.get("record"), Some(2));
    assert!(error.source_unredacted().is_some());

    let raw = serde_json::value::RawValue::from_string("{\n\"a\": 1\n}".to_owned()).unwrap();
    let error = JsonLinesWriter::new(JsonLinesOptions::json_lines())
        .to_vec([raw])
        .unwrap_err();
    assert_eq!(
        error.label(),
        "a record encoded as JSON contains a line feed (a raw JSON value spanning lines)"
    );
    assert_eq!(error.get("record"), Some(1));
    let flat = serde_json::value::RawValue::from_string("{\"a\": 1}".to_owned()).unwrap();
    assert_eq!(
        JsonLinesWriter::new(JsonLinesOptions::json_lines())
            .to_vec([flat])
            .unwrap(),
        b"{\"a\": 1}\n"
    );
}

/// Collects a request body by polling it directly (no network).
fn collect_body(body: concord_core::reqwest::Body) -> Result<Vec<u8>, String> {
    use http_body::Body as _;
    let mut body = std::pin::pin!(body);
    let mut out = Vec::new();
    loop {
        let (frame, _) = poll_counting(std::future::poll_fn(|cx| body.as_mut().poll_frame(cx)));
        match frame {
            None => return Ok(out),
            Some(Ok(frame)) => out.extend_from_slice(frame.data_ref().expect("data frame")),
            Some(Err(e)) => return Err(e.to_string()),
        }
    }
}

/// `Encode<StreamRecords<S>>` polls the source zero times when the body is built, then
/// produces exactly the finite encoding when polled, consuming the source once.
#[test]
fn streamed_encoding_is_lazy_and_matches_the_finite_one() {
    let polls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = std::sync::Arc::clone(&polls);
    let source = from_iter((0..2000u32).map(move |id| {
        counter.fetch_add(1, Ordering::SeqCst);
        Ok::<_, std::io::Error>(Rec { id })
    }));
    let body = <Ndjson as Encode<StreamRecords<_>>>::encode(StreamRecords::new(source)).unwrap();
    assert_eq!(polls.load(Ordering::SeqCst), 0, "no poll while preparing");
    let streamed = collect_body(body).unwrap();
    assert_eq!(
        polls.load(Ordering::SeqCst),
        2000,
        "each record pulled once"
    );
    let finite = JsonLinesWriter::new(JsonLinesOptions::ndjson())
        .to_vec((0..2000u32).map(|id| Rec { id }))
        .unwrap();
    assert_eq!(streamed, finite);
    let decoded = <Ndjson as Decode<Vec<Value>>>::decode_body((), Bytes::from(streamed)).unwrap();
    assert_eq!(decoded.len(), 2000);
}

fn head_verdict<F: Decode<Vec<Value>>>(content_types: &[&[u8]]) -> Result<(), &'static str> {
    let mut map = HeaderMap::new();
    for ct in content_types {
        map.append(CONTENT_TYPE, HeaderValue::from_bytes(ct).unwrap());
    }
    let head = ResponseHead::new(StatusCode::OK, Version::HTTP_11, &map);
    F::check_content_type(&head)
        .and_then(|()| F::context(&head).map(drop))
        .map_err(|e| e.label())
}

/// The finite formats' response head checks: the documented media types (case-insensitive,
/// parameters allowed, charset UTF-8 only), everything else refused before the body is read;
/// `IgnoreContentType` skips exactly that check.
#[test]
fn finite_media_types() {
    for ct in [
        &b"application/jsonl"[..],
        b"application/jsonlines",
        b"application/x-jsonlines",
        b"Application/JSONL; charset=utf-8",
        b"application/jsonl;charset=\"US-ASCII\"",
    ] {
        assert_eq!(head_verdict::<JsonLines>(&[ct]), Ok(()), "{ct:?}");
    }
    for ct in [
        &b"application/x-ndjson"[..],
        b"application/ndjson",
        b"APPLICATION/X-NDJSON; foo=bar",
    ] {
        assert_eq!(head_verdict::<Ndjson>(&[ct]), Ok(()), "{ct:?}");
    }
    let jsonl_refused = "the response Content-Type is not a JSON Lines media type (application/jsonl, application/jsonlines, application/x-jsonlines)";
    for (ct, label) in [
        (&b"application/x-ndjson"[..], jsonl_refused),
        (b"application/json", jsonl_refused),
        (b"text/plain", jsonl_refused),
        (b"application/octet-stream", jsonl_refused),
        (b"application/jsonl+json", jsonl_refused),
        (
            b"application/*",
            "the response Content-Type is a media range, not a media type",
        ),
        (
            b"application/jsonl; charset=latin1",
            "the response declares a charset other than utf-8",
        ),
        (
            b"application/jsonl; charset=utf-8; charset=utf-8",
            "the response Content-Type declares charset more than once",
        ),
        (b"jsonl", "the response Content-Type is not a media type"),
    ] {
        assert_eq!(head_verdict::<JsonLines>(&[ct]), Err(label), "{ct:?}");
    }
    assert_eq!(
        head_verdict::<Ndjson>(&[b"application/jsonl"]),
        Err(
            "the response Content-Type is not an NDJSON media type (application/x-ndjson, application/ndjson)"
        )
    );
    assert_eq!(
        head_verdict::<JsonLines>(&[]),
        Err("the response has no Content-Type")
    );
    assert_eq!(
        head_verdict::<JsonLines>(&[b"application/jsonl", b"application/jsonl"]),
        Err("the response has more than one Content-Type field")
    );
    assert_eq!(
        head_verdict::<IgnoreContentType<JsonLines>>(&[b"text/html"]),
        Ok(())
    );
    assert_eq!(head_verdict::<IgnoreContentType<Ndjson>>(&[]), Ok(()));
}

/// Readers, their records and their futures can move across threads (an SDK spawns them).
#[test]
fn readers_and_futures_are_send() {
    fn send<T: Send>(_: &T) {}
    fn send_sync<T: Send + Sync + Clone>() {}
    send_sync::<JsonLinesReader>();
    send_sync::<JsonLinesWriter>();
    send_sync::<JsonLinesOptions>();
    fn records_future(r: &mut concord_extras::jsonl::JsonLinesRecords<Value>) {
        send(&r.next());
    }
    let _ = records_future;
    let mut records = JsonLinesReader::new(JsonLinesOptions::json_lines())
        .read::<Value, _>(Chunks::new([&b"1\n"[..]]));
    send(&records);
    send(&records.next());
}

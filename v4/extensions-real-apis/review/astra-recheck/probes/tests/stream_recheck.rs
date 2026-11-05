use astra_recheck_probes::{drain, once, run};
use concord_core::bytes::Bytes;
use concord_extras::{
    csv::{CsvOptions, CsvReader, CsvWriter, HeaderMode, StreamRows},
    jsonl::{
        BlankLines, BomPolicy, JsonLinesOptions, JsonLinesReader, JsonValuesReader, RecordErrors,
    },
    stream::{ByteSource, Chunks, ReadError, SourceError, from_iter},
};
use serde_json::Value;
use std::{
    convert::Infallible,
    marker::PhantomPinned,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering::SeqCst},
    },
};
fn error(e: ReadError) -> String {
    format!(
        "{:?}:{}:{}",
        e.kind(),
        e.is_terminal(),
        e.codec().unwrap().label()
    )
}
fn lines(data: Vec<Vec<u8>>, max: usize) -> Vec<Result<Value, String>> {
    let mut r = JsonLinesReader::new(
        JsonLinesOptions::json_lines()
            .max_record_bytes(max)
            .bom(BomPolicy::Skip)
            .blank_lines(BlankLines::Ignore)
            .record_errors(RecordErrors::Yield),
    )
    .read(Chunks::new(data));
    let mut out = vec![];
    while let Some(v) = run(r.next()) {
        out.push(v.map_err(error));
    }
    out
}
fn values(data: Vec<Vec<u8>>, max: usize) -> Vec<Result<Value, String>> {
    let mut r = JsonValuesReader::new()
        .max_value_bytes(max)
        .read(Chunks::new(data));
    let mut out = vec![];
    while let Some(v) = run(r.next()) {
        out.push(v.map_err(error));
    }
    out
}
#[test]
fn every_small_boundary_and_exact_limits() {
    let bodies: Vec<&[u8]> = vec![
        b"",
        b"1",
        b"1\r\n2\nnull",
        b"\xef\xbb\xbf1\r\n",
        b"\xef\xbb",
        b"1\nnot-json",
        b"1\nnot-json\n2\n",
        b"1\n\"cut",
        b"\"a\\\"b\"\n[]\n",
        b"\"\xc3\xa9\"\r\n",
        b"\n \r\n1\n",
        b"{\"a\":[true,null]}\n",
        b"1\r",
        b"1\r\r\n",
        b"\xff\n1\n",
    ];
    let mut count = 0;
    for body in bodies {
        for max in [1, 2, 3, 4, 8, 16, 64] {
            let expected_lines = lines(vec![body.to_vec()], max);
            let expected_values = values(vec![body.to_vec()], max);
            for split in 0..=body.len() {
                let chunks = vec![
                    vec![],
                    body[..split].to_vec(),
                    vec![],
                    body[split..].to_vec(),
                    vec![],
                ];
                assert_eq!(
                    lines(chunks.clone(), max),
                    expected_lines,
                    "lines body={body:?} split={split} max={max}"
                );
                assert_eq!(
                    values(chunks, max),
                    expected_values,
                    "values body={body:?} split={split} max={max}"
                );
                count += 2;
            }
            assert_eq!(
                lines(body.chunks(1).map(|c| c.to_vec()).collect(), max),
                expected_lines
            );
            assert_eq!(
                values(body.chunks(1).map(|c| c.to_vec()).collect(), max),
                expected_values
            );
        }
    }
    println!("JSON boundary/limit comparisons: {count} split cases plus bytewise controls");
}
#[test]
fn slice_boundaries_do_not_become_eof() {
    let mut count = 0;
    for n in [
        65530, 65531, 65532, 65533, 65534, 65535, 65536, 65537, 65538, 131069, 131070, 131071,
        131072,
    ] {
        let value = Value::String(format!("{}é\\\"\r\n", "x".repeat(n)));
        let encoded = serde_json::to_vec(&value).unwrap();
        for terminated in [false, true] {
            let mut body = encoded.clone();
            if terminated {
                body.extend_from_slice(b"\r\n");
            }
            for split in [0, 1, 65535, 65536, 65537, body.len() - 1, body.len()] {
                let chunks = vec![body[..split].to_vec(), vec![], body[split..].to_vec()];
                assert_eq!(
                    lines(chunks.clone(), encoded.len()),
                    vec![Ok(value.clone())]
                );
                assert_eq!(
                    values(chunks.clone(), encoded.len()),
                    vec![Ok(value.clone())]
                );
                assert!(lines(chunks.clone(), encoded.len() - 1)[0].is_err());
                assert!(values(chunks, encoded.len() - 1)[0].is_err());
                count += 4;
            }
        }
        let mut cut = encoded.clone();
        cut.pop();
        assert!(
            lines(vec![cut.clone()], encoded.len())[0]
                .as_ref()
                .unwrap_err()
                .contains(":true:")
        );
        assert!(values(vec![cut], encoded.len())[0].is_err());
    }
    // A primitive spanning the 64 KiB step must wait for a real delimiter/EOF.
    let body = format!("{}1", " ".repeat(65535));
    assert_eq!(values(vec![body.into_bytes()], 1), vec![Ok(Value::from(1))]);
    println!(
        "large JSON slice/CRLF/escape/UTF-8/limit comparisons: {count}; truncated EOF rejected"
    );
}
struct EmptyThen {
    calls: Arc<AtomicUsize>,
    remaining: usize,
    sent: bool,
}
impl ByteSource for EmptyThen {
    async fn next_chunk(&mut self) -> Result<Option<Bytes>, SourceError> {
        self.calls.fetch_add(1, SeqCst);
        if self.remaining > 0 {
            self.remaining -= 1;
            return Ok(Some(Bytes::new()));
        }
        if !self.sent {
            self.sent = true;
            Ok(Some(Bytes::from_static(b"7\n")))
        } else {
            Ok(None)
        }
    }
}
fn source() -> (EmptyThen, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    (
        EmptyThen {
            calls: calls.clone(),
            remaining: 4096,
            sent: false,
        },
        calls,
    )
}
#[test]
fn cancellation_empty_budget_and_partial_record_budget() {
    let (s, c) = source();
    let mut csv = CsvReader::new(CsvOptions::new(HeaderMode::Absent, b','))
        .unwrap()
        .read::<(u64,), _>(s);
    assert!(once(csv.next()).is_pending());
    assert_eq!(c.load(SeqCst), 64);
    assert_eq!(run(csv.next()).unwrap().unwrap(), (7,));
    let (s, c) = source();
    let mut lines = JsonLinesReader::new(JsonLinesOptions::json_lines()).read::<u64, _>(s);
    assert!(once(lines.next()).is_pending());
    assert_eq!(c.load(SeqCst), 64);
    assert_eq!(run(lines.next()).unwrap().unwrap(), 7);
    let (s, c) = source();
    let mut values = JsonValuesReader::new().read::<u64, _>(s);
    assert!(once(values.next()).is_pending());
    assert_eq!(c.load(SeqCst), 64);
    assert_eq!(run(values.next()).unwrap().unwrap(), 7);
    let mut b = vec![b'x'; 8 * 1024 * 1024];
    b[0] = b'"';
    let mut l = JsonLinesReader::new(JsonLinesOptions::json_lines())
        .read::<String, _>(Chunks::new([b.clone()]));
    let mut v = JsonValuesReader::new().read::<String, _>(Chunks::new([b]));
    for i in 1..=4 {
        assert!(once(l.next()).is_pending());
        assert!(once(v.next()).is_pending());
        assert_eq!(l.position().bytes, i * 65536);
        assert_eq!(v.position().bytes, i * 65536);
    }
    println!(
        "all readers: first poll 64 empty chunks; cancellation retains record; JSON cancelled futures consume 65536 bytes/poll"
    );
}
#[test]
fn documented_cost_and_limits_before_growth() {
    let size = 1048577;
    let mut bytes = vec![b'x'; size];
    bytes[0] = b'"';
    bytes[size - 1] = b'"';
    bytes.push(b'\n');
    let mut l = JsonLinesReader::new(JsonLinesOptions::json_lines())
        .read::<String, _>(Chunks::new([bytes.clone()]));
    let mut v = JsonValuesReader::new().read::<String, _>(Chunks::new([bytes.clone()]));
    assert_eq!(run(l.next()).unwrap().unwrap().len(), size - 2);
    assert_eq!(run(v.next()).unwrap().unwrap().len(), size - 2);
    assert_eq!(l.peak_retained_bytes(), size);
    assert_eq!(v.peak_retained_bytes(), size);
    println!(
        "1 MiB string line: peak_retained JSONL={}, values={}",
        l.peak_retained_bytes(),
        v.peak_retained_bytes()
    );
    let mut l = JsonLinesReader::new(JsonLinesOptions::json_lines().max_record_bytes(524288))
        .read::<String, _>(Chunks::new([bytes.clone()]));
    let mut v = JsonValuesReader::new()
        .max_value_bytes(524288)
        .read::<String, _>(Chunks::new([bytes]));
    assert_eq!(
        run(l.next()).unwrap().unwrap_err().kind(),
        concord_extras::stream::ReadErrorKind::Limit
    );
    assert_eq!(
        run(v.next()).unwrap().unwrap_err().kind(),
        concord_extras::stream::ReadErrorKind::Limit
    );
    assert_eq!(l.position().bytes, 524288);
    assert_eq!(v.position().bytes, 524288);
    assert_eq!(l.peak_retained_bytes(), 524288);
    assert_eq!(v.peak_retained_bytes(), 524288);
    println!("512 KiB limit: JSONL and values position=524288, peak_retained=524288");
    for mib in [1, 8, 32] {
        let b = vec![b'x'; mib * 1024 * 1024];
        let mut r = JsonLinesReader::new(JsonLinesOptions::json_lines().max_record_bytes(64))
            .read::<Value, _>(Chunks::new([b]));
        let start = std::time::Instant::now();
        assert!(once(r.next()).is_ready());
        let elapsed = start.elapsed();
        assert_eq!(r.peak_retained_bytes(), 0);
        println!("JSONL overlimit chunk={mib}MiB peak_retained=0 first_poll={elapsed:?}");
    }
}
#[test]
fn debug_unpin_and_yield_docs() {
    let text = format!(
        "{:?}",
        Chunks::new(["secret-token", "secret-body"]).then_fail("secret-error")
    );
    assert!(!text.contains("secret"));
    println!("Chunks Debug: {text}");
    #[derive(serde::Serialize)]
    struct Pinned {
        id: u8,
        #[serde(skip)]
        _pin: PhantomPinned,
    }
    let rows = from_iter(vec![Ok::<_, Infallible>(Pinned {
        id: 7,
        _pin: PhantomPinned,
    })]);
    fn assert_unpin<T: Unpin>(_: &T) {}
    assert_unpin(&rows);
    let b = CsvWriter::new(CsvOptions::new(HeaderMode::Present, b','))
        .unwrap()
        .upload(StreamRows::new(rows))
        .unwrap()
        .into_parts()
        .0;
    assert_eq!(drain(b).unwrap(), b"id\r\n7\r\n");
    for (suffix, terminal) in [
        ("not-json", true),
        ("not-json\n", false),
        ("\"wrong type\"", true),
    ] {
        let data = format!("1\n{suffix}");
        let mut r =
            JsonLinesReader::new(JsonLinesOptions::json_lines().record_errors(RecordErrors::Yield))
                .read::<u64, _>(Chunks::new([data.into_bytes()]));
        assert_eq!(run(r.next()).unwrap().unwrap(), 1);
        assert_eq!(run(r.next()).unwrap().unwrap_err().is_terminal(), terminal);
        assert!(run(r.next()).is_none());
    }
    println!(
        "!Unpin item via from_iter -> CSV upload passes; Yield malformed/type-error EOF terminal, LF nonterminal"
    );
}

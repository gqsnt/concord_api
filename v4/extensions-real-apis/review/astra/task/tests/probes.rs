//! Post-task probes. Assertions capture observed defects, not desired fixed behavior.
use concord::{
    bytes::Bytes,
    csv::{CsvOptions, CsvReader, CsvWriter, HeaderMode, Rows},
    jsonl::{JsonLinesOptions, JsonLinesReader, JsonValuesReader},
    stream::{ByteSource, Chunks, SourceError},
};
use std::{
    future::Future,
    pin::pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering::SeqCst},
    },
    task::{Context, Poll, Waker},
};

#[tokio::test]
async fn escape_profiles_round_trip_probe() {
    let original = r"back\slash,comma".to_owned();
    // Control: the default quote-doubling dialect preserves literal backslashes.
    let options = CsvOptions::new(HeaderMode::Absent, b',');
    let encoded = CsvWriter::new(options.clone())
        .unwrap()
        .to_vec(Rows::new([(original.clone(),)]))
        .unwrap();
    let mut reader = CsvReader::new(options)
        .unwrap()
        .read::<(String,), _>(Chunks::new([encoded]));
    assert_eq!(reader.next().await.unwrap().unwrap().0, original);
    for double_quote in [true, false] {
        let options = CsvOptions::new(HeaderMode::Absent, b',')
            .double_quote(double_quote)
            .escape(Some(b'\\'));
        let encoded = CsvWriter::new(options.clone())
            .unwrap()
            .to_vec(Rows::new([(original.clone(),)]))
            .unwrap();
        println!(
            "escape profile: double_quote={double_quote}, original={original:?}, wire={:?}",
            String::from_utf8_lossy(&encoded)
        );
        let mut reader = CsvReader::new(options)
            .unwrap()
            .read::<(String,), _>(Chunks::new([encoded]));
        let decoded = reader.next().await.unwrap().unwrap().0;
        println!("decoded={decoded:?}");
        assert_ne!(decoded, original, "probe records current data-loss bug");
        assert_eq!(decoded, "backslash,comma");
    }
}

struct EmptyRun {
    count: Arc<AtomicUsize>,
    empty: usize,
}
impl ByteSource for EmptyRun {
    async fn next_chunk(&mut self) -> Result<Option<Bytes>, SourceError> {
        let n = self.count.fetch_add(1, SeqCst);
        Ok(if n < self.empty {
            Some(Bytes::new())
        } else if n == self.empty {
            Some(Bytes::from_static(b"1\n"))
        } else {
            None
        })
    }
}

#[test]
fn one_poll_consumes_a_million_empty_chunks_in_every_reader() {
    fn check<F: Future>(future: F, polls: Arc<AtomicUsize>, name: &str) {
        let mut future = pin!(future);
        let mut cx = Context::from_waker(Waker::noop());
        let ready = future.as_mut().poll(&mut cx).is_ready();
        println!(
            "{name}: ready={ready}; source polls in ONE future poll={}",
            polls.load(SeqCst)
        );
        assert!(ready);
        assert_eq!(polls.load(SeqCst), 1_000_001);
    }
    let make = || {
        let polls = Arc::new(AtomicUsize::new(0));
        (
            EmptyRun {
                count: polls.clone(),
                empty: 1_000_000,
            },
            polls,
        )
    };
    let (source, polls) = make();
    let mut csv = CsvReader::new(CsvOptions::new(HeaderMode::Absent, b','))
        .unwrap()
        .read::<(u64,), _>(source);
    check(csv.next(), polls, "csv");
    let (source, polls) = make();
    let mut lines = JsonLinesReader::new(JsonLinesOptions::json_lines()).read::<u64, _>(source);
    check(lines.next(), polls, "jsonl");
    let (source, polls) = make();
    let mut values = JsonValuesReader::new().read::<u64, _>(source);
    check(values.next(), polls, "values");
}

#[test]
fn jsonl_large_unfinished_chunk_work_probe() {
    let mut reader = JsonLinesReader::new(JsonLinesOptions::json_lines().max_record_bytes(64))
        .read::<String, _>(Chunks::new([Bytes::from(vec![b'x'; 32 * 1024 * 1024])]));
    let mut future = pin!(reader.next());
    let mut cx = Context::from_waker(Waker::noop());
    let before = std::time::Instant::now();
    let result = future.as_mut().poll(&mut cx);
    println!(
        "JSONL 32MiB unterminated chunk, 64-byte cap: ONE poll {:?}; ready={}",
        before.elapsed(),
        result.is_ready()
    );
    assert!(
        matches!(result, Poll::Ready(Some(Err(error))) if error.kind() == concord::stream::ReadErrorKind::Limit)
    );
}

#[test]
fn json_readers_exceed_the_documented_64k_poll_budget_on_one_partial_record() {
    const SIZE: usize = 8 * 1024 * 1024;
    fn poll_once<F: Future>(future: F) {
        let mut future = pin!(future);
        let mut cx = Context::from_waker(Waker::noop());
        assert!(future.as_mut().poll(&mut cx).is_pending());
    }
    let mut data = vec![b'x'; SIZE];
    data[0] = b'"'; // a still-unfinished string, so no deserialize work is involved
    let mut lines = JsonLinesReader::new(JsonLinesOptions::json_lines())
        .read::<String, _>(Chunks::new([Bytes::from(data.clone())]));
    poll_once(lines.next());
    let mut values =
        JsonValuesReader::new().read::<String, _>(Chunks::new([Bytes::from(data.clone())]));
    poll_once(values.next());
    let mut csv =
        CsvReader::new(CsvOptions::new(HeaderMode::Absent, b',').max_record_bytes(SIZE * 2))
            .unwrap()
            .read::<String, _>(Chunks::new([Bytes::from(data)]));
    poll_once(csv.next());
    println!(
        "bytes consumed in ONE poll of unfinished record: jsonl={}, values={}, csv={}",
        lines.position().bytes,
        values.position().bytes,
        csv.position().bytes
    );
    assert_eq!(lines.position().bytes, SIZE as u64);
    assert_eq!(values.position().bytes, SIZE as u64);
    assert_eq!(csv.position().bytes, 65536);
}

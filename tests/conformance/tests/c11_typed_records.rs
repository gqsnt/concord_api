//! C11.13, typed streams of records: `Sse::try_map` and `JsonLines::try_map`, finished by
//! `until_end_of_body` or `until_last_record`, give a `RecordStream<T, client::Error>`.
//!
//! One test for each row of the table of endings (C11.13 and the rustdoc of `TryMap`),
//! named after its row, for both readers where the row applies. Every test goes through a
//! description of `concord::api!` and a generated client over a `concord_testkit::Script`, on
//! Tokio's paused clock: no socket, no wait on real time.

mod common;

use common::engine::{secs, success};
use concord::error::Kind;
use concord::read::{
    JsonLine, JsonLines, RecordError, RecordFail, RecordStream, Sse, SseEvent, Step,
};
use concord::Retry;
use concord_testkit::Script;
use futures_core::Stream;
use serde::Deserialize;
use std::cell::Cell;
use std::error::Error as _;
use std::future::IntoFuture;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Wake, Waker};
use tokio::time::Instant;

/// The API's own error, in a record and in an error response.
#[derive(Debug, Deserialize, PartialEq)]
pub struct Problem {
    pub code: u16,
}

/// A record of the test API: its `kind` says what the mapping does with it.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Wire {
    Item { n: u32 },
    Ping,
    Last { n: u32 },
    End,
    Error { code: u16 },
    Reject,
    ForeignStream,
    ForeignDecode,
    Panic,
}

thread_local! {
    /// How many records the mapping was given, on this test's thread.
    static CALLS: Cell<usize> = const { Cell::new(0) };
}

/// A failure of another response's body, for the row that returns one from a mapping.
static FOREIGN: Mutex<Option<RecordError>> = Mutex::new(None);

fn calls() -> usize {
    CALLS.with(Cell::get)
}

fn wire(record: Wire) -> Result<Step<u32>, RecordFail<Problem>> {
    CALLS.with(|calls| calls.set(calls.get() + 1));
    match record {
        Wire::Item { n } => Ok(Step::Next(n)),
        Wire::Ping => Ok(Step::Skip),
        Wire::Last { n } => Ok(Step::Last(n)),
        Wire::End => Ok(Step::End),
        Wire::Error { code } => Err(RecordFail::problem(Problem { code })),
        Wire::Reject => Err(RecordFail::mismatch("a rejected record")),
        Wire::ForeignStream => {
            let foreign = FOREIGN.lock().unwrap().take().expect("a foreign error");
            Err(foreign.into())
        }
        Wire::ForeignDecode => {
            // A decoder's failure on other bytes than this record: only its place survives.
            let other = SseEvent::new("{\"x\": oops}");
            Err(other.json::<u32>().unwrap_err().into())
        }
        Wire::Panic => panic!("the mapping panics"),
    }
}

fn sse(event: SseEvent) -> Result<Step<u32>, RecordFail<Problem>> {
    wire(event.json()?)
}

fn line(line: JsonLine) -> Result<Step<u32>, RecordFail<Problem>> {
    wire(line.json()?)
}

concord::api! {
    client Feed {
        base "https://feed.example.test";
        problem Problem read concord::Json;

        GET sse_body() -> RecordStream<u32, feed::Error> { path "s"; read Sse::new(4096).try_map(sse).until_end_of_body(); }
        GET sse_last() -> RecordStream<u32, feed::Error> { path "s"; read Sse::new(4096).try_map(sse).until_last_record(); }
        GET lines_body() -> RecordStream<u32, feed::Error> { path "l"; read JsonLines::new(4096).try_map(line).until_end_of_body(); }
        GET lines_last() -> RecordStream<u32, feed::Error> { path "l"; read JsonLines::new(4096).try_map(line).until_last_record(); }
        // `data: {"kind":"item","n":1}` is 27 bytes and one delimiter; `{"kind":"item","n":1}` is 21.
        GET sse_small() -> RecordStream<u32, feed::Error> { path "s"; read Sse::new(28).try_map(sse).until_end_of_body(); }
        GET lines_small() -> RecordStream<u32, feed::Error> { path "l"; read JsonLines::new(21).try_map(line).until_end_of_body(); }
        GET sse_text() -> RecordStream<String, feed::Error> {
            path "s";
            read Sse::new(4096).try_map(|event| Ok(Step::Next(event.data))).until_end_of_body();
        }
        GET lines_text() -> RecordStream<String, feed::Error> {
            path "l";
            read JsonLines::new(4096).try_map(|line| Ok(Step::Next(line.text().to_owned()))).until_end_of_body();
        }
        // A raw stream, to take a failure of another response's body.
        GET raw() -> RecordStream<SseEvent> { path "r"; read Sse::new(4096); }
    }
}

#[derive(Clone, Copy, Debug)]
enum Reader {
    Sse,
    Lines,
}

const BOTH: [Reader; 2] = [Reader::Sse, Reader::Lines];

impl Reader {
    /// One record of the test API, framed by this reader.
    fn record(self, json: &str) -> Vec<u8> {
        match self {
            Reader::Sse => format!("data: {json}\n\n").into_bytes(),
            Reader::Lines => format!("{json}\n").into_bytes(),
        }
    }

    /// Several records, in one piece.
    fn records(self, records: &[&str]) -> Vec<u8> {
        records.iter().flat_map(|json| self.record(json)).collect()
    }

    fn endpoint(self, last: bool) -> &'static str {
        match (self, last) {
            (Reader::Sse, false) => "feed.sse_body",
            (Reader::Sse, true) => "feed.sse_last",
            (Reader::Lines, false) => "feed.lines_body",
            (Reader::Lines, true) => "feed.lines_last",
        }
    }

    /// The typed stream of this reader, ended by the body or by its last record.
    async fn open(self, feed: &Feed, last: bool) -> RecordStream<u32, feed::Error> {
        success(match (self, last) {
            (Reader::Sse, false) => feed.sse_body().await,
            (Reader::Sse, true) => feed.sse_last().await,
            (Reader::Lines, false) => feed.lines_body().await,
            (Reader::Lines, true) => feed.lines_last().await,
        })
    }
}

const ITEM_1: &str = r#"{"kind":"item","n":1}"#;
const ITEM_2: &str = r#"{"kind":"item","n":2}"#;
const PING: &str = r#"{"kind":"ping"}"#;

fn feed(script: &Script) -> Feed {
    Feed::new().with_transport(script.clone())
}

/// A client whose calls share one place in flight.
fn limited(script: &Script) -> Feed {
    feed(script).with_middleware(concord::limit::Limit::new().max_in_flight(1))
}

/// The next answer of the script: `200` with `x-trace: t1`, in these pieces.
fn answer(script: &Script, pieces: &[Vec<u8>]) {
    let mut answer = script.answer(200).header("x-trace", "t1");
    for piece in pieces {
        answer = answer.body(piece.clone());
    }
}

#[track_caller]
fn item<T: std::fmt::Debug>(next: Result<Option<T>, feed::Error>) -> T {
    match next {
        Ok(Some(value)) => value,
        Ok(None) => panic!("expected a value, got the end"),
        Err(error) => panic!("expected a value, got: {error}"),
    }
}

#[track_caller]
fn end<T: std::fmt::Debug>(next: Result<Option<T>, feed::Error>) {
    match next {
        Ok(None) => {}
        Ok(Some(value)) => panic!("expected the end, got {value:?}"),
        Err(error) => panic!("expected the end, got: {error}"),
    }
}

#[track_caller]
fn failed<T: std::fmt::Debug>(next: Result<Option<T>, feed::Error>) -> feed::Error {
    match next {
        Ok(Some(value)) => panic!("expected an error, got {value:?}"),
        Ok(None) => panic!("expected an error, got the end"),
        Err(error) => error,
    }
}

/// What every error of a record holds: the response's head, and nothing of its body.
#[track_caller]
fn of_the_response(error: &feed::Error, endpoint: &str) {
    assert_eq!(error.status().map(|s| s.as_u16()), Some(200));
    assert!(error.is_status(200));
    assert_eq!(error.headers().and_then(|h| h.get("x-trace")), Some("t1"));
    assert_eq!(error.endpoint(), endpoint);
    assert_eq!(error.tries(), 1);
    assert!(error.body().is_none());
    assert!(error.kept().is_none());
}

/// A Mismatch of a record, with this text after the status, and one cause of the same words.
#[track_caller]
fn record_mismatch(error: &feed::Error, endpoint: &str, words: &str) {
    assert_eq!(error.kind(), Kind::Mismatch);
    assert_eq!(
        error.to_string(),
        format!("mismatch: status 200, {words}, in {endpoint}")
    );
    let cause = error.source().expect("a cause");
    assert_eq!(cause.to_string(), words);
    assert!(cause.source().is_none());
    assert!(error.problem().is_none());
    of_the_response(error, endpoint);
}

/// The place in flight is free: another call goes at once.
async fn place_is_free(feed: &Feed, script: &Script, reader: Reader) {
    answer(script, &[reader.record(ITEM_2)]);
    let next = tokio::time::timeout(secs(1), reader_call(feed, reader)).await;
    let mut next = success(next.expect("the place in flight was not released"));
    assert_eq!(item(next.try_next().await), 2);
}

/// The place in flight is held: another call waits.
async fn place_is_held(feed: &Feed, reader: Reader) {
    assert!(tokio::time::timeout(secs(1), reader_call(feed, reader))
        .await
        .is_err());
}

async fn reader_call(feed: &Feed, reader: Reader) -> feed::Result<RecordStream<u32, feed::Error>> {
    match reader {
        Reader::Sse => feed.sse_body().into_future().await,
        Reader::Lines => feed.lines_body().into_future().await,
    }
}

/// A waker that counts how often it was woken.
struct Count(AtomicUsize);
impl Wake for Count {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

fn poll<T>(
    stream: &mut RecordStream<T, feed::Error>,
    cx: &mut Context<'_>,
) -> Poll<Option<Result<T, feed::Error>>> {
    Pin::new(stream).poll_next(cx)
}

// Row 1 ------------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c11_13_row_01_next_yields_the_value_and_the_next_record_is_read() {
    for reader in BOTH {
        let script = Script::new();
        answer(&script, &[reader.records(&[ITEM_1, ITEM_2])]);
        let mut stream = reader.open(&feed(&script), false).await;
        assert_eq!(item(stream.try_next().await), 1);
        assert_eq!(item(stream.try_next().await), 2);
        end(stream.try_next().await);
        assert_eq!(stream.status().as_u16(), 200);
        assert_eq!(stream.headers().get("x-trace"), Some("t1"));
    }
}

// Row 2 ------------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c11_13_row_02_skip_yields_nothing_and_the_same_pull_reads_on() {
    for reader in BOTH {
        let script = Script::new();
        answer(&script, &[reader.records(&[PING, PING, ITEM_1])]);
        let mut stream = reader.open(&feed(&script), false).await;
        let before = calls();
        let waker = Waker::noop();
        let mut cx = Context::from_waker(waker);
        // One pull: two skipped records and the item.
        match poll(&mut stream, &mut cx) {
            Poll::Ready(Some(Ok(1))) => {}
            other => panic!(
                "expected the item in one pull, got {:?}",
                other.map(|o| o.map(|r| r.map_err(|e| e.to_string())))
            ),
        }
        assert_eq!(calls() - before, 3);
        end(stream.try_next().await);
    }
}

// Row 3 ------------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c11_13_row_03_skips_spend_one_budget_per_pull_and_the_pull_yields_and_goes_on() {
    for reader in BOTH {
        // The byte budget: 2,000 skipped records in one chunk, more than 16 KiB.
        let mut many = Vec::new();
        for _ in 0..2_000 {
            many.extend(reader.record(PING));
        }
        many.extend(reader.record(ITEM_1));
        assert!(many.len() > 16_384);
        // The chunk budget: 100 chunks of one skipped record each.
        let mut chunks: Vec<Vec<u8>> = (0..100).map(|_| reader.record(PING)).collect();
        chunks.push(reader.record(ITEM_2));
        for (pieces, expected, records) in [(vec![many], 1, 2_001), (chunks, 2, 101)] {
            let script = Script::new();
            answer(&script, &pieces);
            let mut stream = reader.open(&feed(&script), false).await;
            let count = Arc::new(Count(AtomicUsize::new(0)));
            let waker = Waker::from(count.clone());
            let mut cx = Context::from_waker(&waker);
            let before = calls();
            let mut pending = 0;
            let value = loop {
                match poll(&mut stream, &mut cx) {
                    Poll::Pending => {
                        pending += 1;
                        // The pull woke its task before it yielded.
                        assert_eq!(count.0.load(Ordering::SeqCst), pending);
                    }
                    Poll::Ready(Some(Ok(value))) => break value,
                    Poll::Ready(other) => panic!(
                        "expected the item, got {:?}",
                        other.map(|r| r.map_err(|e| e.to_string()))
                    ),
                }
            };
            assert_eq!(value, expected);
            assert!(pending >= 1, "{reader:?}: the budget was never spent");
            // Every record was mapped once, across the pulls.
            assert_eq!(calls() - before, records);
        }
    }
}

/// Skipped records, then the item last, in one chunk of exactly `total` bytes.
fn exactly(reader: Reader, total: usize) -> Vec<u8> {
    let ping = reader.record(PING);
    let item = reader.record(ITEM_1);
    let mut body = Vec::new();
    let mut left = total - item.len();
    while left >= 2 * ping.len() {
        body.extend_from_slice(&ping);
        left -= ping.len();
    }
    // The last skipped record fills what is left, with spaces inside its JSON.
    let padded = format!("{{\"kind\":\"ping\"{}}}", " ".repeat(left - ping.len()));
    body.extend(reader.record(&padded));
    body.extend(item);
    assert_eq!(body.len(), total);
    body
}

/// The first poll of the first item of this stream, with a waker that does nothing.
fn first_poll(
    stream: &mut RecordStream<u32, feed::Error>,
) -> Poll<Option<Result<u32, feed::Error>>> {
    let waker = Waker::noop();
    let mut cx = Context::from_waker(waker);
    poll(stream, &mut cx)
}

#[tokio::test(start_paused = true)]
async fn c11_13_row_03_the_byte_budget_of_a_poll_is_exactly_16_kib() {
    for reader in BOTH {
        for (total, ready) in [(16_384, true), (16_385, false)] {
            let script = Script::new();
            answer(&script, &[exactly(reader, total)]);
            let mut stream = reader.open(&feed(&script), false).await;
            match first_poll(&mut stream) {
                Poll::Ready(Some(Ok(1))) if ready => {}
                Poll::Pending if !ready => assert_eq!(item(stream.try_next().await), 1),
                other => panic!(
                    "{reader:?}, {total} bytes: {:?}",
                    other.map(|o| o.map(|r| r.map_err(|e| e.to_string())))
                ),
            }
            end(stream.try_next().await);
        }
    }
}

#[tokio::test(start_paused = true)]
async fn c11_13_row_03_the_chunk_budget_of_a_poll_is_exactly_64_chunks() {
    for reader in BOTH {
        for (chunks, ready) in [(64, true), (65, false)] {
            let mut pieces: Vec<Vec<u8>> = (1..chunks).map(|_| reader.record(PING)).collect();
            pieces.push(reader.record(ITEM_1));
            let script = Script::new();
            answer(&script, &pieces);
            let mut stream = reader.open(&feed(&script), false).await;
            match first_poll(&mut stream) {
                Poll::Ready(Some(Ok(1))) if ready => {}
                Poll::Pending if !ready => assert_eq!(item(stream.try_next().await), 1),
                other => panic!(
                    "{reader:?}, {chunks} chunks: {:?}",
                    other.map(|o| o.map(|r| r.map_err(|e| e.to_string())))
                ),
            }
            end(stream.try_next().await);
        }
    }
}

#[tokio::test(start_paused = true)]
async fn c11_13_row_01_a_crlf_cut_between_its_two_bytes_is_one_line_end() {
    // An event of two data lines whose first CRLF is cut between the CR and the LF: the LF
    // that opens the next chunk ends the same line, and no blank line dispatches the event.
    let script = Script::new();
    answer(
        &script,
        &[
            b"data: {\"kind\":\"item\",\r".to_vec(),
            b"\ndata: \"n\":7}\r\n\r\n".to_vec(),
        ],
    );
    let mut stream = Reader::Sse.open(&feed(&script), false).await;
    assert_eq!(item(stream.try_next().await), 7);
    end(stream.try_next().await);
    // A JSON line whose CRLF is cut the same way.
    let script = Script::new();
    answer(
        &script,
        &[
            b"{\"kind\":\"item\",\"n\":7}\r".to_vec(),
            b"\n{\"kind\":\"item\",\"n\":8}\r\n".to_vec(),
        ],
    );
    let mut stream = Reader::Lines.open(&feed(&script), false).await;
    assert_eq!(item(stream.try_next().await), 7);
    assert_eq!(item(stream.try_next().await), 8);
    end(stream.try_next().await);
}

// Row 4 ------------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c11_13_row_04_input_that_never_pauses_is_ended_by_the_call_deadline_when_a_pull_yields() {
    for reader in BOTH {
        let mut pings = reader.record(ITEM_1);
        for _ in 0..20_000 {
            pings.extend(reader.record(PING));
        }
        let script = Script::new();
        answer(&script, &[pings]);
        let client = feed(&script);
        let start = Instant::now();
        let mut stream = success(match reader {
            Reader::Sse => client.sse_body().with_timeout(secs(60)).await,
            Reader::Lines => client.lines_body().with_timeout(secs(60)).await,
        });
        assert_eq!(item(stream.try_next().await), 1);
        let waker = Waker::noop();
        let mut cx = Context::from_waker(waker);
        // The bytes are always ready: the pull spends its budget and yields.
        assert!(poll(&mut stream, &mut cx).is_pending());
        // The deadline passes while no read waits for bytes.
        tokio::time::advance(secs(61)).await;
        let error = match poll(&mut stream, &mut cx) {
            Poll::Ready(Some(Err(error))) => error,
            other => panic!(
                "expected the deadline, got {:?}",
                other.map(|o| o.map(|r| r.map_err(|e| e.to_string())))
            ),
        };
        assert_eq!(error.kind(), Kind::Transport);
        assert!(error.is_timeout());
        assert_eq!(
            error.to_string(),
            format!(
                "transport error: the call took longer than 60 s (with_timeout), in {}",
                reader.endpoint(false)
            )
        );
        assert!(start.elapsed() >= secs(61));
        end(stream.try_next().await);
    }
}

// Rows 5 to 7 ------------------------------------------------------------------------------

/// A terminal record, invalid bytes behind it in the same chunk, and a body that would break.
fn terminal(reader: Reader, marker: &str) -> Vec<Vec<u8>> {
    let mut chunk = reader.records(&[ITEM_1, marker]);
    chunk.extend_from_slice(b"\xff\xfe not a record\n\n");
    vec![chunk]
}

#[tokio::test(start_paused = true)]
async fn c11_13_row_05_last_yields_its_value_and_releases_the_body_before_it_returns() {
    for reader in BOTH {
        for last in [false, true] {
            let script = Script::new();
            let client = limited(&script);
            {
                let mut answer = script.answer(200);
                for piece in terminal(reader, r#"{"kind":"last","n":9}"#) {
                    answer = answer.body(piece);
                }
                answer.cut();
            }
            let mut stream = reader.open(&client, last).await;
            assert_eq!(item(stream.try_next().await), 1);
            let before = calls();
            assert_eq!(item(stream.try_next().await), 9);
            assert_eq!(calls() - before, 1);
            // Released at the last record, while the stream is still held.
            place_is_free(&client, &script, reader).await;
            end(stream.try_next().await);
            end(stream.try_next().await);
            assert_eq!(
                calls() - before,
                2,
                "only the item of the second call was mapped"
            );
        }
    }
}

#[tokio::test(start_paused = true)]
async fn c11_13_row_06_end_yields_nothing_and_releases_the_body() {
    for reader in BOTH {
        for last in [false, true] {
            let script = Script::new();
            let client = limited(&script);
            {
                let mut answer = script.answer(200);
                for piece in terminal(reader, r#"{"kind":"end"}"#) {
                    answer = answer.body(piece);
                }
                answer.cut();
            }
            let mut stream = reader.open(&client, last).await;
            assert_eq!(item(stream.try_next().await), 1);
            let before = calls();
            end(stream.try_next().await);
            assert_eq!(calls() - before, 1);
            place_is_free(&client, &script, reader).await;
            end(stream.try_next().await);
        }
    }
}

#[tokio::test(start_paused = true)]
async fn c11_13_row_07_bytes_after_a_terminal_record_are_never_read() {
    for reader in BOTH {
        for marker in [r#"{"kind":"last","n":9}"#, r#"{"kind":"end"}"#] {
            let script = Script::new();
            {
                let mut answer = script.answer(200);
                for piece in terminal(reader, marker) {
                    answer = answer.body(piece);
                }
                // A body that would break if it were read again.
                answer.cut();
            }
            let mut stream = reader.open(&feed(&script), true).await;
            let mut seen = Vec::new();
            while let Some(value) = stream
                .try_next()
                .await
                .expect("no error: nothing behind the marker is read")
            {
                seen.push(value);
            }
            assert_eq!(
                seen,
                if marker.contains("last") {
                    vec![1, 9]
                } else {
                    vec![1]
                }
            );
            end(stream.try_next().await);
        }
    }
}

// Row 8 ------------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c11_13_row_08_a_problem_is_a_refused_error_with_the_problem_and_the_body_is_released() {
    for reader in BOTH {
        let script = Script::new();
        let client = limited(&script);
        answer(
            &script,
            &[reader.records(&[ITEM_1, r#"{"kind":"error","code":410}"#, ITEM_2])],
        );
        let mut stream = reader.open(&client, false).await;
        assert_eq!(item(stream.try_next().await), 1);
        let error = failed(stream.try_next().await);
        assert_eq!(error.kind(), Kind::Refused);
        assert_eq!(
            error.to_string(),
            format!("refused: status 200, in {}", reader.endpoint(false))
        );
        assert_eq!(error.problem(), Some(&Problem { code: 410 }));
        assert!(!error.is_status(410));
        assert!(error.reason().is_none());
        assert!(error.source().is_none());
        let details = error.details().to_string();
        assert!(
            details.contains("problem: Problem { code: 410 }"),
            "{details}"
        );
        assert!(!format!("{error:?}").contains("410"));
        of_the_response(&error, reader.endpoint(false));
        place_is_free(&client, &script, reader).await;
        end(stream.try_next().await);
    }
}

// Row 9 ------------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c11_13_row_09_a_rejection_is_a_mismatch_with_its_fixed_reason_outside_the_text() {
    for reader in BOTH {
        let script = Script::new();
        let client = limited(&script);
        answer(
            &script,
            &[reader.records(&[ITEM_1, r#"{"kind":"reject"}"#])],
        );
        let mut stream = reader.open(&client, false).await;
        assert_eq!(item(stream.try_next().await), 1);
        let error = failed(stream.try_next().await);
        record_mismatch(
            &error,
            reader.endpoint(false),
            "record 2 does not fit the stream",
        );
        assert_eq!(error.reason(), Some("a rejected record"));
        assert!(error
            .details()
            .to_string()
            .contains("reason: a rejected record"));
        assert!(!format!("{error:?}").contains("rejected"));
        place_is_free(&client, &script, reader).await;
        end(stream.try_next().await);
    }
}

// Row 10 -----------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c11_13_row_10_a_record_that_does_not_decode_names_its_place_in_details_only() {
    // A syntax fault has a place; a fault inside a tagged enum, which `serde` decodes from a
    // buffer, has none.
    let syntax = r#"{"kind":"item","n":1,}"#;
    let place = serde_json::from_str::<serde_json::Value>(syntax).unwrap_err();
    let expected = format!("reader: line {}, column {}", place.line(), place.column());
    for reader in BOTH {
        for (record, details) in [
            (syntax, Some(expected.as_str())),
            (r#"{"kind":"item","n":"x"}"#, None),
        ] {
            let script = Script::new();
            let client = limited(&script);
            answer(&script, &[reader.records(&[ITEM_1, record])]);
            let mut stream = reader.open(&client, false).await;
            assert_eq!(item(stream.try_next().await), 1);
            let error = failed(stream.try_next().await);
            record_mismatch(
                &error,
                reader.endpoint(false),
                "record 2 could not be decoded",
            );
            assert!(error.reason().is_none());
            let text = error.details().to_string();
            match details {
                Some(details) => assert!(text.contains(details), "{text}"),
                None => assert!(!text.contains("reader:"), "{text}"),
            }
            assert!(!text.contains("\"x\"") && !text.contains("item"), "{text}");
            place_is_free(&client, &script, reader).await;
            end(stream.try_next().await);
        }
    }
}

// Row 11 -----------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c11_13_row_11_a_failure_of_another_body_returned_by_a_mapping_keeps_nothing() {
    for reader in BOTH {
        // Another response, whose body breaks: its failure is a `RecordError::Stream`.
        let script = Script::new();
        script
            .answer(201)
            .header("x-trace", "other")
            .body(b"data: a".to_vec())
            .cut();
        let mut other = success(feed(&script).raw().await);
        let broken = other.try_next().await.unwrap_err();
        assert!(matches!(broken, RecordError::Stream(_)));
        *FOREIGN.lock().unwrap() = Some(broken);

        answer(
            &script,
            &[reader.records(&[ITEM_1, r#"{"kind":"foreign_stream"}"#])],
        );
        let mut stream = reader.open(&feed(&script), false).await;
        assert_eq!(item(stream.try_next().await), 1);
        let error = failed(stream.try_next().await);
        // Not the other body's status, kind or cause: the generic Mismatch of this body.
        record_mismatch(
            &error,
            reader.endpoint(false),
            "record 2 does not fit the stream",
        );
        assert!(error.reason().is_none());
        assert!(!error.details().to_string().contains("reader:"));
        end(stream.try_next().await);
    }
}

#[tokio::test(start_paused = true)]
async fn c11_13_row_11_a_decode_failure_of_other_bytes_keeps_only_its_coordinates() {
    for reader in BOTH {
        let script = Script::new();
        answer(
            &script,
            &[reader.records(&[r#"{"kind":"foreign_decode"}"#])],
        );
        let mut stream = reader.open(&feed(&script), false).await;
        let error = failed(stream.try_next().await);
        record_mismatch(
            &error,
            reader.endpoint(false),
            "record 1 could not be decoded",
        );
        let place = serde_json::from_str::<u32>("{\"x\": oops}").unwrap_err();
        let details = error.details().to_string();
        let expected = format!("reader: line {}, column {}", place.line(), place.column());
        assert!(details.contains(&expected), "{details}");
        assert!(!details.contains("oops"), "{details}");
        end(stream.try_next().await);
    }
}

// Row 12 -----------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c11_13_row_12_an_empty_line_or_a_line_that_is_not_json_does_not_decode() {
    for (body, record) in [
        (&b"\n"[..], 1),
        (&b"{\"kind\":\"item\",\"n\":1}\nnot json\n"[..], 2),
    ] {
        let script = Script::new();
        answer(&script, &[body.to_vec()]);
        let mut stream = Reader::Lines.open(&feed(&script), false).await;
        if record == 2 {
            assert_eq!(item(stream.try_next().await), 1);
        }
        let error = failed(stream.try_next().await);
        record_mismatch(
            &error,
            "feed.lines_body",
            &format!("record {record} could not be decoded"),
        );
        end(stream.try_next().await);
    }
    // The data of an event that is not JSON.
    let script = Script::new();
    answer(&script, &[b"data: not json\n\n".to_vec()]);
    let mut stream = Reader::Sse.open(&feed(&script), false).await;
    let error = failed(stream.try_next().await);
    record_mismatch(&error, "feed.sse_body", "record 1 could not be decoded");
}

#[tokio::test(start_paused = true)]
async fn c11_13_row_12_a_mapping_that_only_reads_the_text_accepts_a_line_that_is_not_json() {
    let script = Script::new();
    answer(&script, &[b"not json\n\n{\"a\": 1}\r\n".to_vec()]);
    let mut lines = success(feed(&script).lines_text().await);
    assert_eq!(item(lines.try_next().await), "not json");
    assert_eq!(item(lines.try_next().await), "");
    assert_eq!(item(lines.try_next().await), "{\"a\": 1}");
    end(lines.try_next().await);

    answer(&script, &[b"data: not json\n\n".to_vec()]);
    let mut events = success(feed(&script).sse_text().await);
    assert_eq!(item(events.try_next().await), "not json");
    end(events.try_next().await);
}

// Row 13 -----------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c11_13_row_13_a_record_of_exactly_the_bound_is_read_and_one_byte_more_is_refused() {
    for reader in BOTH {
        let script = Script::new();
        let client = limited(&script);
        answer(
            &script,
            &[reader.records(&[ITEM_1, r#"{"kind":"item","n":10}"#])],
        );
        let mut stream = success(match reader {
            Reader::Sse => client.sse_small().await,
            Reader::Lines => client.lines_small().await,
        });
        let before = calls();
        assert_eq!(item(stream.try_next().await), 1);
        let endpoint = match reader {
            Reader::Sse => "feed.sse_small",
            Reader::Lines => "feed.lines_small",
        };
        let error = failed(stream.try_next().await);
        record_mismatch(&error, endpoint, "record 2 exceeds the reader's size limit");
        assert!(error.reason().is_none());
        assert_eq!(
            calls() - before,
            1,
            "the record over the bound was not mapped"
        );
        place_is_free(&client, &script, reader).await;
        end(stream.try_next().await);
    }
}

// Row 14 -----------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c11_13_row_14_a_record_that_is_not_utf8_is_refused_before_the_mapping() {
    for reader in BOTH {
        let script = Script::new();
        let mut body = reader.record(ITEM_1);
        body.extend(match reader {
            Reader::Sse => b"data: \xff\n\n".to_vec(),
            Reader::Lines => b"\"\xff\"\n".to_vec(),
        });
        answer(&script, &[body]);
        let mut stream = reader.open(&feed(&script), false).await;
        let before = calls();
        assert_eq!(item(stream.try_next().await), 1);
        let error = failed(stream.try_next().await);
        record_mismatch(&error, reader.endpoint(false), "record 2 is not UTF-8");
        assert_eq!(calls() - before, 1);
        end(stream.try_next().await);
    }
}

// Rows 15 and 16 ---------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c11_13_row_15_until_end_of_body_ends_at_a_clean_end_also_of_an_empty_body() {
    for reader in BOTH {
        let script = Script::new();
        let client = limited(&script);
        answer(&script, &[reader.records(&[ITEM_1])]);
        let mut stream = reader.open(&client, false).await;
        assert_eq!(item(stream.try_next().await), 1);
        end(stream.try_next().await);
        place_is_free(&client, &script, reader).await;

        let script = Script::new();
        answer(&script, &[Vec::new()]);
        let mut empty = reader.open(&feed(&script), false).await;
        end(empty.try_next().await);
    }
    // An event block that the body ends before its blank line is dropped.
    let script = Script::new();
    answer(&script, &[b"data: {\"kind\":\"item\",\"n\":1}\n".to_vec()]);
    let mut stream = Reader::Sse.open(&feed(&script), false).await;
    end(stream.try_next().await);
    // A last JSON line without LF is a record.
    let script = Script::new();
    answer(&script, &[ITEM_1.as_bytes().to_vec()]);
    let mut stream = Reader::Lines.open(&feed(&script), false).await;
    assert_eq!(item(stream.try_next().await), 1);
    end(stream.try_next().await);
}

#[tokio::test(start_paused = true)]
async fn c11_13_row_16_until_last_record_refuses_a_body_that_ends_before_its_last_record() {
    for reader in BOTH {
        for body in [reader.records(&[ITEM_1]), Vec::new()] {
            let script = Script::new();
            let client = limited(&script);
            let records = usize::from(!body.is_empty());
            answer(&script, &[body]);
            let mut stream = reader.open(&client, true).await;
            if records == 1 {
                assert_eq!(item(stream.try_next().await), 1);
            }
            let error = failed(stream.try_next().await);
            record_mismatch(
                &error,
                reader.endpoint(true),
                "the body ended before its last record",
            );
            assert!(error.reason().is_none());
            place_is_free(&client, &script, reader).await;
            end(stream.try_next().await);
        }
    }
}

#[tokio::test(start_paused = true)]
async fn c11_13_row_16_each_ending_accepts_last_and_end() {
    for reader in BOTH {
        for last in [false, true] {
            for (marker, values) in [
                (r#"{"kind":"last","n":9}"#, vec![1, 9]),
                (r#"{"kind":"end"}"#, vec![1]),
            ] {
                let script = Script::new();
                answer(&script, &[reader.records(&[ITEM_1, marker])]);
                let mut stream = reader.open(&feed(&script), last).await;
                let mut seen = Vec::new();
                while let Some(value) = stream.try_next().await.expect("an ending, not an error") {
                    seen.push(value);
                }
                assert_eq!(seen, values, "{reader:?}, last record required: {last}");
            }
        }
    }
}

// Rows 17 to 19 ----------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c11_13_row_17_a_body_that_breaks_is_the_cores_transport_error() {
    for reader in BOTH {
        let script = Script::new();
        script
            .answer(200)
            .body(reader.record(ITEM_1))
            .body(b"{\"kind\"".to_vec())
            .cut();
        let mut stream = reader.open(&feed(&script), false).await;
        assert_eq!(item(stream.try_next().await), 1);
        let error = failed(stream.try_next().await);
        assert_eq!(error.kind(), Kind::Transport);
        assert_eq!(
            error.to_string(),
            format!(
                "transport error: the response body could not be read after status 200, in {}",
                reader.endpoint(false)
            )
        );
        end(stream.try_next().await);
    }
}

#[tokio::test(start_paused = true)]
async fn c11_13_row_17_a_body_one_byte_short_of_its_length_is_the_cores_transport_error() {
    for reader in BOTH {
        // A transport that passes the body as it is, with its `content-length`: only the
        // reader can see that the body ended one byte short.
        let body = reader.records(&[ITEM_1, ITEM_2]);
        let length = (body.len() + 1).to_string();
        let short = concord::fake(move |_| {
            Ok(concord::reply_with(
                200,
                &[("content-length", length.as_str()), ("x-trace", "t1")],
                body.clone(),
            ))
        });
        let mut stream = reader.open(&Feed::new().with_transport(short), false).await;
        assert_eq!(item(stream.try_next().await), 1);
        assert_eq!(item(stream.try_next().await), 2);
        // A clean end under `until_end_of_body`, but one byte is missing: not the end.
        let error = failed(stream.try_next().await);
        assert_eq!(error.kind(), Kind::Transport);
        assert_eq!(
            error.to_string(),
            format!(
                "transport error: the response body could not be read after status 200, in {}",
                reader.endpoint(false)
            )
        );
        end(stream.try_next().await);
    }
}

#[tokio::test(start_paused = true)]
async fn c11_13_row_18_silence_for_the_idle_timeout_is_a_timeout() {
    for reader in BOTH {
        let script = Script::new();
        script
            .answer(200)
            .body(reader.record(ITEM_1))
            .pause(secs(40))
            .body(reader.record(ITEM_2));
        let mut stream = reader.open(&feed(&script), false).await;
        assert_eq!(item(stream.try_next().await), 1);
        let start = Instant::now();
        let error = failed(stream.try_next().await);
        assert_eq!(start.elapsed(), secs(30));
        assert!(error.is_timeout());
        assert_eq!(
            error.to_string(),
            format!("transport error: no body data for 30 s after status 200 (with_idle_timeout changes it), in {}", reader.endpoint(false))
        );
        end(stream.try_next().await);
    }
}

#[tokio::test(start_paused = true)]
async fn c11_13_row_19_the_call_timeout_ends_a_stream_that_waits() {
    for reader in BOTH {
        let script = Script::new();
        script
            .answer(200)
            .body(reader.record(ITEM_1))
            .pause(secs(20))
            .body(reader.record(PING))
            .pause(secs(20))
            .body(reader.record(PING))
            .pause(secs(25))
            .body(reader.record(ITEM_2));
        let client = feed(&script);
        let start = Instant::now();
        let mut stream = success(match reader {
            Reader::Sse => client.sse_body().with_timeout(secs(60)).await,
            Reader::Lines => client.lines_body().with_timeout(secs(60)).await,
        });
        assert_eq!(item(stream.try_next().await), 1);
        let error = failed(stream.try_next().await);
        assert_eq!(start.elapsed(), secs(60));
        assert!(error.is_timeout());
        assert_eq!(
            error.to_string(),
            format!(
                "transport error: the call took longer than 60 s (with_timeout), in {}",
                reader.endpoint(false)
            )
        );
    }
}

#[tokio::test(start_paused = true)]
async fn c11_13_row_19_every_text_names_the_attempts_of_the_call() {
    for reader in BOTH {
        let script = Script::new();
        script.answer(503);
        answer(&script, &[reader.records(&[r#"{"kind":"reject"}"#])]);
        let client = feed(&script).with_retry(Retry::<Problem>::standard());
        let mut stream = reader.open(&client, false).await;
        let error = failed(stream.try_next().await);
        assert_eq!(error.tries(), 2);
        assert_eq!(
            error.to_string(),
            format!(
                "mismatch: status 200, record 1 does not fit the stream, in {}, after 2 attempts",
                reader.endpoint(false)
            )
        );
    }
}

#[tokio::test(start_paused = true)]
async fn c11_13_row_19_a_refusal_names_the_attempts_of_the_call() {
    for reader in BOTH {
        let script = Script::new();
        script.answer(503);
        answer(
            &script,
            &[reader.records(&[r#"{"kind":"error","code":410}"#])],
        );
        let client = feed(&script).with_retry(Retry::<Problem>::standard());
        let mut stream = reader.open(&client, false).await;
        let error = failed(stream.try_next().await);
        assert_eq!(error.kind(), Kind::Refused);
        assert_eq!(error.problem(), Some(&Problem { code: 410 }));
        assert_eq!(error.tries(), 2);
        assert_eq!(
            error.to_string(),
            format!(
                "refused: status 200, in {}, after 2 attempts",
                reader.endpoint(false)
            )
        );
    }
}

// Rows 20 to 22 ----------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c11_13_row_20_dropping_the_stream_releases_the_body_and_its_place() {
    for reader in BOTH {
        let script = Script::new();
        let client = limited(&script);
        script.answer(200).body(reader.record(ITEM_1)).stall();
        let mut stream = reader.open(&client, false).await;
        assert_eq!(item(stream.try_next().await), 1);
        place_is_held(&client, reader).await;
        assert_eq!(script.requests().len(), 1);
        drop(stream);
        place_is_free(&client, &script, reader).await;
    }
}

#[tokio::test(start_paused = true)]
async fn c11_13_row_21_a_dropped_pull_keeps_its_skips_and_its_partial_record_and_stops_the_idle_timer(
) {
    for reader in BOTH {
        let mut first = reader.record(PING);
        let whole = reader.record(r#"{"kind":"item","n":5}"#);
        let (head, tail) = whole.split_at(12);
        first.extend_from_slice(head);
        let script = Script::new();
        script
            .answer(200)
            .body(first)
            .pause(secs(60))
            .body(tail.to_vec());
        let mut stream = reader.open(&feed(&script), false).await;
        let before = calls();
        // A pull that is dropped after the skip and inside the item.
        assert!(tokio::time::timeout(secs(5), stream.try_next())
            .await
            .is_err());
        assert_eq!(calls() - before, 1);
        // Longer than the idle timeout since the first pull: the dropped pull's timer stopped.
        tokio::time::sleep(secs(40)).await;
        let start = Instant::now();
        assert_eq!(item(stream.try_next().await), 5);
        assert_eq!(start.elapsed(), secs(15));
        assert_eq!(calls() - before, 2, "each record mapped once");
        end(stream.try_next().await);
    }
}

#[tokio::test(start_paused = true)]
async fn c11_13_row_22_a_panic_of_the_mapping_releases_the_body_as_it_unwinds() {
    for reader in BOTH {
        let script = Script::new();
        let client = limited(&script);
        script
            .answer(200)
            .body(reader.records(&[ITEM_1, r#"{"kind":"panic"}"#, ITEM_2]))
            .stall();
        let mut stream = reader.open(&client, false).await;
        assert_eq!(item(stream.try_next().await), 1);
        let waker = Waker::noop();
        let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut cx = Context::from_waker(waker);
            let _ = poll(&mut stream, &mut cx);
        }));
        assert!(unwound.is_err());
        // The stream is still held, and its place is free.
        place_is_free(&client, &script, reader).await;
        end(stream.try_next().await);
        assert_eq!(stream.status().as_u16(), 200);
    }
}

// Row 23 -----------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c11_13_row_23_event_blocks_that_dispatch_nothing_count_in_the_position() {
    let script = Script::new();
    answer(
        &script,
        &[b": a comment\n\ndata: {\"kind\":\"item\",\"n\":1}\n\nid: 7\n\nretry: 10\n\ndata: {\"kind\":\"reject\"}\n\n".to_vec()],
    );
    let mut stream = Reader::Sse.open(&feed(&script), false).await;
    assert_eq!(item(stream.try_next().await), 1);
    let error = failed(stream.try_next().await);
    record_mismatch(&error, "feed.sse_body", "record 5 does not fit the stream");
    // In JSON Lines every line is a record.
    let script = Script::new();
    answer(
        &script,
        &[Reader::Lines.records(&[PING, ITEM_1, PING, r#"{"kind":"reject"}"#])],
    );
    let mut stream = Reader::Lines.open(&feed(&script), false).await;
    assert_eq!(item(stream.try_next().await), 1);
    let error = failed(stream.try_next().await);
    record_mismatch(
        &error,
        "feed.lines_body",
        "record 4 does not fit the stream",
    );
}

// A reader of the author's own ---------------------------------------------------------------

/// The body of a response, handed to the author's own framer as records.
pub struct OwnRecords(concord::read::RecordBody<Problem>);

impl concord::error::CarriesProblem for OwnRecords {
    type Problem = Problem;
}

/// A reader of the author's own, which frames nothing: the test makes the errors itself.
pub struct OwnReader;

impl concord::Reader<OwnRecords> for OwnReader {
    async fn read(
        &self,
        body: concord::read::Incoming,
    ) -> Result<OwnRecords, concord::read::ReadError> {
        Ok(OwnRecords(body.stream().into_records()))
    }
}

concord::api! {
    client Own {
        base "https://own.example.test";
        problem Problem read concord::Json;

        GET records() -> OwnRecords { path "o"; read OwnReader; }
    }
}

#[tokio::test(start_paused = true)]
async fn c11_13_a_reader_of_the_authors_own_keeps_a_place_only_for_a_record_that_does_not_decode() {
    use concord::read::RecordFault;
    let cases = [
        (
            RecordFault::too_large(),
            "record 1 exceeds the reader's size limit",
            false,
        ),
        (RecordFault::not_utf8(), "record 1 is not UTF-8", false),
        (
            RecordFault::does_not_fit(),
            "record 1 does not fit the stream",
            false,
        ),
        (
            RecordFault::not_decoded(),
            "record 1 could not be decoded",
            true,
        ),
    ];
    for (fault, words, placed) in cases {
        let script = Script::new();
        answer(&script, &[b"x\n".to_vec()]);
        let own = Own::new().with_transport(script.clone());
        let OwnRecords(mut body) = success(own.records().await);
        body.start_record();
        let error = body.fail(fault.at(3, 4));
        record_mismatch(&error, "own.records", words);
        let details = error.details().to_string();
        assert_eq!(
            details.contains("line 3, column 4"),
            placed,
            "{words}: {details}"
        );
    }
}

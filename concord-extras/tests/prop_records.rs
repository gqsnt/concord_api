//! C11.8 and C11.9: the readers of records against a model of their contracts, for every
//! input and every way of cutting it into chunks, with the work and the memory they spend.
#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]

use concord_core::bytes::Bytes;
use concord_core::read::{Incoming, Reader};
use concord_core::reqwest;
#[cfg(feature = "json")]
use concord_extras::read::JsonLines;
use concord_extras::read::{RecordError, RecordStream, Sse, SseEvent};
use futures_core::Stream;
use futures_util::{FutureExt as _, StreamExt as _};
use proptest::prelude::*;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
use std::time::Duration;

#[path = "corpus/read.rs"]
mod corpus;

/// Chunks alive at once: each chunk the body makes is counted until the reader drops it.
#[derive(Clone, Default)]
struct Alive {
    now: Arc<AtomicUsize>,
    most: Arc<AtomicUsize>,
}
struct Owned(Vec<u8>, Alive);
impl AsRef<[u8]> for Owned {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
impl Drop for Owned {
    fn drop(&mut self) {
        self.1.now.fetch_sub(1, Ordering::SeqCst);
    }
}
impl Alive {
    fn chunk(&self, bytes: Vec<u8>) -> Bytes {
        let now = self.now.fetch_add(1, Ordering::SeqCst) + 1;
        self.most.fetch_max(now, Ordering::SeqCst);
        Bytes::from_owner(Owned(bytes, self.clone()))
    }
    fn most(&self) -> usize {
        self.most.load(Ordering::SeqCst)
    }
}

const BOM: &[u8] = b"\xef\xbb\xbf";

/// A body whose chunks are made only when the reader asks for them, counted.
fn body(
    chunks: impl Iterator<Item = Vec<u8>> + Send + 'static,
    pulled: Arc<AtomicUsize>,
) -> Incoming {
    body_alive(chunks, pulled, Alive::default())
}
fn body_alive(
    chunks: impl Iterator<Item = Vec<u8>> + Send + 'static,
    pulled: Arc<AtomicUsize>,
    alive: Alive,
) -> Incoming {
    let source = futures_util::stream::iter(chunks).map(move |chunk| {
        pulled.fetch_add(1, Ordering::SeqCst);
        Ok::<_, std::io::Error>(alive.chunk(chunk))
    });
    let response = concord_core::http::Response::new(reqwest::Body::wrap_stream(source));
    Incoming::new(reqwest::Response::from(response))
}

/// Cuts `input` at the given sizes; what is left after the last size is one more chunk.
fn cut(input: &[u8], sizes: &[usize]) -> Vec<Vec<u8>> {
    let mut chunks = Vec::new();
    let mut rest = input;
    for size in sizes {
        let (head, tail) = rest.split_at((*size).min(rest.len()));
        chunks.push(head.to_vec());
        rest = tail;
    }
    if !rest.is_empty() {
        chunks.push(rest.to_vec());
    }
    chunks
}

#[derive(Default)]
struct Wakes(AtomicUsize);
impl Wake for Wakes {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

/// What reading a whole body cost: polls, polls that yielded, and records.
#[derive(Debug, Default)]
struct Cost {
    polls: usize,
    yields: usize,
}

/// Polls the stream to its end on this thread. Every `Pending` must come with a wake-up: the
/// body is always ready, so a `Pending` is the reader yielding its budget, never a wait.
fn drain<T>(
    mut records: RecordStream<T>,
    mut each: impl FnMut(Result<T, RecordError>),
    most_polls: usize,
) -> Cost {
    let wakes = Arc::new(Wakes::default());
    let waker = Waker::from(wakes.clone());
    let mut cx = Context::from_waker(&waker);
    let mut cost = Cost::default();
    let mut ended = false;
    loop {
        assert!(cost.polls < most_polls, "more than {most_polls} polls");
        cost.polls += 1;
        let before = wakes.0.load(Ordering::SeqCst);
        match Pin::new(&mut records).poll_next(&mut cx) {
            Poll::Ready(None) => break,
            Poll::Ready(Some(item)) => {
                assert!(!ended, "a record after the first error");
                ended = item.is_err();
                each(item);
            }
            Poll::Pending => {
                assert!(
                    wakes.0.load(Ordering::SeqCst) > before,
                    "pending on a ready body without a wake-up"
                );
                cost.yields += 1;
            }
        }
    }
    // The stream stays ended.
    assert!(matches!(
        Pin::new(&mut records).poll_next(&mut cx),
        Poll::Ready(None)
    ));
    cost
}

/// The most polls a body can take: one per record and one for the end, and one each time the
/// reader spends its budget of 16 KiB or of 64 chunks, the end of the body counted as a chunk.
fn most_polls(records: usize, bytes: usize, chunks: usize) -> usize {
    records + 2 + bytes / 16_384 + (chunks + 1) / 64
}

/// One poll takes at most 64 chunks and 16 KiB: a body read whole took at least that many polls.
fn spent_within_budget(
    cost: &Cost,
    pulled: usize,
    bytes: usize,
    end: &End,
) -> Result<(), TestCaseError> {
    prop_assert!(
        pulled <= 64 * cost.polls,
        "{} chunks in {} polls",
        pulled,
        cost.polls
    );
    if *end == End::Clean {
        prop_assert!(
            bytes <= 16_384 * cost.polls,
            "{} bytes in {} polls",
            bytes,
            cost.polls
        );
    }
    Ok(())
}

/// The end of a stream, as the model and the reader give it.
#[derive(Clone, Debug, PartialEq, Eq)]
enum End {
    Clean,
    TooLarge(usize),
    Utf8(usize),
    Decode(Option<usize>, usize, usize),
    // An error of the body itself, which these bodies never give.
    Other(String),
}
fn end_of(error: &RecordError) -> End {
    match error {
        RecordError::TooLarge { limit, .. } => End::TooLarge(*limit),
        RecordError::Utf8 { record, .. } => End::Utf8(*record),
        RecordError::Decode {
            record,
            line,
            column,
            ..
        } => End::Decode(*record, *line, *column),
        other => End::Other(format!("{other:?}")),
    }
}

/// One event as the model gives it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Event {
    event: String,
    data: String,
    id: String,
    retry: Option<Duration>,
}
fn event_of(event: &SseEvent) -> Event {
    Event {
        event: event.event.clone(),
        data: event.data.clone(),
        id: event.id.clone(),
        retry: event.retry,
    }
}

/// The lines of a body as server-sent events cut them: LF, CRLF or a lone CR. The last item
/// is the unfinished line, without its end, when the body does not end on one.
fn sse_lines(input: &[u8]) -> Vec<(&[u8], bool)> {
    let mut lines = Vec::new();
    let mut at = 0;
    while at < input.len() {
        match input[at..].iter().position(|b| *b == b'\r' || *b == b'\n') {
            Some(i) => {
                lines.push((&input[at..at + i], true));
                at += i + 1;
                if input[at - 1] == b'\r' && input.get(at) == Some(&b'\n') {
                    at += 1;
                }
            }
            None => {
                lines.push((&input[at..], false));
                at = input.len();
            }
        }
    }
    lines
}

/// The events and the end that C11.8 and C11.9 give for a whole body, and the size of the
/// largest event block (lines that are not blank, each with one byte for its end).
fn sse_model(input: &[u8], max: usize) -> (Vec<Event>, End, usize) {
    let input = input.strip_prefix(BOM).unwrap_or(input);
    let (mut events, mut size, mut largest, mut block) = (Vec::new(), 0usize, 0usize, 1usize);
    let (mut data, mut kind) = (String::new(), String::new());
    let (mut id, mut retry) = (String::new(), None);
    for (line, ended) in sse_lines(input) {
        if line.is_empty() {
            // A blank line dispatches; an empty data buffer dispatches nothing.
            block += 1;
            size = 0;
            let kind = std::mem::take(&mut kind);
            if !data.is_empty() {
                data.pop();
                events.push(Event {
                    event: if kind.is_empty() {
                        "message".into()
                    } else {
                        kind
                    },
                    data: std::mem::take(&mut data),
                    id: id.clone(),
                    retry,
                });
            }
            continue;
        }
        size += line.len() + 1;
        largest = largest.max(size);
        if size > max {
            return (events, End::TooLarge(max), largest);
        }
        let Ok(line) = std::str::from_utf8(line) else {
            return (events, End::Utf8(block), largest);
        };
        if line.starts_with(':') {
            continue;
        }
        let (field, value) = line.split_once(':').unwrap_or((line, ""));
        let value = value.strip_prefix(' ').unwrap_or(value);
        match field {
            "data" => {
                data.push_str(value);
                data.push('\n');
            }
            "event" => kind = value.to_owned(),
            "id" if !value.contains('\0') => id = value.to_owned(),
            "retry" if !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) => {
                if let Ok(ms) = value.parse::<u64>() {
                    retry = Some(Duration::from_millis(ms));
                }
            }
            _ => {}
        }
        // An unfinished last line is read, but its event is never dispatched.
        let _ = ended;
    }
    (events, End::Clean, largest)
}

/// The records and the end that C11.8 and C11.9 give for JSON lines.
#[cfg(feature = "json")]
fn json_lines_model(input: &[u8], max: usize) -> (Vec<serde_json::Value>, End, usize) {
    let (mut records, mut largest) = (Vec::new(), 0usize);
    let mut at = 0;
    let mut record = 0;
    while at < input.len() {
        let (line, ended) = match input[at..].iter().position(|b| *b == b'\n') {
            Some(i) => (&input[at..at + i], true),
            None => (&input[at..], false),
        };
        at += line.len() + usize::from(ended);
        record += 1;
        // An empty CRLF line reaches the decoder at every bound, zero included.
        let counted = if ended && line == b"\r" {
            0
        } else {
            line.len()
        };
        largest = largest.max(counted);
        if counted > max {
            return (records, End::TooLarge(max), largest);
        }
        let line = if ended {
            line.strip_suffix(b"\r").unwrap_or(line)
        } else {
            line
        };
        if std::str::from_utf8(line).is_err() {
            return (records, End::Utf8(record), largest);
        }
        match serde_json::from_slice::<serde_json::Value>(line) {
            Ok(value) => records.push(value),
            Err(error) => {
                return (
                    records,
                    End::Decode(Some(record), error.line(), error.column()),
                    largest,
                )
            }
        }
    }
    (records, End::Clean, largest)
}

/// Reads `input` with server-sent events cut at `sizes`, and checks it against the model.
fn check_sse(input: &[u8], sizes: &[usize], max: usize) -> Result<(), TestCaseError> {
    let chunks = cut(input, sizes);
    let count = chunks.len();
    let pulled = Arc::new(AtomicUsize::new(0));
    let records: RecordStream<SseEvent> = Sse::new(max)
        .read(body(chunks.into_iter(), pulled.clone()))
        .now_or_never()
        .unwrap()
        .unwrap();
    let (expected, end, _) = sse_model(input, max);
    let (mut events, mut actual) = (Vec::new(), End::Clean);
    let cost = drain(
        records,
        |item| match item {
            Ok(event) => events.push(event_of(&event)),
            Err(error) => actual = end_of(&error),
        },
        most_polls(expected.len() + 1, input.len(), count),
    );
    prop_assert_eq!(&events, &expected);
    prop_assert_eq!(&actual, &end);
    prop_assert!(pulled.load(Ordering::SeqCst) <= count);
    if end == End::Clean {
        prop_assert_eq!(pulled.load(Ordering::SeqCst), count);
    }
    prop_assert!(cost.polls <= most_polls(events.len(), input.len(), count));
    spent_within_budget(&cost, pulled.load(Ordering::SeqCst), input.len(), &actual)?;
    Ok(())
}

#[cfg(feature = "json")]
fn check_json_lines(input: &[u8], sizes: &[usize], max: usize) -> Result<(), TestCaseError> {
    let chunks = cut(input, sizes);
    let count = chunks.len();
    let pulled = Arc::new(AtomicUsize::new(0));
    let records: RecordStream<serde_json::Value> = JsonLines::new(max)
        .read(body(chunks.into_iter(), pulled.clone()))
        .now_or_never()
        .unwrap()
        .unwrap();
    let (expected, end, _) = json_lines_model(input, max);
    let (mut values, mut actual) = (Vec::new(), End::Clean);
    let cost = drain(
        records,
        |item| match item {
            Ok(value) => values.push(value),
            Err(error) => actual = end_of(&error),
        },
        most_polls(expected.len() + 1, input.len(), count),
    );
    prop_assert_eq!(&values, &expected);
    prop_assert_eq!(&actual, &end);
    prop_assert!(pulled.load(Ordering::SeqCst) <= count);
    prop_assert!(cost.polls <= most_polls(values.len(), input.len(), count));
    spent_within_budget(&cost, pulled.load(Ordering::SeqCst), input.len(), &actual)?;
    Ok(())
}

/// Pieces of an event stream: fields, comments, a BOM or a part of one, bytes that are not
/// UTF-8, text of two to four bytes, NUL, digits of every length, and every line end.
fn sse_piece() -> impl Strategy<Value = Vec<u8>> {
    prop_oneof![
        3 => prop::sample::select(vec![
            "data", "data:", "data: ", "data:x", "data: x", "data:  two spaces", "event: delta",
            "event:", "id: 7", "id", "id: a\0b", "id:", "retry: 1500", "retry:", "retry: 15x",
            "retry: -1", "retry: +5", "retry: 0000000000000000000042",
            "retry: 18446744073709551615", "retry: 18446744073709551616", ": comment", ":",
            "unknown: field", "data: é€😀", "DATA: upper", " data: lead", "data\t: tab",
        ])
        .prop_map(|piece| piece.as_bytes().to_vec()),
        3 => prop::sample::select(vec!["\n", "\r", "\r\n", "\n\n", "\r\r", "\r\n\r\n", "\n\r"])
            .prop_map(|end| end.as_bytes().to_vec()),
        1 => prop::sample::select(vec![
            b"\xef\xbb\xbf".to_vec(),
            b"\xef\xbb".to_vec(),
            b"\xef".to_vec(),
            b"\xff".to_vec(),
            b"\xc3".to_vec(),
            b"\xe2\x82".to_vec(),
            b"\x00".to_vec(),
        ]),
        1 => ("[0-9]{1,40}", prop::sample::select(vec!["retry: ", "retry:", "id: "]))
            .prop_map(|(digits, field)| format!("{field}{digits}").into_bytes()),
        1 => proptest::collection::vec(any::<u8>(), 0..24),
    ]
}

fn sse_input() -> impl Strategy<Value = Vec<u8>> {
    (any::<bool>(), proptest::collection::vec(sse_piece(), 0..40)).prop_map(|(bom, pieces)| {
        let mut input = if bom { BOM.to_vec() } else { Vec::new() };
        for piece in pieces {
            input.extend(piece);
        }
        input
    })
}

#[cfg(feature = "json")]
fn json_piece() -> impl Strategy<Value = Vec<u8>> {
    prop_oneof![
        3 => prop::sample::select(vec![
            "1", "-0", "{\"a\":1}", "\"é\"", "[1,2]", " 2 ", "null", "{", "x", "", "\t", "1 2",
            "\"\\u0000\"", "{\"text\":\"line\\nend\"}", "1e999", "\"€😀\"", "\u{feff}1",
        ])
        .prop_map(|piece| piece.as_bytes().to_vec()),
        3 => prop::sample::select(vec!["\n", "\r\n", "\r", "\n\n", "\r\r\n"])
            .prop_map(|end| end.as_bytes().to_vec()),
        1 => "[0-9+-]{1,40}".prop_map(String::into_bytes),
        1 => prop::sample::select(vec![
            b"\xef\xbb\xbf".to_vec(),
            b"\xff".to_vec(),
            b"\xc3".to_vec(),
            b"\"\xc3\"".to_vec(),
        ]),
        1 => proptest::collection::vec(any::<u8>(), 0..24),
    ]
}

#[cfg(feature = "json")]
fn json_input() -> impl Strategy<Value = Vec<u8>> {
    proptest::collection::vec(json_piece(), 0..40).prop_map(|pieces| pieces.concat())
}

/// Chunk sizes: zero-length chunks, single bytes, and larger pieces.
fn sizes() -> impl Strategy<Value = Vec<usize>> {
    prop_oneof![
        3 => proptest::collection::vec(prop_oneof![Just(0usize), 1usize..4, 1usize..64], 0..48),
        // More chunks than one poll may take, most of them empty.
        1 => proptest::collection::vec(prop_oneof![3 => Just(0usize), 1 => 1usize..3], 64..400),
    ]
}

/// A bound at, just under and just over what the body needs, or any small one.
fn bound(needed: usize, pick: u8, other: usize) -> usize {
    match pick % 4 {
        0 => needed,
        1 => needed.saturating_sub(1),
        2 => needed + 1,
        _ => other,
    }
}

proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    /// Also proves C11.8.
    fn c11_9_sse_frames_every_body_as_the_model_wherever_it_is_cut(
        input in sse_input(), sizes in sizes(), pick in any::<u8>(), other in 0usize..96,
    ) {
        let (_, _, needed) = sse_model(&input, usize::MAX);
        check_sse(&input, &sizes, bound(needed, pick, other))?;
    }

    #[test]
    /// Also proves C11.8.
    fn c11_9_sse_frames_arbitrary_bytes_as_the_model(
        input in proptest::collection::vec(any::<u8>(), 0..512), sizes in sizes(),
        pick in any::<u8>(), other in 0usize..600,
    ) {
        let (_, _, needed) = sse_model(&input, usize::MAX);
        check_sse(&input, &sizes, bound(needed, pick, other))?;
    }

    #[test]
    #[cfg(feature = "json")]
    /// Also proves C11.8.
    fn c11_9_json_lines_frame_every_body_as_the_model_wherever_it_is_cut(
        input in json_input(), sizes in sizes(), pick in any::<u8>(), other in 0usize..64,
    ) {
        let (_, _, needed) = json_lines_model(&input, usize::MAX);
        check_json_lines(&input, &sizes, bound(needed, pick, other))?;
    }

    #[test]
    #[cfg(feature = "json")]
    /// Also proves C11.8.
    fn c11_9_json_lines_frame_arbitrary_bytes_as_the_model(
        input in proptest::collection::vec(any::<u8>(), 0..512), sizes in sizes(),
        pick in any::<u8>(), other in 0usize..600,
    ) {
        let (_, _, needed) = json_lines_model(&input, usize::MAX);
        check_json_lines(&input, &sizes, bound(needed, pick, other))?;
    }

    #[test]
    /// A line end split across two chunks is one line end, whatever the reader.
    fn c11_9_a_line_end_split_across_chunks_is_one_line_end(
        text in "[a-z]{0,8}", max in 0usize..24,
        ends in proptest::collection::vec(prop::sample::select(vec!["\n", "\r", "\r\n"]), 1..6),
    ) {
        let mut input = Vec::new();
        let mut sizes = Vec::new();
        let mut last = 0;
        for end in &ends {
            input.extend_from_slice(format!("data: {text}").as_bytes());
            // Cut between CR and LF, or right after the end.
            let split = input.len() + 1;
            input.extend_from_slice(end.as_bytes());
            input.extend_from_slice(b"\n");
            sizes.push(split - last);
            last = split;
        }
        check_sse(&input, &sizes, max)?;
        #[cfg(feature = "json")]
        check_json_lines(&input, &sizes, max)?;
    }
}

/// The comment lines of a keep-alive, a hundred thousand times, each event block on its own.
fn keep_alive(blocks: usize, size: usize) -> impl Iterator<Item = Vec<u8>> + Send + 'static {
    let block = b": keep-alive\n\n";
    let total = blocks * block.len();
    (0..total.div_ceil(size)).map(move |n| {
        (n * size..((n + 1) * size).min(total))
            .map(|at| block[at % block.len()])
            .collect()
    })
}

#[test]
fn c11_8_a_long_run_of_comment_blocks_reads_in_linear_work_holding_one_chunk() {
    const BLOCKS: usize = 100_000;
    for size in [7, 100, 4096] {
        let total = BLOCKS * 14;
        let chunks = total.div_ceil(size);
        let pulled = Arc::new(AtomicUsize::new(0));
        let alive = Alive::default();
        let records: RecordStream<SseEvent> = Sse::new(64)
            .read(body_alive(
                keep_alive(BLOCKS, size),
                pulled.clone(),
                alive.clone(),
            ))
            .now_or_never()
            .unwrap()
            .unwrap();
        let mut seen = 0;
        let cost = drain(records, |_| seen += 1, most_polls(1, total, chunks));
        assert_eq!(seen, 0, "a comment dispatches nothing");
        assert_eq!(pulled.load(Ordering::SeqCst), chunks);
        // Each chunk is released before the next is asked for: one is alive at a time.
        assert_eq!(alive.most(), 1, "chunks of {size}");
        assert!(cost.polls >= total / 16_384, "the budget was not spent");
    }
}

#[test]
fn c11_8_a_run_of_comment_lines_in_one_block_ends_at_the_bound_and_reads_no_further() {
    // One block that never ends: the bound counts comments, so reading stops at 1,024 bytes.
    let line = b": still here\n";
    let pulled = Arc::new(AtomicUsize::new(0));
    let chunks = (0..1_000_000).map(move |_| line.to_vec());
    let records: RecordStream<SseEvent> = Sse::new(1024)
        .read(body(chunks, pulled.clone()))
        .now_or_never()
        .unwrap()
        .unwrap();
    let mut ends = Vec::new();
    drain(records, |item| ends.push(item.map(|_| ())), 16);
    assert_eq!(ends.len(), 1);
    assert_eq!(end_of(ends[0].as_ref().unwrap_err()), End::TooLarge(1024));
    assert_eq!(pulled.load(Ordering::SeqCst), 1024 / line.len() + 1);
}

#[cfg(feature = "json")]
#[test]
fn c11_8_json_lines_hold_one_chunk_whatever_the_length_of_the_body() {
    // Fifty thousand records of 30 bytes, in chunks of 13: what is held stays near the bound.
    let line = b"{\"n\":12345,\"text\":\"abcdefg\"}\n";
    let total = 50_000 * line.len();
    let chunks = (0..total.div_ceil(13)).map(move |n| {
        (n * 13..((n + 1) * 13).min(total))
            .map(|at| line[at % line.len()])
            .collect::<Vec<u8>>()
    });
    let pulled = Arc::new(AtomicUsize::new(0));
    let alive = Alive::default();
    let records: RecordStream<serde_json::Value> = JsonLines::new(64)
        .read(body_alive(chunks, pulled, alive.clone()))
        .now_or_never()
        .unwrap()
        .unwrap();
    let mut count = 0;
    drain(
        records,
        |item| {
            item.unwrap();
            count += 1;
        },
        most_polls(50_001, total, total.div_ceil(13)),
    );
    assert_eq!(count, 50_000);
    assert_eq!(alive.most(), 1);
}

#[cfg(feature = "json")]
#[test]
fn c11_8_a_line_that_never_ends_stops_at_the_bound_and_reads_no_further() {
    let piece = b"[1,2,3,4,5,6,7,8,9,10,";
    let pulled = Arc::new(AtomicUsize::new(0));
    let chunks = (0..1_000_000).map(move |_| piece.to_vec());
    let records: RecordStream<serde_json::Value> = JsonLines::new(1024)
        .read(body(chunks, pulled.clone()))
        .now_or_never()
        .unwrap()
        .unwrap();
    let mut ends = Vec::new();
    drain(records, |item| ends.push(item.map(|_| ())), 16);
    assert_eq!(ends.len(), 1);
    assert_eq!(end_of(ends[0].as_ref().unwrap_err()), End::TooLarge(1024));
    assert_eq!(pulled.load(Ordering::SeqCst), 1024 / piece.len() + 1);
}

/// Whole, one byte at a time, and cut in two at every byte.
fn every_cut(input: &[u8]) -> Vec<Vec<usize>> {
    let mut cuts = vec![Vec::new(), vec![1; input.len()]];
    cuts.extend((0..=input.len()).map(|at| vec![at]));
    cuts
}

/// `N end`: the number of records, and how the stream ends.
fn expectation(field: &[u8]) -> (usize, End) {
    let (count, end) = corpus::text(field).split_once(' ').unwrap();
    let end = match end.split_once('@') {
        None if end == "clean" => End::Clean,
        None => End::TooLarge(usize::MAX),
        Some(("utf8", n)) => End::Utf8(n.parse().unwrap()),
        Some((_, n)) => End::Decode(Some(n.parse().unwrap()), 0, 0),
    };
    (count.parse().unwrap(), end)
}
fn same_end(actual: &End, expected: &End, max: usize) -> bool {
    match (actual, expected) {
        (End::TooLarge(limit), End::TooLarge(_)) => *limit == max,
        (End::Decode(record, ..), End::Decode(expected, ..)) => record == expected,
        (actual, expected) => actual == expected,
    }
}

#[test]
fn c11_9_the_sse_corpus_reads_as_written_wherever_it_is_cut() {
    for (line, fields) in corpus::cases(include_str!("corpus/sse.txt")) {
        let max: usize = corpus::text(&fields[0]).parse().unwrap();
        let (count, end) = expectation(&fields[1]);
        let input = &fields[2];
        let (events, model_end, _) = sse_model(input, max);
        assert_eq!(events.len(), count, "line {line}");
        assert!(
            same_end(&model_end, &end, max),
            "line {line}: {model_end:?}"
        );
        for sizes in every_cut(input) {
            check_sse(input, &sizes, max).unwrap_or_else(|e| panic!("line {line}: {e}"));
        }
    }
}

#[cfg(feature = "json")]
#[test]
fn c11_9_the_json_lines_corpus_reads_as_written_wherever_it_is_cut() {
    for (line, fields) in corpus::cases(include_str!("corpus/json_lines.txt")) {
        let max: usize = corpus::text(&fields[0]).parse().unwrap();
        let (count, end) = expectation(&fields[1]);
        let input = &fields[2];
        let (records, model_end, _) = json_lines_model(input, max);
        assert_eq!(records.len(), count, "line {line}");
        assert!(
            same_end(&model_end, &end, max),
            "line {line}: {model_end:?}"
        );
        for sizes in every_cut(input) {
            check_json_lines(input, &sizes, max).unwrap_or_else(|e| panic!("line {line}: {e}"));
        }
    }
}

/// The resident memory of this process, from `/proc/self/status`, in KiB.
#[cfg(target_os = "linux")]
fn resident_kib() -> usize {
    let status = std::fs::read_to_string("/proc/self/status").unwrap();
    let line = status
        .lines()
        .find(|line| line.starts_with("VmRSS:"))
        .unwrap();
    line.split_whitespace().nth(1).unwrap().parse().unwrap()
}

/// A body of `total` bytes of one record repeated, made a chunk at a time.
fn repeated(record: &'static [u8], total: usize) -> impl Iterator<Item = Vec<u8>> + Send + 'static {
    (0..total.div_ceil(4096)).map(move |n| {
        (n * 4096..((n + 1) * 4096).min(total))
            .map(|at| record[at % record.len()])
            .collect()
    })
}

/// A reader holds the record it decodes, not the records before it. Without an allocator of
/// its own, the test sees the resident memory of the process on Linux: 32 MiB of records must
/// leave it less than 16 MiB higher. It cannot see a smaller retention, memory that is given
/// back before the body ends, or anything on another system, where it checks only the count.
#[test]
fn c11_8_a_reader_keeps_no_record_after_it_is_returned() {
    const TOTAL: usize = 32 * 1024 * 1024;
    #[cfg(feature = "json")]
    {
        let line: &'static [u8] =
            b"\"abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghij\"\n";
        #[cfg(target_os = "linux")]
        let before = resident_kib();
        let records: RecordStream<serde::de::IgnoredAny> = JsonLines::new(80)
            .read(body(repeated(line, TOTAL), Arc::default()))
            .now_or_never()
            .unwrap()
            .unwrap();
        let mut count = 0;
        drain(
            records,
            |item| count += usize::from(item.is_ok()),
            usize::MAX,
        );
        assert_eq!(count, TOTAL / line.len());
        #[cfg(target_os = "linux")]
        {
            let grown = resident_kib().saturating_sub(before);
            assert!(grown < 16 * 1024, "JSON lines: {grown} KiB more held");
        }
    }
    let event: &'static [u8] = b"data: abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz\n\n";
    #[cfg(target_os = "linux")]
    let before = resident_kib();
    let records: RecordStream<SseEvent> = Sse::new(80)
        .read(body(repeated(event, TOTAL), Arc::default()))
        .now_or_never()
        .unwrap()
        .unwrap();
    let mut count = 0;
    drain(
        records,
        |item| count += usize::from(item.is_ok()),
        usize::MAX,
    );
    assert_eq!(count, TOTAL / event.len());
    #[cfg(target_os = "linux")]
    {
        let grown = resident_kib().saturating_sub(before);
        assert!(grown < 16 * 1024, "events: {grown} KiB more held");
    }
}

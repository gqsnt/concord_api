//! C11.8–C11.10 and C11.13: framing and ending a typed stream are independent of chunk boundaries.

// PANIC-SAFETY: Generated indices and test helpers belong to a test executable; failures report a case.
#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]

use concord::bytes::Bytes;
use concord::error::{Class, Kind};
use concord::http::HeaderMap;
use concord::limit::Limit;
use concord::read::{
    Incoming, JsonLine, JsonLines, Reader, RecordFail, RecordStream, Sse, SseEvent, Step,
};
use concord::Error;
use futures_util::{FutureExt, StreamExt};
use proptest::prelude::*;
use proptest::test_runner::{TestCaseResult, TestRunner};
use std::future::IntoFuture;
use std::io;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const BOUND: usize = 40;
const REASON: &str = "the record was rejected";
const PROBLEM: &str = "the provider refused";

#[derive(Clone, Copy, Debug)]
enum Format {
    Lines,
    Events,
}

#[derive(Clone, Copy, Debug)]
enum Tail {
    Eof,
    Cut,
    Short,
    Idle,
    Deadline,
}

#[derive(Debug, PartialEq, Eq)]
struct Item {
    text: String,
    event: Option<String>,
    id: String,
    retry: Option<Duration>,
}

impl Item {
    fn line(text: &str) -> Self {
        Self {
            text: text.into(),
            event: None,
            id: String::new(),
            retry: None,
        }
    }
}

type Mapped = Result<Step<Item>, RecordFail<String>>;
type Records = RecordStream<Item, Error<String>>;

fn map(
    mut item: Item,
    decode: impl FnOnce() -> Result<String, concord::read::RecordError>,
) -> Mapped {
    Ok(match item.text.as_str() {
        "skip" => Step::Skip,
        "last" => Step::Last(item),
        "end" => Step::End,
        "refuse" => return Err(RecordFail::problem(PROBLEM.into())),
        "reject" => return Err(RecordFail::mismatch(REASON)),
        "foreign" => return Err(unrelated_failure()),
        "panic" => panic!("mapper panicked"),
        _ => {
            if item.text.starts_with(['"', '[', '!']) {
                item.text = decode()?;
            }
            Step::Next(item)
        }
    })
}

fn unrelated_failure() -> RecordFail<String> {
    let body = Incoming::new(concord::reply(200, "too long"));
    let mut other: RecordStream<()> = JsonLines::new(0)
        .read(body)
        .now_or_never()
        .unwrap()
        .unwrap();
    other.try_next().now_or_never().unwrap().unwrap_err().into()
}

fn line(record: JsonLine) -> Mapped {
    map(Item::line(record.text()), || record.json())
}

fn event(record: SseEvent) -> Mapped {
    map(
        Item {
            text: record.data.clone(),
            event: Some(record.event.clone()),
            id: record.id.clone(),
            retry: record.retry,
        },
        || record.json(),
    )
}

concord::api! {
    client Feed {
        base "https://feed.test";
        problem String read concord::Json;
        GET lines() -> Records { read JsonLines::new(BOUND).try_map(line).until_end_of_body(); }
        GET lines_last() -> Records { read JsonLines::new(BOUND).try_map(line).until_last_record(); }
        GET events() -> Records { read Sse::new(BOUND).try_map(event).until_end_of_body(); }
        GET events_last() -> Records { read Sse::new(BOUND).try_map(event).until_last_record(); }
        GET peer();
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Fault {
    Size(usize),
    Utf8(usize),
    Decode(usize, usize, usize),
    Rejected(usize),
    Foreign(usize),
    Refused,
    MissingLast,
    Body,
    Idle,
    Deadline,
}

#[derive(Debug)]
enum Pull {
    Value(Item, bool),
    End,
    Error(Fault),
    Panic,
}

fn dispatch(mut item: Item, number: usize, eof: bool, pulls: &mut Vec<Pull>) -> bool {
    let pull = match item.text.as_str() {
        "skip" => return false,
        "last" => Pull::Value(item, true),
        "end" => Pull::End,
        "refuse" => Pull::Error(Fault::Refused),
        "reject" => Pull::Error(Fault::Rejected(number)),
        "foreign" => Pull::Error(Fault::Foreign(number)),
        "panic" => Pull::Panic,
        _ => {
            if item.text.starts_with(['"', '[', '!']) {
                match serde_json::from_str(&item.text) {
                    Ok(text) => item.text = text,
                    Err(e) => {
                        pulls.push(Pull::Error(Fault::Decode(number, e.line(), e.column())));
                        return true;
                    }
                }
            }
            pulls.push(Pull::Value(item, eof));
            return false;
        }
    };
    pulls.push(pull);
    true
}

/// Work in logical lines from a whole body, keeping the original lengths for the bound.
fn logical_lines(bytes: &[u8], events: bool) -> Vec<(&[u8], bool)> {
    let mut start = 0;
    let mut lines = Vec::new();
    for (index, &byte) in bytes.iter().enumerate() {
        if events && byte == b'\n' && index > 0 && bytes[index - 1] == b'\r' {
            start = index + 1;
        } else if byte == b'\n' || (events && byte == b'\r') {
            lines.push((&bytes[start..index], true));
            start = index + 1;
        }
    }
    if start < bytes.len() {
        lines.push((&bytes[start..], false));
    }
    lines
}

fn expected(format: Format, bytes: &[u8], tail: Tail, require_last: bool) -> Vec<Pull> {
    let events = matches!(format, Format::Events);
    let bytes = if events {
        bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes)
    } else {
        bytes
    };
    let mut pulls = Vec::new();
    let mut number = 1;
    let mut size = 0;
    let mut data = Vec::new();
    let mut name = "";
    let mut id = "";
    let mut retry = None;
    for (raw, terminated) in logical_lines(bytes, events) {
        size += raw.len() + usize::from(events && !raw.is_empty());
        if size > BOUND {
            pulls.push(Pull::Error(Fault::Size(number)));
            return pulls;
        }
        if !terminated && !matches!(tail, Tail::Eof) {
            break;
        }
        let raw = if !events && terminated {
            raw.strip_suffix(b"\r").unwrap_or(raw)
        } else {
            raw
        };
        let Ok(line) = std::str::from_utf8(raw) else {
            pulls.push(Pull::Error(Fault::Utf8(number)));
            return pulls;
        };
        if !events {
            if dispatch(Item::line(line), number, !terminated, &mut pulls) {
                return pulls;
            }
            number += 1;
            size = 0;
        } else if line.is_empty() {
            if !data.is_empty() {
                let item = Item {
                    text: data.join("\n"),
                    event: Some(if name.is_empty() { "message" } else { name }.into()),
                    id: id.into(),
                    retry,
                };
                if dispatch(item, number, false, &mut pulls) {
                    return pulls;
                }
            }
            number += 1;
            size = 0;
            data.clear();
            name = "";
        } else {
            let (field, value) = line.split_once(':').unwrap_or((line, ""));
            let value = value.strip_prefix(' ').unwrap_or(value);
            match field {
                "data" => data.push(value),
                "event" => name = value,
                "id" if !value.contains('\0') => id = value,
                "retry" if value.bytes().all(|b| b.is_ascii_digit()) => {
                    if let Ok(ms) = value.parse() {
                        retry = Some(Duration::from_millis(ms));
                    }
                }
                _ => {}
            }
        }
    }
    pulls.push(match tail {
        Tail::Eof if require_last => Pull::Error(Fault::MissingLast),
        Tail::Eof => Pull::End,
        Tail::Cut | Tail::Short => Pull::Error(Fault::Body),
        Tail::Idle => Pull::Error(Fault::Idle),
        Tail::Deadline => Pull::Error(Fault::Deadline),
    });
    pulls
}

fn check_error(
    got: Error<String>,
    fault: Fault,
    endpoint: &str,
    headers: &HeaderMap,
) -> TestCaseResult {
    let (kind, note, class) = match fault {
        Fault::Size(n) => (
            Kind::Mismatch,
            format!("record {n} exceeds the reader's size limit"),
            None,
        ),
        Fault::Utf8(n) => (Kind::Mismatch, format!("record {n} is not UTF-8"), None),
        Fault::Decode(n, _, _) => (
            Kind::Mismatch,
            format!("record {n} could not be decoded"),
            None,
        ),
        Fault::Rejected(n) | Fault::Foreign(n) => (
            Kind::Mismatch,
            format!("record {n} does not fit the stream"),
            None,
        ),
        Fault::Refused => (Kind::Refused, String::new(), None),
        Fault::MissingLast => (
            Kind::Mismatch,
            "the body ended before its last record".into(),
            None,
        ),
        Fault::Body => (
            Kind::Transport,
            "the response body could not be read after status 200".into(),
            Some(Class::Body),
        ),
        Fault::Idle => (
            Kind::Transport,
            "no body data for 2 s after status 200 (with_idle_timeout changes it)".into(),
            Some(Class::Timeout),
        ),
        Fault::Deadline => (
            Kind::Transport,
            "the call took longer than 1 s (with_timeout)".into(),
            Some(Class::Timeout),
        ),
    };
    let headline = match kind {
        Kind::Mismatch => format!("mismatch: status 200, {note}"),
        Kind::Refused => "refused: status 200".into(),
        _ => format!("transport error: {note}"),
    };
    let message = format!("{headline}, in {endpoint}");
    prop_assert_eq!(got.kind(), kind);
    prop_assert_eq!(got.class(), class);
    prop_assert_eq!(got.to_string(), message.as_str());
    prop_assert_eq!(got.endpoint(), endpoint);
    prop_assert_eq!(got.tries(), 1);
    prop_assert!(got.is_status(200));
    prop_assert_eq!(got.headers().unwrap().as_map(), headers);
    prop_assert!(got.body().is_none() && got.kept().is_none());
    let reason = matches!(fault, Fault::Rejected(_)).then_some(REASON);
    let problem = matches!(fault, Fault::Refused).then_some(PROBLEM);
    prop_assert_eq!(got.reason(), reason);
    prop_assert_eq!(got.problem().map(String::as_str), problem);

    let mut details = message;
    if let Some(reason) = reason {
        details.push_str(&format!("\nreason: {reason}"));
    }
    details.push_str("\nheaders:");
    for (name, value) in headers {
        details.push_str(&format!("\n  {name}: {}", value.to_str().unwrap()));
    }
    if let Some(problem) = problem {
        details.push_str(&format!("\nproblem: {problem:?}"));
    }
    if let Fault::Decode(_, line, column) = fault {
        details.push_str(&format!("\nreader: line {line}, column {column}"));
    }
    if kind == Kind::Mismatch {
        details.push_str(&format!("\ncaused by: {note}"));
    }
    prop_assert_eq!(got.details().to_string(), details);
    Ok(())
}

struct Release(Arc<AtomicBool>);

impl Drop for Release {
    fn drop(&mut self) {
        self.0.store(true, SeqCst);
    }
}

async fn check(
    format: Format,
    bytes: &[u8],
    cuts: &[usize],
    tail: Tail,
    require_last: bool,
) -> TestCaseResult {
    let mut start = 0;
    let chunks: Vec<_> = cuts
        .iter()
        .copied()
        .chain([bytes.len()])
        .map(|end| {
            let chunk = Bytes::copy_from_slice(&bytes[start..end]);
            start = end;
            Ok(chunk)
        })
        .collect();
    let mut head = HeaderMap::new();
    head.insert("x-feed", "sample".parse().unwrap());
    if matches!(tail, Tail::Short) {
        head.insert("content-length", (bytes.len() + 1).into());
    }
    let mut chunks = chunks.into_iter();
    let released = Arc::new(AtomicBool::new(false));
    let witness = Release(released.clone());
    let mut cut = matches!(tail, Tail::Cut);
    let reached_cut = Arc::new(AtomicBool::new(false));
    let cut_witness = reached_cut.clone();
    let source = futures_util::stream::poll_fn(move |_| {
        let _keep = &witness;
        if let Some(chunk) = chunks.next() {
            return std::task::Poll::Ready(Some(chunk));
        }
        if std::mem::take(&mut cut) {
            cut_witness.store(true, SeqCst);
            return std::task::Poll::Ready(Some(Err(io::Error::other("cut"))));
        }
        if matches!(tail, Tail::Idle | Tail::Deadline) {
            return std::task::Poll::Pending;
        }
        std::task::Poll::Ready(None)
    });
    let mut reply = concord::reply(200, concord::reqwest::Body::wrap_stream(source));
    *reply.headers_mut() = head.clone();
    let reply = Mutex::new(Some(reply));
    let sends = Arc::new(AtomicUsize::new(0));
    let sent = sends.clone();
    let client = Feed::new()
        .with_transport(concord::fake(move |_| {
            sent.fetch_add(1, SeqCst);
            Ok(reply
                .lock()
                .unwrap()
                .take()
                .unwrap_or_else(|| concord::reply(204, "")))
        }))
        .with_retry(concord::Retry::standard())
        .with_middleware(Limit::new().max_in_flight(1));

    let timeout = Duration::from_secs(if matches!(tail, Tail::Deadline) {
        1
    } else {
        30
    });
    let idle = Duration::from_secs(2);
    macro_rules! open {
        ($method:ident) => {
            client
                .$method()
                .with_timeout(timeout)
                .with_idle_timeout(idle)
                .await
                .unwrap()
        };
    }
    let (mut stream, endpoint) = match (format, require_last) {
        (Format::Lines, false) => (open!(lines), "feed.lines"),
        (Format::Lines, true) => (open!(lines_last), "feed.lines_last"),
        (Format::Events, false) => (open!(events), "feed.events"),
        (Format::Events, true) => (open!(events_last), "feed.events_last"),
    };
    let mut peer = std::pin::pin!(client
        .peer()
        .with_timeout(Duration::from_secs(60))
        .into_future());
    prop_assert!(peer.as_mut().now_or_never().is_none());
    let pulls = expected(format, bytes, tail, require_last);
    let should_reach_cut = matches!(
        (tail, pulls.last()),
        (Tail::Cut, Some(Pull::Error(Fault::Body)))
    );
    for pull in pulls {
        let got = AssertUnwindSafe(stream.try_next()).catch_unwind().await;
        let is_released = match pull {
            Pull::Value(item, released) => {
                prop_assert_eq!(got.unwrap().unwrap(), Some(item));
                released
            }
            Pull::End => {
                prop_assert_eq!(got.unwrap().unwrap(), None);
                true
            }
            Pull::Error(fault) => {
                check_error(got.unwrap().unwrap_err(), fault, endpoint, &head)?;
                true
            }
            Pull::Panic => {
                prop_assert!(got.is_err());
                true
            }
        };
        prop_assert_eq!(
            released.load(SeqCst),
            is_released,
            "body release at the wrong pull"
        );
        prop_assert_eq!(sends.load(SeqCst), 1);
    }
    prop_assert_eq!(reached_cut.load(SeqCst), should_reach_cut);
    prop_assert_eq!(stream.status().as_u16(), 200);
    prop_assert_eq!(stream.headers().as_map(), &head);
    prop_assert!(
        peer.as_mut().now_or_never().is_some_and(|r| r.is_ok()),
        "body permit is still held"
    );
    prop_assert_eq!(sends.load(SeqCst), 2);
    prop_assert_eq!(stream.try_next().await.unwrap(), None);
    prop_assert!(stream.next().await.is_none());
    Ok(())
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .start_paused(true)
        .build()
        .unwrap()
}

const TEXT: &[&[u8]] = &[
    b"",
    b"skip",
    b"last",
    b"end",
    b"refuse",
    b"reject",
    b"!",
    b"[",
    b"\"decoded\"",
    b"foreign",
    "é".as_bytes(),
    b"\xff",
    b"ordinary",
    b"1234567890123456789012345678901234567890",
    b"12345678901234567890123456789012345678901",
];
const SSE_FIELD: &[&[u8]] = &[
    b"id: one",
    b"id:",
    b"id: two\0ignored",
    b"retry: 021",
    b"retry: +1",
    b"retry: 18446744073709551616",
    b"event: delta",
    b": comment",
    b"unknown",
    b"data: second",
];

fn body(format: Format) -> impl Strategy<Value = Vec<u8>> {
    (
        prop::collection::vec(
            (
                0..TEXT.len(),
                0..4usize,
                prop::option::of(0..SSE_FIELD.len()),
            ),
            0..9,
        ),
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(move |(rows, bom, truncate)| {
            let mut wire = Vec::new();
            if bom && matches!(format, Format::Events) {
                wire.extend(b"\xef\xbb\xbf");
            }
            for (i, (text, style, field)) in rows.into_iter().enumerate() {
                match format {
                    Format::Lines => {
                        wire.extend(TEXT[text]);
                        wire.extend(if style == 0 {
                            b"\r\n".as_slice()
                        } else {
                            b"\n"
                        });
                    }
                    Format::Events => {
                        let eol = [b"\r\n".as_slice(), b"\n", b"\r", b"\n"][style];
                        if let Some(field) = field {
                            wire.extend(SSE_FIELD[field]);
                            wire.extend(eol);
                        }
                        if !(i % 3 == 0 && style == 3) {
                            wire.extend(b"data: ");
                            wire.extend(TEXT[text]);
                            wire.extend(eol);
                        }
                        wire.extend(eol);
                    }
                }
            }
            if truncate {
                wire.pop();
            }
            wire
        })
}

#[test]
fn c11_13_typed_records_follow_their_framing_and_ending() {
    let mut runner = TestRunner::new(ProptestConfig {
        cases: 512,
        failure_persistence: None,
        ..ProptestConfig::default()
    });
    let cases = (
        prop_oneof![
            body(Format::Lines).prop_map(|b| (Format::Lines, b)),
            body(Format::Events).prop_map(|b| (Format::Events, b)),
        ],
        prop::collection::vec(0..128usize, 0..12),
        prop_oneof![4 => Just(Tail::Eof), 1 => Just(Tail::Cut), 1 => Just(Tail::Short),
            1 => Just(Tail::Idle), 1 => Just(Tail::Deadline)],
    );
    let runtime = runtime();
    let result = runner.run(&cases, |((format, bytes), cuts, tail)| {
        let mut cuts: Vec<_> = cuts
            .into_iter()
            .map(|cut| cut % (bytes.len() + 1))
            .collect();
        cuts.sort_unstable();
        for require_last in [false, true] {
            runtime.block_on(check(format, &bytes, &cuts, tail, require_last))?;
        }
        Ok(())
    });
    if let Err(failure) = result {
        panic!("{failure}");
    }
}

#[test]
fn c11_13_each_ending_survives_every_two_chunk_split() -> TestCaseResult {
    let runtime = runtime();
    for (format, prefix) in [
        (Format::Lines, "\r\nskip\né\r\n"),
        (Format::Events, "\u{feff}: comment\r\n\r\nid: 4\rretry: 2\ndata: skip\r\n\r\nevent: delta\rdata: é\r\ndata: second\r\n\r\n"),
    ] {
        for (ending, tail) in [
            ("", Tail::Eof), ("last", Tail::Cut), ("end", Tail::Cut), ("refuse", Tail::Eof),
            ("reject", Tail::Eof), ("foreign", Tail::Eof), ("!", Tail::Eof), ("panic", Tail::Eof),
            ("oversize oversize oversize oversize oversize", Tail::Eof), ("invalid", Tail::Eof),
            ("", Tail::Cut), ("", Tail::Short), ("", Tail::Idle), ("", Tail::Deadline),
        ] {
            let mut bytes = prefix.as_bytes().to_vec();
            if !ending.is_empty() {
                if matches!(format, Format::Events) {
                    bytes.extend(b"data: ");
                }
                bytes.extend(if ending == "invalid" { b"\xff" } else { ending.as_bytes() });
                bytes.extend(if matches!(format, Format::Events) { b"\r\n\r\n" } else { b"\r\n".as_slice() });
            }
            for split in 0..=bytes.len() {
                for require_last in [false, true] {
                    runtime.block_on(check(format, &bytes, &[split], tail, require_last))?;
                }
            }
        }
    }
    Ok(())
}

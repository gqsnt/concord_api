//! Server-sent events without a network: the WHATWG examples, every interpretation rule, every
//! split of each fixture (one cut anywhere, one byte per chunk, random partitions with empty
//! chunks), byte order mark and CR LF splits, Concord's stricter end-of-input, encoding and size
//! rules, cancellation of a pending `next()`, the `RecordStream` bridge, the head check, safe
//! diagnostics and bounded work per poll.
#![cfg(feature = "sse")]

mod support;

use std::collections::VecDeque;
use std::future::{Future, poll_fn};
use std::pin::{Pin, pin};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use concord_core::bytes::Bytes;
use concord_core::reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderValue};
use concord_core::reqwest::{StatusCode, Version};
use concord_core::{Incremental, ResponseHead};
use concord_extras::sse::{DEFAULT_MAX_EVENT_BYTES, Event, Sse, SseEvents, SseOptions, SseReader};
use concord_extras::stream::{
    ByteSource, Chunks, Completion, ReadError, ReadErrorKind, SourceError,
};
use futures_core::Stream;
use support::partitions;

const NOT_UTF8: &str = "a line is not valid UTF-8";
const ENDS_IN_LINE: &str = "the input ends inside a line";
const ENDS_IN_EVENT: &str =
    "the input ends inside an event, before the blank line that dispatches it";
const TOO_LARGE: &str = "an event exceeds max_event_bytes";
const TOO_MANY: &str = "the input exceeds the configured total records";

/// One item from `next()`: an event's `(type, data, id)`, or an error's kind, label and `line`.
type Item = Result<(String, String, String), (ReadErrorKind, &'static str, Option<u64>)>;

fn ev(event: &str, data: &str, id: &str) -> Item {
    Ok((event.into(), data.into(), id.into()))
}

fn msg(data: &str) -> Item {
    ev("message", data, "")
}

fn err(kind: ReadErrorKind, label: &'static str, line: u64) -> Item {
    Err((kind, label, Some(line)))
}

fn item(result: Result<Event, ReadError>) -> Item {
    result
        .map(|e| (e.event().to_owned(), e.data().to_owned(), e.id().to_owned()))
        .map_err(|e| {
            let codec = e.codec().expect("a codec description");
            (e.kind(), codec.label(), codec.get("line"))
        })
}

/// What a whole read observed: the items, how it ended, and the stream state left behind.
#[derive(Debug, PartialEq)]
struct Outcome {
    items: Vec<Item>,
    completion: Completion,
    last_event_id: String,
    retry: Option<Duration>,
}

/// Reads every item, then checks the reader stays fused.
async fn read_all<S: ByteSource>(mut events: SseEvents<S>) -> Outcome {
    let mut items = Vec::new();
    while let Some(next) = events.next().await {
        items.push(item(next));
        assert!(items.len() < 100_000, "runaway reader");
    }
    assert!(events.next().await.is_none(), "fused after the end");
    assert!(events.next().await.is_none(), "still fused");
    Outcome {
        items,
        completion: events.completion(),
        last_event_id: events.last_event_id().to_owned(),
        retry: events.retry(),
    }
}

async fn read_with(options: &SseOptions, chunks: Vec<Vec<u8>>) -> Outcome {
    read_all(SseReader::new(options.clone()).read(Chunks::new(chunks))).await
}

/// The outcome of `input` is the same for every partition, and its items are `expected`.
async fn every_partition_with(options: &SseOptions, input: &[u8], expected: &[Item]) -> Outcome {
    let whole = read_with(options, vec![input.to_vec()]).await;
    assert_eq!(whole.items, expected, "unsplit input");
    let ended = match expected.last() {
        Some(Err(_)) => Completion::Failed,
        _ => Completion::EndOfInput,
    };
    assert_eq!(whole.completion, ended);
    for parts in partitions(input) {
        let got = read_with(options, parts.clone()).await;
        assert_eq!(got, whole, "partition {parts:?}");
    }
    whole
}

async fn every_partition(input: &[u8], expected: &[Item]) -> Outcome {
    every_partition_with(&SseOptions::new(), input, expected).await
}

/// The examples of the WHATWG HTML standard, "Server-sent events" (9.2.6, "Interpreting an event
/// stream", and the authoring notes before it).
#[tokio::test]
async fn whatwg_examples() {
    // Three lines of data, one event.
    every_partition(
        b"data: YHOO\ndata: +2\ndata: 10\n\n",
        &[msg("YHOO\n+2\n10")],
    )
    .await;
    // A comment, ids, a field without a value (resets the ID), and two spaces of which one stays.
    let out = every_partition(
        b": test stream\n\ndata: first event\nid: 1\n\ndata:second event\nid\n\ndata:  third event\n\n",
        &[
            ev("message", "first event", "1"),
            msg("second event"),
            msg(" third event"),
        ],
    )
    .await;
    assert_eq!(out.last_event_id, "");
    // `data` without a value: the first event's data is empty, the second one's a line feed; the
    // trailing `data:` is not dispatched (the standard discards it, Concord refuses the end).
    every_partition(
        b"data\n\ndata\ndata\n\ndata:",
        &[
            msg(""),
            msg("\n"),
            err(ReadErrorKind::Format, ENDS_IN_LINE, 6),
        ],
    )
    .await;
    every_partition(
        b"data\n\ndata\ndata\n\ndata:\n",
        &[
            msg(""),
            msg("\n"),
            err(ReadErrorKind::Format, ENDS_IN_EVENT, 6),
        ],
    )
    .await;
    every_partition(
        b"data\n\ndata\ndata\n\ndata:\n\n",
        &[msg(""), msg("\n"), msg("")],
    )
    .await;
    // With or without the space after the colon: identical events.
    every_partition(b"data:test\n\ndata: test\n\n", &[msg("test"), msg("test")]).await;
    // Event types.
    every_partition(
        b"event: add\ndata: 73857293\n\nevent: remove\ndata: 2153\n\nevent: add\ndata: 113411\n\n",
        &[
            ev("add", "73857293", ""),
            ev("remove", "2153", ""),
            ev("add", "113411", ""),
        ],
    )
    .await;
}

#[tokio::test]
async fn field_rules() {
    // Unknown fields, case-sensitive names, a field without a colon, one space removed only.
    every_partition(
        b"foo: bar\nData: no\nDATA: no\ndata\ndata:x\ndata:  y\ndata: z \n\n",
        &[msg("\nx\n y\nz ")],
    )
    .await;
    // The colon splits once: the value may contain colons.
    every_partition(b"data: a: b:c\n\n", &[msg("a: b:c")]).await;
    // Comments anywhere, also between the fields of one event and before a blank line.
    every_partition(
        b":start\ndata: a\n: middle\n:\ndata: b\n:end\n\n",
        &[msg("a\nb")],
    )
    .await;
    // The event type applies to one event; an empty one means `message`.
    every_partition(
        b"event: tick\ndata: 1\n\ndata: 2\n\nevent:\ndata: 3\n\nevent: a\nevent: b\ndata: 4\n\n",
        &[ev("tick", "1", ""), msg("2"), msg("3"), ev("b", "4", "")],
    )
    .await;
    // An event with an empty data buffer is not dispatched, and its type is reset.
    every_partition(
        b"event: lonely\n\ndata: x\n\nevent: e\nid: 9\nretry: 5\n\n",
        &[msg("x")],
    )
    .await;
}

#[tokio::test]
async fn event_ids() {
    // The ID persists across events until changed; an empty value resets it.
    let out = every_partition(
        b"id: 1\ndata: a\n\ndata: b\n\nid:\ndata: c\n\nid: 2\ndata: d\n\n",
        &[
            ev("message", "a", "1"),
            ev("message", "b", "1"),
            msg("c"),
            ev("message", "d", "2"),
        ],
    )
    .await;
    assert_eq!(out.last_event_id, "2");
    // An ID containing NUL is ignored: the previous one stays.
    let out = every_partition(
        b"id: 7\ndata: a\n\nid: 8\x009\ndata: b\n\n",
        &[ev("message", "a", "7"), ev("message", "b", "7")],
    )
    .await;
    assert_eq!(out.last_event_id, "7");
    // An ID without data dispatches nothing but becomes the last event ID at its blank line; the
    // last field of an event wins; the ID is text (not trimmed beyond the one space).
    let out = every_partition(
        "id: 3\n\nid: x\nid:  é 4\ndata: a\n\n".as_bytes(),
        &[ev("message", "a", " é 4")],
    )
    .await;
    assert_eq!(out.last_event_id, " é 4");
    let out = every_partition(b"data: a\nid: 5\n\nid: 6\n\n", &[ev("message", "a", "5")]).await;
    assert_eq!(out.last_event_id, "6");
}

/// The last event ID is the buffer as of the last blank line: an `id` in an event that never
/// completed does not count (it would skip that event on a resume).
#[tokio::test]
async fn an_unfinished_event_does_not_advance_the_last_event_id() {
    let out = every_partition(
        b"id: 1\ndata: a\n\nid: 2\ndata: b",
        &[
            ev("message", "a", "1"),
            err(ReadErrorKind::Format, ENDS_IN_LINE, 5),
        ],
    )
    .await;
    assert_eq!(out.last_event_id, "1");
    let mut events = SseReader::default().read(Chunks::new([&b"id: 1\ndata: a\n\nid: 2\n"[..]]));
    assert_eq!(events.last_event_id(), "");
    assert_eq!(item(events.next().await.unwrap()), ev("message", "a", "1"));
    assert_eq!(events.last_event_id(), "1");
    assert!(events.next().await.unwrap().is_err());
    assert_eq!(events.last_event_id(), "1");
}

#[tokio::test]
async fn retry_rules() {
    let out = every_partition(b"retry: 3000\ndata: a\n\n", &[msg("a")]).await;
    assert_eq!(out.retry, Some(Duration::from_millis(3000)));
    // Only ASCII digits, at least one; an invalid value leaves the previous one.
    for invalid in [
        &b"retry:\n\n"[..],
        b"retry\n\n",
        b"retry: \n\n",
        b"retry: 1.5\n\n",
        b"retry: -1\n\n",
        b"retry:  10\n\n",
        b"retry: 10 \n\n",
        b"retry: +10\n\n",
        "retry: \u{0661}\n\n".as_bytes(), // an Arabic-Indic digit is not an ASCII digit
    ] {
        let out = every_partition(invalid, &[]).await;
        assert_eq!(out.retry, None, "{invalid:?}");
        let input = [b"retry: 7\n\n".as_slice(), invalid].concat();
        let out = every_partition(&input, &[]).await;
        assert_eq!(out.retry, Some(Duration::from_millis(7)), "{invalid:?}");
    }
    // Applied as soon as its line is read, without a dispatch, and the last valid one wins.
    let mut events =
        SseReader::default().read(Chunks::new([&b"retry: 5\nretry: x\nretry: 0\ndata: a"[..]]));
    assert!(events.next().await.unwrap().is_err());
    assert_eq!(events.retry(), Some(Duration::ZERO));
    // Past u64 milliseconds: saturated.
    let out = every_partition(b"retry: 184467440737095516160\n\n", &[]).await;
    assert_eq!(out.retry, Some(Duration::from_millis(u64::MAX)));
}

#[tokio::test]
async fn line_ends_lf_cr_crlf_and_their_splits() {
    // Every partition cuts each CR LF, and every CR before an LF in the next chunk is one end.
    let expected = [msg("a\nb"), ev("t", "c", "1"), ev("message", "d", "1")];
    for input in [
        &b"data: a\ndata: b\n\nevent: t\nid: 1\ndata: c\n\ndata: d\n\n"[..],
        b"data: a\r\ndata: b\r\n\r\nevent: t\r\nid: 1\r\ndata: c\r\n\r\ndata: d\r\n\r\n",
        b"data: a\rdata: b\r\revent: t\rid: 1\rdata: c\r\rdata: d\r\r",
        b"data: a\r\ndata: b\r\revent: t\nid: 1\r\ndata: c\n\r\ndata: d\r\r\n",
    ] {
        every_partition(input, &expected).await;
    }
    // CR CR is two line ends (a blank line), CR LF one; LF CR is two.
    every_partition(b"data: a\r\rdata: b\r\n\r\n", &[msg("a"), msg("b")]).await;
    every_partition(b"data: a\n\rdata: b\n\r", &[msg("a"), msg("b")]).await;
    // A CR ends the stream's last blank line: a clean end, also when the CR is the last byte.
    let out = every_partition(b"data: a\r\r", &[msg("a")]).await;
    assert_eq!(out.completion, Completion::EndOfInput);
}

#[tokio::test]
async fn byte_order_mark() {
    // One leading mark is skipped, also split at any byte (every partition cuts it).
    every_partition(b"\xEF\xBB\xBFdata: a\n\n", &[msg("a")]).await;
    every_partition(b"\xEF\xBB\xBF\r\ndata: a\r\n\r\n", &[msg("a")]).await;
    let out = every_partition(b"\xEF\xBB\xBF", &[]).await;
    assert_eq!(out.completion, Completion::EndOfInput);
    // A second mark is content: the field `\u{FEFF}data` is unknown.
    every_partition(
        b"\xEF\xBB\xBF\xEF\xBB\xBFdata: a\n\ndata: b\n\n",
        &[msg("b")],
    )
    .await;
    // The mark elsewhere is ordinary (valid) text.
    every_partition("data: \u{FEFF}a\n\n".as_bytes(), &[msg("\u{FEFF}a")]).await;
    // A partial mark is content: invalid UTF-8, or an unfinished line at the end.
    every_partition(
        b"\xEF\xBBdata: a\n\n",
        &[err(ReadErrorKind::Format, NOT_UTF8, 1)],
    )
    .await;
    every_partition(b"\xEF\xBB", &[err(ReadErrorKind::Format, ENDS_IN_LINE, 1)]).await;
}

#[tokio::test]
async fn utf8_is_validated_on_every_line() {
    // Multi-byte characters split at every interior byte by the partitions.
    every_partition(
        "event: é\ndata: café 🌍 — ok\n\n".as_bytes(),
        &[ev("é", "café 🌍 — ok", "")],
    )
    .await;
    // Invalid UTF-8 in data, an event type, an ID, a comment and an unknown field: a terminal
    // Format error naming the line and the byte column, after the events before it.
    for (input, line, column) in [
        (&b"data: a\n\ndata: \xFF\n\n"[..], 3, 7),
        (b"event: \xC3\n\n", 1, 8),
        (b"id: \xE2\x82\n\n", 1, 5),
        (b": \xF0\x9F\x8C\n\n", 1, 3),
        (b"x\xC0\xAF: y\n\n", 1, 2),
        (b"data: \xED\xA0\x80\n\n", 1, 7), // an encoded surrogate
    ] {
        let events = if input.starts_with(b"data: a\n") {
            vec![msg("a")]
        } else {
            Vec::new()
        };
        let mut expected = events;
        expected.push(err(ReadErrorKind::Format, NOT_UTF8, line));
        every_partition(input, &expected).await;
        let mut events = SseReader::default().read(Chunks::new([input.to_vec()]));
        let error = loop {
            match events.next().await.expect("an error before the end") {
                Ok(_) => continue,
                Err(e) => break e,
            }
        };
        assert_eq!(
            error.codec().unwrap().get("column"),
            Some(column),
            "{input:?}"
        );
        assert_eq!(events.completion(), Completion::Failed);
    }
}

/// Only an end at an event boundary is clean; anything else is a terminal truncation error.
#[tokio::test]
async fn end_of_input_only_at_an_event_boundary() {
    for (clean, expected) in [
        (&b""[..], &[][..]),
        (b"\n", &[]),
        (b"\r\n\r\n", &[]),
        (b": keep-alive\n", &[]),
        (b": a\n:b\r\n\n\n", &[]),
        (b"data: a\n\n: done\n", &[msg("a")]),
        (b"data: a\n\n\n\n", &[msg("a")]),
    ] {
        let out = every_partition(clean, expected).await;
        assert_eq!(out.completion, Completion::EndOfInput, "{clean:?}");
    }
    for (input, expected) in [
        (
            &b"data: a"[..],
            vec![err(ReadErrorKind::Format, ENDS_IN_LINE, 1)],
        ),
        (
            b"data: a\n",
            vec![err(ReadErrorKind::Format, ENDS_IN_EVENT, 1)],
        ),
        (
            b"data: a\r",
            vec![err(ReadErrorKind::Format, ENDS_IN_EVENT, 1)],
        ),
        (
            b"data: a\r\n",
            vec![err(ReadErrorKind::Format, ENDS_IN_EVENT, 1)],
        ),
        (
            b"event: e\n",
            vec![err(ReadErrorKind::Format, ENDS_IN_EVENT, 1)],
        ),
        (
            b"id: 1\n",
            vec![err(ReadErrorKind::Format, ENDS_IN_EVENT, 1)],
        ),
        (
            b"retry: 1\n",
            vec![err(ReadErrorKind::Format, ENDS_IN_EVENT, 1)],
        ),
        (
            b"unknown\n",
            vec![err(ReadErrorKind::Format, ENDS_IN_EVENT, 1)],
        ),
        (
            b"data: a\n: comment\n",
            vec![err(ReadErrorKind::Format, ENDS_IN_EVENT, 2)],
        ),
        (
            b": unfinished comment",
            vec![err(ReadErrorKind::Format, ENDS_IN_LINE, 1)],
        ),
        (
            b"data: a\n\ndata: b\ndata: c",
            vec![msg("a"), err(ReadErrorKind::Format, ENDS_IN_LINE, 4)],
        ),
        (
            b"data: a\n\ndata: b\n",
            vec![msg("a"), err(ReadErrorKind::Format, ENDS_IN_EVENT, 3)],
        ),
    ] {
        let out = every_partition(input, &expected).await;
        assert_eq!(out.completion, Completion::Failed, "{input:?}");
    }
}

#[tokio::test]
async fn max_event_bytes_bounds_the_event_being_assembled() {
    // `data: 12345` (11) + data buffer `12345\n` would be the next line's starting point.
    let options = |max| SseOptions::new().max_event_bytes(max);
    every_partition_with(&options(11), b"data: 12345\n\n", &[msg("12345")]).await;
    every_partition_with(
        &options(10),
        b"data: 12345\n\n",
        &[err(ReadErrorKind::Limit, TOO_LARGE, 1)],
    )
    .await;
    // The data buffer (`1234\n`, 5) and the line being received (`data: 56`, 8) count together.
    every_partition_with(
        &options(13),
        b"data: 1234\ndata: 56\n\n",
        &[msg("1234\n56")],
    )
    .await;
    every_partition_with(
        &options(12),
        b"data: 1234\ndata: 56\n\n",
        &[err(ReadErrorKind::Limit, TOO_LARGE, 2)],
    )
    .await;
    // So does the event type; a dispatch frees the bound, and earlier events stay delivered.
    every_partition_with(
        &options(13),
        b"event: abcde\ndata: x\n\nevent: abcde\ndata: y\n\nevent: abcde\ndata: xyz\n\n",
        &[
            ev("abcde", "x", ""),
            ev("abcde", "y", ""),
            err(ReadErrorKind::Limit, TOO_LARGE, 8),
        ],
    )
    .await;
    // Complete comments do not count; a comment line being received does (also inside an
    // event, together with its data).
    every_partition_with(
        &options(12),
        b": 0123456789\n: 0123456789\ndata: 12345\n\n: 0123456789\n",
        &[msg("12345")],
    )
    .await;
    every_partition_with(
        &options(12),
        b"data: 12345\n: 0123456789\n\n",
        &[err(ReadErrorKind::Limit, TOO_LARGE, 2)],
    )
    .await;
    every_partition_with(
        &options(12),
        b": 0123456789X\n",
        &[err(ReadErrorKind::Limit, TOO_LARGE, 1)],
    )
    .await;
    // The line end is not counted: CR LF after exactly `max` bytes passes.
    every_partition_with(&options(7), b"data: a\r\n\r\n", &[msg("a")]).await;
    // The error carries the limit, and the reader is fused.
    let mut events = SseReader::new(options(4)).read(Chunks::new([&b"data: abc\n\n"[..]]));
    let error = events.next().await.unwrap().unwrap_err();
    assert_eq!(error.codec().unwrap().get("limit"), Some(4));
    assert!(error.is_terminal());
    assert!(events.next().await.is_none());
    assert_eq!(events.completion(), Completion::Failed);
}

#[tokio::test]
async fn the_default_bound_is_one_mebibyte() {
    assert_eq!(DEFAULT_MAX_EVENT_BYTES, 1024 * 1024);
    let line = |n: usize| [b"data: ".to_vec(), vec![b'x'; n - 6], b"\n\n".to_vec()].concat();
    let at = line(DEFAULT_MAX_EVENT_BYTES);
    let out =
        read_all(SseReader::default().read(Chunks::new(at.chunks(4093).map(<[u8]>::to_vec)))).await;
    assert_eq!(out.completion, Completion::EndOfInput);
    let [Ok((_, data, _))] = &out.items[..] else {
        panic!("one event");
    };
    assert_eq!(data.len(), DEFAULT_MAX_EVENT_BYTES - 6);
    let over = line(DEFAULT_MAX_EVENT_BYTES + 1);
    let out = read_all(SseReader::new(SseOptions::default()).read(Chunks::new([over]))).await;
    assert_eq!(out.items, [err(ReadErrorKind::Limit, TOO_LARGE, 1)]);
}

#[tokio::test]
async fn max_events_is_a_limit_never_a_clean_end() {
    let options = SseOptions::new().max_events(2);
    let out = every_partition_with(
        &options,
        b"data: a\n\n: c\ndata: b\n\n",
        &[msg("a"), msg("b")],
    )
    .await;
    assert_eq!(out.completion, Completion::EndOfInput);
    every_partition_with(
        &options,
        b"data: a\n\ndata: b\n\ndata: c\n\n",
        &[
            msg("a"),
            msg("b"),
            Err((ReadErrorKind::Limit, TOO_MANY, None)),
        ],
    )
    .await;
    // Not-dispatched blocks are not events.
    every_partition_with(
        &options,
        b"data: a\n\nid: 1\n\nevent: x\n\ndata: b\n\n",
        &[msg("a"), ev("message", "b", "1")],
    )
    .await;
    let mut events = SseReader::new(SseOptions::new().max_events(1))
        .read(Chunks::new([&b"data: a\n\ndata: b\n\n"[..]]));
    assert!(events.next().await.unwrap().is_ok());
    let error = events.next().await.unwrap().unwrap_err();
    assert_eq!(error.kind(), ReadErrorKind::Limit);
    assert_eq!(error.position().records, 1);
    assert_eq!(error.codec().unwrap().get("limit"), Some(1));
}

/// A source failure (here an injected one, mid-event) is a terminal `Transport` error after the
/// delivered events; the reader is fused and its completion says it failed.
#[tokio::test]
async fn a_failing_source_is_a_terminal_transport_error() {
    let mut events = SseReader::default().read(
        Chunks::new([&b"data: a\n\n"[..], b"data: b"]).then_fail("the synthetic source failed"),
    );
    assert_eq!(item(events.next().await.unwrap()), msg("a"));
    let error = events.next().await.unwrap().unwrap_err();
    assert_eq!(error.kind(), ReadErrorKind::Transport);
    assert!(error.is_terminal());
    assert_eq!(error.position().records, 1);
    assert_eq!(
        error.position().bytes,
        16,
        "the unfinished line was consumed"
    );
    assert_eq!(
        error.to_string(),
        "reading failed: the synthetic source failed (after 1 records, 16 bytes)"
    );
    assert!(events.next().await.is_none());
    assert_eq!(events.completion(), Completion::Failed);
}

/// Errors show static labels and numbers, never event content: invalid UTF-8 next to a secret,
/// an oversized secret, a truncated secret.
#[tokio::test]
async fn diagnostics_never_show_event_content() {
    const SECRET: &str = "SECRET-7f3a91";
    for (input, options) in [
        (
            [
                format!("data: {SECRET}\ndata: {SECRET}").as_bytes(),
                b"\xFF\n\n",
            ]
            .concat(),
            SseOptions::new(),
        ),
        (
            [b"data: ".as_slice(), SECRET.as_bytes(), b"\xC3"].concat(),
            SseOptions::new(),
        ),
        (
            format!("event: {SECRET}\ndata: {SECRET}{SECRET}\n\n").into_bytes(),
            SseOptions::new().max_event_bytes(30),
        ),
        (
            format!("id: {SECRET}\ndata: {SECRET}").into_bytes(),
            SseOptions::new(),
        ),
        (format!(": {SECRET}").into_bytes(), SseOptions::new()),
    ] {
        let mut events = SseReader::new(options).read(Chunks::new([input]));
        let error = events.next().await.unwrap().unwrap_err();
        let shown = format!("{error} | {error:?} | {events:?}");
        assert!(!shown.contains("7f3a91"), "{shown}");
        assert!(
            error.source_unredacted().is_none(),
            "nothing quoted to reveal"
        );
    }
    let mut events = SseReader::default()
        .read(Chunks::new([
            format!("id: {SECRET}\ndata: {SECRET}\n\n").into_bytes()
        ]));
    let event = events.next().await.unwrap().unwrap();
    // The event itself is the caller's data.
    assert_eq!(event.data(), SECRET);
    assert!(!format!("{events:?}").contains("7f3a91"));
}

/// A source the test feeds by hand: `Pending` while its queue is empty and it is open.
#[derive(Clone, Default)]
struct Gate {
    queue: Arc<Mutex<VecDeque<Bytes>>>,
    closed: Arc<AtomicBool>,
}

impl Gate {
    fn push(&self, bytes: &[u8]) {
        self.queue
            .lock()
            .unwrap()
            .push_back(Bytes::copy_from_slice(bytes));
    }

    fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }
}

impl ByteSource for Gate {
    async fn next_chunk(&mut self) -> Result<Option<Bytes>, SourceError> {
        poll_fn(|_| match self.queue.lock().unwrap().pop_front() {
            Some(chunk) => Poll::Ready(Ok(Some(chunk))),
            None if self.closed.load(Ordering::SeqCst) => Poll::Ready(Ok(None)),
            None => Poll::Pending,
        })
        .await
    }
}

/// Polls `future` once with a no-op waker.
fn poll_once<F: Future>(future: F) -> Poll<F::Output> {
    let mut future = pin!(future);
    future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
}

const FIXTURE: &[u8] = "\u{FEFF}: hello\r\nid: 1\r\nevent: greeting\r\ndata: café\r\ndata: 🌍\r\n\r\nretry: 250\rdata: two\r\rdata: three\n\n".as_bytes();

fn fixture_items() -> Vec<Item> {
    vec![
        ev("greeting", "café\n🌍", "1"),
        ev("message", "two", "1"),
        ev("message", "three", "1"),
    ]
}

/// A `next()` dropped while pending loses no byte: the fixture arrives one byte at a time, and
/// every `next()` that has to wait is dropped and a new one started; the events are those of the
/// unsplit input.
#[test]
fn a_dropped_pending_next_loses_no_byte() {
    let gate = Gate::default();
    let mut events = SseReader::default().read(gate.clone());
    let mut seen = Vec::new();
    let mut dropped = 0;
    let mut drain = |events: &mut SseEvents<Gate>, seen: &mut Vec<Item>| loop {
        match poll_once(events.next()) {
            Poll::Ready(Some(next)) => seen.push(item(next)),
            Poll::Ready(None) => return true,
            Poll::Pending => {
                dropped += 1;
                return false;
            }
        }
    };
    for byte in FIXTURE {
        gate.push(&[*byte]);
        assert!(!drain(&mut events, &mut seen), "not the end yet");
    }
    gate.close();
    assert!(drain(&mut events, &mut seen), "the end");
    assert!(
        dropped >= FIXTURE.len(),
        "a pending next() was dropped per byte"
    );
    assert_eq!(seen, fixture_items());
    assert_eq!(events.completion(), Completion::EndOfInput);
    assert_eq!(events.position().bytes, FIXTURE.len() as u64);
    assert_eq!(events.retry(), Some(Duration::from_millis(250)));
}

/// The same fixture split at every position (and in every other partition) gives the same events,
/// the same last event ID and retry, and a clean end.
#[tokio::test]
async fn the_fixture_split_anywhere_reads_the_same() {
    let out = every_partition(FIXTURE, &fixture_items()).await;
    assert_eq!(out.last_event_id, "1");
    assert_eq!(out.retry, Some(Duration::from_millis(250)));
    assert_eq!(out.completion, Completion::EndOfInput);
}

fn poll_stream<S: Stream + Unpin>(stream: &mut S) -> Poll<Option<S::Item>> {
    Pin::new(stream).poll_next(&mut Context::from_waker(Waker::noop()))
}

/// `into_stream()`: the same items, a pending read kept by the stream when its caller stops
/// polling, completion and failure observable, and the watch updated after the stream moved.
#[test]
fn the_record_stream_bridge() {
    let gate = Gate::default();
    let mut stream = SseReader::default().read(gate.clone()).into_stream();
    let watch = stream.watch();
    assert!(poll_stream(&mut stream).is_pending());
    gate.push(b"data: a\n\ndata: b");
    let Poll::Ready(Some(first)) = poll_stream(&mut stream) else {
        panic!("the first event");
    };
    assert_eq!(item(first), msg("a"));
    assert!(poll_stream(&mut stream).is_pending(), "b is unfinished");
    assert_eq!(stream.completion(), Completion::Reading);
    gate.push(b"\n\n");
    gate.close();
    let mut moved = stream;
    let Poll::Ready(Some(second)) = poll_stream(&mut moved) else {
        panic!("the second event");
    };
    assert_eq!(item(second), msg("b"));
    assert!(matches!(poll_stream(&mut moved), Poll::Ready(None)));
    assert!(matches!(poll_stream(&mut moved), Poll::Ready(None)));
    assert_eq!(moved.completion(), Completion::EndOfInput);
    assert_eq!(moved.position().records, 2);
    assert_eq!(moved.failure(), None);
    assert_eq!(watch.completion(), Completion::EndOfInput);
    let reader = moved.into_reader().expect("no read in progress");
    assert_eq!(reader.completion(), Completion::EndOfInput);

    // A truncated stream: the error, then the end; the failure kind is kept.
    let mut stream = SseReader::default()
        .read(Chunks::new([&b"data: a\n\ndata: b\n"[..]]))
        .into_stream();
    let watch = stream.watch();
    let mut items = Vec::new();
    while let Poll::Ready(Some(next)) = poll_stream(&mut stream) {
        items.push(item(next));
    }
    assert_eq!(
        items,
        [msg("a"), err(ReadErrorKind::Format, ENDS_IN_EVENT, 3)]
    );
    assert_eq!(stream.completion(), Completion::Failed);
    assert_eq!(stream.failure(), Some(ReadErrorKind::Format));
    assert_eq!(watch.failure(), Some(ReadErrorKind::Format));
}

/// One large event in one ready chunk, and a long run of comments: every poll interprets at most
/// 64 KiB, consuming the input exactly once, and the event is intact.
#[test]
fn work_per_poll_is_bounded() {
    const BUDGET: u64 = 64 * 1024;
    let data = vec![b'x'; 4 * 1024 * 1024];
    let mut big = b"data: ".to_vec();
    big.extend_from_slice(&data);
    big.extend_from_slice(b"\n\n");
    let comments = b": keep-alive\n".repeat(100_000);
    let input = [comments.as_slice(), &big].concat();
    let mut events = SseReader::new(SseOptions::new().max_event_bytes(8 * 1024 * 1024))
        .read(Chunks::new([input.clone()]));
    let mut polls = 0u64;
    let event = loop {
        let before = events.position().bytes;
        let poll = poll_once(events.next());
        let step = events.position().bytes - before;
        assert!(step <= BUDGET, "{step} bytes in one poll");
        polls += 1;
        if let Poll::Ready(next) = poll {
            break next.expect("an event").expect("no error");
        }
    };
    assert_eq!(event.data().len(), data.len());
    assert_eq!(events.position().bytes, input.len() as u64);
    assert!(polls >= input.len() as u64 / BUDGET, "{polls} polls");
}

fn head(content_types: &[&[u8]]) -> Result<(), &'static str> {
    let mut headers = HeaderMap::new();
    for ct in content_types {
        headers.append(CONTENT_TYPE, HeaderValue::from_bytes(ct).unwrap());
    }
    <Sse as Incremental<Event>>::check_content_type(&ResponseHead::new(
        StatusCode::OK,
        Version::HTTP_11,
        &headers,
    ))
    .map_err(|e| e.label())
}

#[test]
fn the_head_check() {
    assert_eq!(
        <Sse as Incremental<Event>>::ACCEPT,
        Some("text/event-stream")
    );
    for ok in [
        &b"text/event-stream"[..],
        b"TEXT/Event-Stream",
        b"text/event-stream; charset=utf-8",
        b"text/event-stream;charset=UTF-8",
        b"text/event-stream; charset=\"utf-8\"",
        b"text/event-stream; charset=us-ascii",
        b"text/event-stream; x=y",
    ] {
        assert_eq!(head(&[ok]), Ok(()), "{:?}", String::from_utf8_lossy(ok));
    }
    let not_sse = Err("the response Content-Type is not text/event-stream");
    assert_eq!(head(&[]), Err("the response has no Content-Type"));
    assert_eq!(head(&[b"text/plain"]), not_sse);
    assert_eq!(head(&[b"text/*"]), not_sse);
    assert_eq!(head(&[b"application/json"]), not_sse);
    assert_eq!(
        head(&[b"text/event-stream; charset=iso-8859-1"]),
        Err("the response declares a charset other than utf-8")
    );
    assert_eq!(
        head(&[b"text/event-stream; charset=utf-8; charset=utf-8"]),
        Err("the response Content-Type declares charset more than once")
    );
    assert_eq!(
        head(&[b"text/event-stream", b"text/event-stream"]),
        Err("the response has more than one Content-Type field")
    );
    assert_eq!(
        head(&[b"text/event-stream, text/plain"]),
        Err("the response Content-Type is not a media type")
    );
}

#[test]
fn readers_and_futures_are_send() {
    fn send<T: Send>(_: &T) {}
    fn send_sync_clone<T: Send + Sync + Clone>() {}
    send_sync_clone::<SseReader>();
    send_sync_clone::<SseOptions>();
    send_sync_clone::<Event>();
    fn next_future(events: &mut SseEvents) {
        send(&events.next());
    }
    let _ = next_future;
    fn stream(events: SseEvents) {
        send(&events.into_stream());
    }
    let _ = stream;
}

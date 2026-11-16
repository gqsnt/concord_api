//! C11.8 to C11.10, the readers of records: `concord::read::JsonLines` and
//! `concord::read::Sse`, and the `RecordStream` that both return after the head
//! (docs/reference/contracts.md C11.8 to C11.10).
//!
//! Every test goes through a description of `concord::api!` and a generated client over a
//! `concord_testkit::Script`, on Tokio's paused clock: no socket, no wait on real time.
//! The bytes of a body are cut at chosen places, so that a reader is seen to frame records
//! the same way wherever the transport cuts them.

mod common;

use common::engine::{secs, success};
use concord::read::{JsonLines, RecordError, RecordStream, Sse, SseEvent, StreamError};
use concord::Retry;
use concord_testkit::Script;
use futures_core::Stream;
use serde::Deserialize;
use std::error::Error as _;
use std::future::IntoFuture;
use std::pin::Pin;
use std::time::Duration;
use tokio::time::Instant;

/// A record with a number.
#[derive(Debug, Deserialize, PartialEq)]
struct Row {
    n: u32,
}

/// A record with a number and a text, for the splits inside a character.
#[derive(Debug, Deserialize, PartialEq)]
struct Line {
    n: u32,
    s: String,
}

concord::api! {
    client Feed {
        base "https://feed.example.test";
        GET lines() -> RecordStream<Row> { path "lines"; read JsonLines::new(16); }
        GET lines_seven() -> RecordStream<Row> { path "lines"; read JsonLines::new(7); }
        GET lines_zero() -> RecordStream<Row> { path "lines"; read JsonLines::new(0); }
        GET lines_largest() -> RecordStream<Row> { path "lines"; read JsonLines::new(usize::MAX); }
        GET texts() -> RecordStream<Line> { path "texts"; read JsonLines::new(64); }
        GET events() -> RecordStream<SseEvent> { path "events"; read Sse::new(4096); }
        GET events_ten() -> RecordStream<SseEvent> { path "events"; read Sse::new(10); }
        GET events_twenty() -> RecordStream<SseEvent> { path "events"; read Sse::new(20); }
        GET events_zero() -> RecordStream<SseEvent> { path "events"; read Sse::new(0); }
    }
}

/// `Feed` over this script.
fn feed(script: &Script) -> Feed {
    Feed::new().with_transport(script.clone())
}

/// The next attempt of the script answers `200`, with `x-trace: t1`, in these pieces.
fn pieces(script: &Script, pieces: &[&[u8]]) {
    let mut answer = script.answer(200).header("x-trace", "t1");
    for piece in pieces {
        answer = answer.body(piece.to_vec());
    }
}

/// `pieces`, cut at these offsets (in order, each at most the length).
fn cut_at(body: &[u8], cuts: &[usize]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut start = 0;
    for &cut in cuts {
        out.push(body[start..cut].to_vec());
        start = cut;
    }
    out.push(body[start..].to_vec());
    out
}

/// Every way of cutting `body` that the framing tests use: whole, at each offset, at each
/// pair of neighbouring offsets (a piece of one byte between two), and one byte per piece.
fn every_cut(body: &[u8]) -> Vec<Vec<Vec<u8>>> {
    let mut ways = vec![vec![body.to_vec()]];
    for i in 0..=body.len() {
        ways.push(cut_at(body, &[i]));
    }
    for i in 0..body.len() {
        ways.push(cut_at(body, &[i, i + 1]));
    }
    ways.push(body.iter().map(|byte| vec![*byte]).collect());
    ways
}

/// Every record until the end or the first error.
async fn drain<T>(records: &mut RecordStream<T>) -> (Vec<T>, Option<RecordError>) {
    let mut out = Vec::new();
    loop {
        match records.try_next().await {
            Ok(Some(record)) => out.push(record),
            Ok(None) => return (out, None),
            Err(error) => return (out, Some(error)),
        }
    }
}

/// A record, or the test fails with the name of what came instead.
#[track_caller]
fn record<T>(next: Result<Option<T>, RecordError>) -> T {
    match next {
        Ok(Some(record)) => record,
        Ok(None) => panic!("expected a record, got the end"),
        Err(error) => panic!("expected a record, got an error: {}", name(&error)),
    }
}

/// The error, or the test fails.
#[track_caller]
fn failed<T>(next: Result<Option<T>, RecordError>) -> RecordError {
    match next {
        Ok(Some(_)) => panic!("expected an error, got a record"),
        Ok(None) => panic!("expected an error, got the end"),
        Err(error) => error,
    }
}

/// The end, or the test fails.
#[track_caller]
fn end<T>(next: Result<Option<T>, RecordError>) {
    match next {
        Ok(None) => {}
        Ok(Some(_)) => panic!("expected the end, got a record"),
        Err(error) => panic!("expected the end, got an error: {}", name(&error)),
    }
}

/// The variant of an error, without its text (the text may be what is under test).
fn name(error: &RecordError) -> &'static str {
    match error {
        RecordError::Stream(_) => "Stream",
        RecordError::TooLarge { .. } => "TooLarge",
        RecordError::Utf8 { .. } => "Utf8",
        RecordError::Decode { .. } => "Decode",
        _ => "another variant",
    }
}

/// What an error prints, with every error of its source chain.
fn printed(error: &RecordError) -> String {
    let mut text = format!("{error} | {error:?}");
    let mut source = error.source();
    while let Some(cause) = source {
        text.push_str(&format!(" | {cause} | {cause:?}"));
        source = cause.source();
    }
    text
}

/// What an event holds, as a tuple that `assert_eq!` can print (its `Debug` hides it).
fn seen(event: &SseEvent) -> (String, String, String, Option<Duration>) {
    (
        event.event.clone(),
        event.data.clone(),
        event.id.clone(),
        event.retry,
    )
}

/// An expected event.
fn ev(
    event: &str,
    data: &str,
    id: &str,
    retry: Option<u64>,
) -> (String, String, String, Option<Duration>) {
    (
        event.to_owned(),
        data.to_owned(),
        id.to_owned(),
        retry.map(Duration::from_millis),
    )
}

/// The events of one body, whole, with `Sse::new(4096)`.
async fn events_of(
    body: &[u8],
) -> (
    Vec<(String, String, String, Option<Duration>)>,
    Option<RecordError>,
) {
    let script = Script::new();
    pieces(&script, &[body]);
    let mut events = success(feed(&script).events().await);
    let (events, error) = drain(&mut events).await;
    (events.iter().map(seen).collect(), error)
}

/// One pull through `Stream::poll_next`.
async fn poll_once<T>(records: &mut Pin<Box<RecordStream<T>>>) -> Option<Result<T, RecordError>> {
    std::future::poll_fn(|cx| records.as_mut().poll_next(cx)).await
}

// C11.8: a reader of records holds at most its bound -------------------------------------

#[tokio::test(start_paused = true)]
async fn c11_8_json_lines_accepts_a_line_of_exactly_the_bound_and_refuses_one_byte_more() {
    // `{"n":1}` is 7 bytes: bound 7 holds it; `{"n":10}` is 8.
    let script = Script::new();
    pieces(&script, &[b"{\"n\":1}\n{\"n\":2}"]);
    let mut rows = success(feed(&script).lines_seven().await);
    assert_eq!(record(rows.try_next().await), Row { n: 1 });
    assert_eq!(record(rows.try_next().await), Row { n: 2 });
    end(rows.try_next().await);

    let script = Script::new();
    pieces(&script, &[b"{\"n\":1}\n{\"n\":10}\n{\"n\":3}\n"]);
    let mut rows = success(feed(&script).lines_seven().await);
    assert_eq!(record(rows.try_next().await), Row { n: 1 });
    let error = failed(rows.try_next().await);
    assert!(
        matches!(error, RecordError::TooLarge { limit: 7, .. }),
        "{}",
        name(&error)
    );
}

#[tokio::test(start_paused = true)]
async fn c11_8_json_lines_counts_the_cr_of_a_crlf_in_the_bound() {
    // "The bound counts bytes before LF, including an optional CR."
    let script = Script::new();
    pieces(&script, &[b"{\"n\":1}\r\n"]);
    let mut rows = success(feed(&script).lines_seven().await);
    let error = failed(rows.try_next().await);
    assert!(
        matches!(error, RecordError::TooLarge { limit: 7, .. }),
        "{}",
        name(&error)
    );
}

#[tokio::test(start_paused = true)]
async fn c11_8_a_last_line_without_lf_over_the_bound_is_too_large() {
    let script = Script::new();
    pieces(&script, &[b"{\"n\":1}\n{\"n\":12345}"]);
    let mut rows = success(feed(&script).lines_seven().await);
    assert_eq!(record(rows.try_next().await), Row { n: 1 });
    let error = failed(rows.try_next().await);
    assert!(
        matches!(error, RecordError::TooLarge { limit: 7, .. }),
        "{}",
        name(&error)
    );
}

#[tokio::test(start_paused = true)]
async fn c11_8_a_line_over_the_bound_fails_without_waiting_for_its_end() {
    // The body gives 17 bytes with no LF, then never another byte. A reader that waits for
    // the LF before it counts holds more than its bound, and ends on the idle timeout.
    let script = Script::new();
    script
        .answer(200)
        .body(b"{\"n\":1,\"padding\":".to_vec())
        .stall();
    let start = Instant::now();
    let mut rows = success(feed(&script).lines().await);
    let error = failed(rows.try_next().await);
    assert!(
        matches!(error, RecordError::TooLarge { limit: 16, .. }),
        "{}",
        name(&error)
    );
    assert_eq!(start.elapsed(), Duration::ZERO);
}

#[tokio::test(start_paused = true)]
async fn c11_8_a_bound_of_zero_permits_no_nonempty_line_and_the_largest_bound_is_no_overflow() {
    let script = Script::new();
    pieces(&script, &[b"{\"n\":1}\n"]);
    let mut rows = success(feed(&script).lines_zero().await);
    let error = failed(rows.try_next().await);
    assert!(
        matches!(error, RecordError::TooLarge { limit: 0, .. }),
        "{}",
        name(&error)
    );

    // An empty body has no record, whatever the bound.
    let script = Script::new();
    pieces(&script, &[b""]);
    let mut rows = success(feed(&script).lines_zero().await);
    end(rows.try_next().await);

    let script = Script::new();
    pieces(&script, &[b"{\"n\":1}\r\n", b"{\"n\":2}"]);
    let mut rows = success(feed(&script).lines_largest().await);
    assert_eq!(record(rows.try_next().await), Row { n: 1 });
    assert_eq!(record(rows.try_next().await), Row { n: 2 });
    end(rows.try_next().await);
}

#[tokio::test(start_paused = true)]
async fn c11_8_sse_bound_counts_every_nonblank_line_plus_one_delimiter() {
    // `data: abc` is 9 bytes, plus one delimiter: 10. The blank line does not count, and a
    // CRLF counts as one delimiter.
    for body in [
        &b"data: abc\n\n"[..],
        b"data: abc\r\n\r\n",
        b"data: abc\r\r",
    ] {
        let script = Script::new();
        pieces(&script, &[body]);
        let mut events = success(feed(&script).events_ten().await);
        let event = record(events.try_next().await);
        assert_eq!(seen(&event), ev("message", "abc", "", None), "{body:?}");
        end(events.try_next().await);
    }
    let script = Script::new();
    pieces(&script, &[b"data: abcd\n\n"]);
    let mut events = success(feed(&script).events_ten().await);
    let error = failed(events.try_next().await);
    assert!(
        matches!(error, RecordError::TooLarge { limit: 10, .. }),
        "{}",
        name(&error)
    );
}

#[tokio::test(start_paused = true)]
async fn c11_8_sse_comments_and_unknown_fields_count_toward_the_bound() {
    // `data: abc` (10) and `: xxxxxxx` (10) is 20: bound 20 holds it.
    let script = Script::new();
    pieces(&script, &[b": xxxxxxx\ndata: abc\n\n"]);
    let mut events = success(feed(&script).events_twenty().await);
    assert_eq!(
        seen(&record(events.try_next().await)),
        ev("message", "abc", "", None)
    );
    // One more byte, in a comment or in a field that is ignored, is over it.
    for body in [
        &b": xxxxxxxx\ndata: abc\n\n"[..],
        b"nonsense:x\ndata: abc\n\n",
    ] {
        let script = Script::new();
        pieces(&script, &[body]);
        let mut events = success(feed(&script).events_twenty().await);
        let error = failed(events.try_next().await);
        assert!(
            matches!(error, RecordError::TooLarge { limit: 20, .. }),
            "{body:?}: {}",
            name(&error)
        );
    }
}

#[tokio::test(start_paused = true)]
async fn c11_8_many_short_sse_comments_without_a_blank_line_are_too_large() {
    // Comments with no blank line between them are one block: a stream of keep-alives
    // cannot grow the reader without end.
    let script = Script::new();
    {
        let mut answer = script.answer(200);
        for _ in 0..10 {
            answer = answer.body(b":\n".to_vec());
        }
        answer.stall();
    }
    let start = Instant::now();
    let mut events = success(feed(&script).events_ten().await);
    let error = failed(events.try_next().await);
    assert!(
        matches!(error, RecordError::TooLarge { limit: 10, .. }),
        "{}",
        name(&error)
    );
    assert_eq!(start.elapsed(), Duration::ZERO);
}

#[tokio::test(start_paused = true)]
async fn c11_8_an_unfinished_sse_line_over_the_bound_fails_without_waiting() {
    let script = Script::new();
    script
        .answer(200)
        .body(b"data: 0123456789012345".to_vec())
        .stall();
    let start = Instant::now();
    let mut events = success(feed(&script).events_ten().await);
    let error = failed(events.try_next().await);
    assert!(
        matches!(error, RecordError::TooLarge { limit: 10, .. }),
        "{}",
        name(&error)
    );
    assert_eq!(start.elapsed(), Duration::ZERO);
}

#[tokio::test(start_paused = true)]
async fn c11_8_an_sse_bound_of_zero_permits_blank_lines_and_no_event() {
    let script = Script::new();
    pieces(&script, &[b"\n\n\r\n\r"]);
    let mut events = success(feed(&script).events_zero().await);
    end(events.try_next().await);

    let script = Script::new();
    pieces(&script, &[b"data: a\n\n"]);
    let mut events = success(feed(&script).events_zero().await);
    let error = failed(events.try_next().await);
    assert!(
        matches!(error, RecordError::TooLarge { limit: 0, .. }),
        "{}",
        name(&error)
    );
}

#[tokio::test(start_paused = true)]
async fn c11_8_an_sse_bom_does_not_count_toward_the_bound() {
    // `data: abc` and its delimiter are 10 bytes; the BOM before them is 3 more, and the
    // bound of 10 still holds the event, wherever the bytes are cut.
    let body = "\u{FEFF}data: abc\n\n".as_bytes();
    for way in every_cut(body) {
        let script = Script::new();
        let parts: Vec<&[u8]> = way.iter().map(Vec::as_slice).collect();
        pieces(&script, &parts);
        let mut events = success(feed(&script).events_ten().await);
        let (events, error) = drain(&mut events).await;
        assert!(
            error.is_none(),
            "{way:?}: {}",
            error.as_ref().map_or("", name)
        );
        let events: Vec<_> = events.iter().map(seen).collect();
        assert_eq!(events, vec![ev("message", "abc", "", None)], "{way:?}");
    }
    // The bound still applies to what follows the BOM.
    let script = Script::new();
    pieces(&script, &["\u{FEFF}data: abcd\n\n".as_bytes()]);
    let mut events = success(feed(&script).events_ten().await);
    let error = failed(events.try_next().await);
    assert!(
        matches!(error, RecordError::TooLarge { limit: 10, .. }),
        "{}",
        name(&error)
    );
    // A bound of zero: a BOM and blank lines are no event and no error.
    let script = Script::new();
    pieces(&script, &["\u{FEFF}\n\n".as_bytes()]);
    let mut events = success(feed(&script).events_zero().await);
    end(events.try_next().await);
}

#[tokio::test(start_paused = true)]
async fn c11_8_bytes_that_are_not_utf8_are_a_utf8_error_with_its_record_and_cause() {
    let script = Script::new();
    pieces(
        &script,
        &[b"{\"n\":1,\"s\":\"a\"}\n{\"n\":2,\"s\":\"\xff\"}\n"],
    );
    let mut texts = success(feed(&script).texts().await);
    assert_eq!(record(texts.try_next().await).n, 1);
    let error = failed(texts.try_next().await);
    assert!(
        matches!(error, RecordError::Utf8 { record: 2, .. }),
        "{}",
        name(&error)
    );
    let cause = error.source().expect("the UTF-8 error is the source");
    assert!(cause.downcast_ref::<std::str::Utf8Error>().is_some());

    // Events count their blocks.
    let script = Script::new();
    pieces(&script, &[b"data: a\n\ndata: \xc3\x28\n\n"]);
    let mut events = success(feed(&script).events().await);
    assert_eq!(seen(&record(events.try_next().await)).1, "a");
    let error = failed(events.try_next().await);
    assert!(
        matches!(error, RecordError::Utf8 { record: 2, .. }),
        "{}",
        name(&error)
    );
}

#[tokio::test(start_paused = true)]
async fn c11_8_a_character_cut_by_the_end_of_the_body_is_a_utf8_error() {
    // The last line, without LF, ends in the first byte of `é`.
    let script = Script::new();
    pieces(
        &script,
        &[b"{\"n\":1,\"s\":\"a\"}\n{\"n\":2,\"s\":\"", b"\xc3"],
    );
    let mut texts = success(feed(&script).texts().await);
    assert_eq!(record(texts.try_next().await).n, 1);
    let error = failed(texts.try_next().await);
    assert!(
        matches!(error, RecordError::Utf8 { record: 2, .. }),
        "{}",
        name(&error)
    );
}

#[tokio::test(start_paused = true)]
async fn c11_8_a_record_that_does_not_decode_is_a_decode_error_with_its_line_number() {
    let script = Script::new();
    pieces(
        &script,
        &[b"{\"n\":1}\n{\"n\":2}\n{\"n\":\"leak-me\"}\n{\"n\":4}\n"],
    );
    let mut rows = success(feed(&script).lines().await);
    assert_eq!(record(rows.try_next().await), Row { n: 1 });
    assert_eq!(record(rows.try_next().await), Row { n: 2 });
    let error = failed(rows.try_next().await);
    assert!(
        matches!(
            error,
            RecordError::Decode {
                record: Some(3),
                line: 1,
                ..
            }
        ),
        "{}",
        name(&error)
    );
}

#[tokio::test(start_paused = true)]
async fn c11_8_the_errors_of_a_record_print_no_byte_of_it() {
    let cases: [(&[u8], &str); 3] = [
        (b"{\"n\":\"leak-me\"}\n", "Decode"),
        (b"{\"n\":1, \"leak-me\":\"leak-me\"}\n", "TooLarge"),
        (b"{\"n\":\"leak-me\xff\"}\n", "Utf8"),
    ];
    for (body, variant) in cases {
        let script = Script::new();
        pieces(&script, &[body]);
        let mut rows = success(feed(&script).lines().await);
        let error = failed(rows.try_next().await);
        assert_eq!(name(&error), variant);
        assert!(
            !printed(&error).contains("leak"),
            "{variant} prints a byte of the record"
        );
    }
    // An event that does not decode, through the helper of the event.
    let event = SseEvent::new("{\"n\":\"leak-me\"}");
    let error = match event.json::<Row>() {
        Ok(_) => panic!("expected a Decode error"),
        Err(error) => error,
    };
    assert!(
        matches!(error, RecordError::Decode { record: None, .. }),
        "{}",
        name(&error)
    );
    assert!(!printed(&error).contains("leak"));
}

#[tokio::test(start_paused = true)]
async fn c11_8_an_event_and_a_stream_print_no_value() {
    let mut event = SseEvent::new("leak-data");
    assert_eq!(seen(&event), ev("message", "leak-data", "", None));
    event.event = "leak-type".into();
    event.id = "leak-id".into();
    event.retry = Some(Duration::from_millis(1500));
    let text = format!("{event:?} {event:#?}");
    assert!(!text.contains("leak"), "{text}");

    let script = Script::new();
    script
        .answer(200)
        .header("x-trace", "leak-header")
        .body(b"data: leak-data\n\n".to_vec());
    let events = success(feed(&script).events().await);
    let text = format!("{events:?} {events:#?}");
    assert!(!text.contains("leak"), "{text}");
}

#[tokio::test(start_paused = true)]
async fn c11_8_the_first_error_ends_the_stream_for_try_next_and_for_poll_next() {
    // Good records after the error are never given.
    let body: &[u8] = b"{\"n\":1}\n{\"n\":\"x\"}\n{\"n\":3}\n{\"n\":4}\n";
    let script = Script::new();
    pieces(&script, &[body]);
    let mut rows = success(feed(&script).lines().await);
    assert_eq!(record(rows.try_next().await), Row { n: 1 });
    let _ = failed(rows.try_next().await);
    end(rows.try_next().await);
    end(rows.try_next().await);

    let script = Script::new();
    pieces(&script, &[body]);
    let mut rows = Box::pin(success(feed(&script).lines().await));
    assert!(matches!(poll_once(&mut rows).await, Some(Ok(Row { n: 1 }))));
    assert!(matches!(poll_once(&mut rows).await, Some(Err(_))));
    assert!(poll_once(&mut rows).await.is_none());
    assert!(poll_once(&mut rows).await.is_none());
}

// C11.9: records are framed the same way wherever the bytes are cut -----------------------

#[tokio::test(start_paused = true)]
async fn c11_9_json_lines_every_cut_gives_the_same_records() {
    // LF, CRLF, a last line without LF, characters of two and three bytes.
    let body =
        "{\"n\":1,\"s\":\"é\"}\n{\"n\":2,\"s\":\"日本\"}\r\n{\"n\":3,\"s\":\"x\"}".as_bytes();
    let expected = vec![
        Line {
            n: 1,
            s: "é".into(),
        },
        Line {
            n: 2,
            s: "日本".into(),
        },
        Line {
            n: 3,
            s: "x".into(),
        },
    ];
    for way in every_cut(body) {
        let script = Script::new();
        let parts: Vec<&[u8]> = way.iter().map(Vec::as_slice).collect();
        pieces(&script, &parts);
        let mut texts = success(feed(&script).texts().await);
        let (records, error) = drain(&mut texts).await;
        assert!(
            error.is_none(),
            "{way:?}: {}",
            error.as_ref().map_or("", name)
        );
        assert_eq!(records, expected, "{way:?}");
    }
}

#[tokio::test(start_paused = true)]
async fn c11_9_json_lines_a_final_lf_ends_the_last_line_and_adds_none() {
    for body in [&b"{\"n\":1}\n"[..], b"{\"n\":1}\r\n", b"{\"n\":1}"] {
        let script = Script::new();
        pieces(&script, &[body]);
        let mut rows = success(feed(&script).lines().await);
        assert_eq!(record(rows.try_next().await), Row { n: 1 }, "{body:?}");
        end(rows.try_next().await);
    }
}

#[tokio::test(start_paused = true)]
async fn c11_9_json_lines_an_empty_line_is_a_decode_error() {
    for body in [
        &b"{\"n\":1}\n\n{\"n\":3}\n"[..],
        b"{\"n\":1}\r\n\r\n{\"n\":3}\n",
        b"{\"n\":1}\n\n",
    ] {
        let script = Script::new();
        pieces(&script, &[body]);
        let mut rows = success(feed(&script).lines().await);
        assert_eq!(record(rows.try_next().await), Row { n: 1 }, "{body:?}");
        let error = failed(rows.try_next().await);
        assert!(
            matches!(
                error,
                RecordError::Decode {
                    record: Some(2),
                    ..
                }
            ),
            "{body:?}: {}",
            name(&error)
        );
        end(rows.try_next().await);
    }
    // At the start, too.
    let script = Script::new();
    pieces(&script, &[b"\n{\"n\":2}\n"]);
    let mut rows = success(feed(&script).lines().await);
    let error = failed(rows.try_next().await);
    assert!(
        matches!(
            error,
            RecordError::Decode {
                record: Some(1),
                ..
            }
        ),
        "{}",
        name(&error)
    );
}

#[tokio::test(start_paused = true)]
async fn c11_9_json_lines_an_empty_line_is_a_decode_error_whatever_the_bound() {
    // At a bound of zero too, and wherever a CRLF is cut. The error has no source,
    // and its column is the decoder's, zero at the start of the empty line.
    let empty_lf: &[u8] = b"\n";
    let empty_crlf: &[u8] = b"\r\n";
    for body in [empty_lf, empty_crlf] {
        for way in every_cut(body) {
            for bound in [0, 7, 16] {
                let script = Script::new();
                let parts: Vec<&[u8]> = way.iter().map(Vec::as_slice).collect();
                pieces(&script, &parts);
                let client = feed(&script);
                let mut rows = match bound {
                    0 => success(client.lines_zero().await),
                    7 => success(client.lines_seven().await),
                    _ => success(client.lines().await),
                };
                let error = failed(rows.try_next().await);
                assert!(
                    matches!(
                        error,
                        RecordError::Decode {
                            record: Some(1),
                            line: 1,
                            column: 0,
                            ..
                        }
                    ),
                    "bound {bound}, {way:?}: {}",
                    name(&error)
                );
                assert!(error.source().is_none(), "bound {bound}, {way:?}");
                end(rows.try_next().await);
            }
        }
    }
}

#[tokio::test(start_paused = true)]
async fn c11_8_a_decode_error_has_no_source() {
    // For a line and for the helper of an event (whose column can be zero too).
    let script = Script::new();
    pieces(&script, &[b"{\"n\":\"leak-me\"}\n"]);
    let mut rows = success(feed(&script).lines().await);
    let error = failed(rows.try_next().await);
    assert!(
        matches!(
            error,
            RecordError::Decode {
                record: Some(1),
                line: 1,
                ..
            }
        ),
        "{}",
        name(&error)
    );
    assert!(error.source().is_none());

    let error = match SseEvent::new("").json::<Row>() {
        Ok(_) => panic!("expected a Decode error"),
        Err(error) => error,
    };
    assert!(
        matches!(
            error,
            RecordError::Decode {
                record: None,
                line: 1,
                column: 0,
                ..
            }
        ),
        "{}",
        name(&error)
    );
    assert!(error.source().is_none());
}

#[tokio::test(start_paused = true)]
async fn c11_9_json_lines_two_values_on_one_line_are_a_decode_error() {
    let script = Script::new();
    pieces(&script, &[b"{\"n\":1}{\"n\":2}\n"]);
    let mut rows = success(feed(&script).lines().await);
    let error = failed(rows.try_next().await);
    assert!(
        matches!(
            error,
            RecordError::Decode {
                record: Some(1),
                ..
            }
        ),
        "{}",
        name(&error)
    );
}

/// A body that uses every rule of the framing: a BOM, a comment, CRLF, lone CR and LF line
/// ends, `event`, `data` joined, `id` kept and reset, `retry` kept and an invalid one
/// ignored, characters of two and three bytes, and an event with no blank line at the end.
const SSE_BODY: &str =
    "\u{FEFF}: hello\r\nevent: delta\r\ndata: é1\r\ndata:two\r\nid: 7\r\nretry: 1500\r\n\r\n\
data: second\r\r\
id\ndata: third\n\n\
event: x\nretry: abc\ndata: 日本\n\n\
data: lost\n";

fn sse_expected() -> Vec<(String, String, String, Option<Duration>)> {
    vec![
        ev("delta", "é1\ntwo", "7", Some(1500)),
        ev("message", "second", "7", Some(1500)),
        ev("message", "third", "", Some(1500)),
        ev("x", "日本", "", Some(1500)),
    ]
}

#[tokio::test(start_paused = true)]
async fn c11_9_sse_every_cut_gives_the_same_events() {
    // A cut between the CR and the LF of a CRLF is one line end, not two: a reader that
    // takes the CR at the end of a piece for a line end dispatches early.
    for way in every_cut(SSE_BODY.as_bytes()) {
        let script = Script::new();
        let parts: Vec<&[u8]> = way.iter().map(Vec::as_slice).collect();
        pieces(&script, &parts);
        let mut events = success(feed(&script).events().await);
        let (events, error) = drain(&mut events).await;
        assert!(
            error.is_none(),
            "{way:?}: {}",
            error.as_ref().map_or("", name)
        );
        let events: Vec<_> = events.iter().map(seen).collect();
        assert_eq!(events, sse_expected(), "{way:?}");
    }
}

#[tokio::test(start_paused = true)]
async fn c11_9_sse_lf_crlf_and_cr_frame_alike() {
    for body in [
        &b"data: a\ndata: b\n\ndata: c\n\n"[..],
        b"data: a\r\ndata: b\r\n\r\ndata: c\r\n\r\n",
        b"data: a\rdata: b\r\rdata: c\r\r",
        b"data: a\r\ndata: b\r\rdata: c\n\n",
    ] {
        let (events, error) = events_of(body).await;
        assert!(error.is_none(), "{body:?}");
        assert_eq!(
            events,
            vec![
                ev("message", "a\nb", "", None),
                ev("message", "c", "", None)
            ],
            "{body:?}"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn c11_9_sse_only_the_first_bom_is_skipped() {
    let (events, error) = events_of("\u{FEFF}data: a\n\n\u{FEFF}data: b\n\n".as_bytes()).await;
    assert!(error.is_none());
    // The second BOM is part of a field name that is not `data`: that block has no data.
    assert_eq!(events, vec![ev("message", "a", "", None)]);
}

#[tokio::test(start_paused = true)]
async fn c11_9_sse_fields_follow_the_rules_of_server_sent_events() {
    let body = b": comment\n\
unknown: x\n\
data\n\n\
data:  two spaces\n\n\
data:x\n\n\
event: only-type\n\n\
event: typed\ndata: t\n\n\
data: back to message\n\n";
    let (events, error) = events_of(body).await;
    assert!(error.is_none());
    assert_eq!(
        events,
        vec![
            // `data` with no colon: an empty data line still dispatches.
            ev("message", "", "", None),
            // Only one space after the colon is taken away.
            ev("message", " two spaces", "", None),
            ev("message", "x", "", None),
            // A block with no data dispatches nothing; its type does not carry over.
            ev("typed", "t", "", None),
            ev("message", "back to message", "", None),
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn c11_9_sse_id_is_kept_reset_by_an_empty_id_and_ignored_with_nul() {
    let body = b"id: 1\ndata: a\n\n\
data: b\n\n\
id: x\0y\ndata: c\n\n\
id:\ndata: d\n\n\
id: 2\n\n\
data: e\n\n";
    let (events, error) = events_of(body).await;
    assert!(error.is_none());
    assert_eq!(
        events,
        vec![
            ev("message", "a", "1", None),
            ev("message", "b", "1", None),
            ev("message", "c", "1", None),
            ev("message", "d", "", None),
            // An `id` in a block with no data still sets the last ID.
            ev("message", "e", "2", None),
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn c11_9_sse_retry_is_a_duration_in_milliseconds_and_an_invalid_one_is_ignored() {
    let body = b"data: a\n\n\
retry: 1500\ndata: b\n\n\
retry: 1.5\ndata: c\n\n\
retry: -1\ndata: d\n\n\
retry: 12a\ndata: e\n\n\
retry: 0\ndata: g\n\n";
    let (events, error) = events_of(body).await;
    assert!(error.is_none());
    assert_eq!(
        events,
        vec![
            ev("message", "a", "", None),
            ev("message", "b", "", Some(1500)),
            ev("message", "c", "", Some(1500)),
            ev("message", "d", "", Some(1500)),
            ev("message", "e", "", Some(1500)),
            ev("message", "g", "", Some(0)),
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn c11_9_sse_a_retry_that_is_empty_not_digits_or_too_large_is_ignored() {
    // `u64::MAX` milliseconds fits; one more does not. Only one space after the colon is
    // taken away, so a second one is not a digit.
    let body = "retry: 1500\ndata: a\n\n\
retry:\ndata: b\n\n\
retry: \ndata: c\n\n\
retry: +5\ndata: d\n\n\
retry: 0x10\ndata: e\n\n\
retry: \u{0661}\u{0662}\ndata: f\n\n\
retry: 18446744073709551616\ndata: g\n\n\
retry: 18446744073709551615\ndata: h\n\n\
retry:  7\ndata: i\n\n\
retry: 7 \ndata: j\n\n";
    let (events, error) = events_of(body.as_bytes()).await;
    assert!(error.is_none());
    let max = Some(u64::MAX);
    assert_eq!(
        events,
        vec![
            ev("message", "a", "", Some(1500)),
            ev("message", "b", "", Some(1500)),
            ev("message", "c", "", Some(1500)),
            ev("message", "d", "", Some(1500)),
            ev("message", "e", "", Some(1500)),
            ev("message", "f", "", Some(1500)),
            ev("message", "g", "", Some(1500)),
            ev("message", "h", "", max),
            ev("message", "i", "", max),
            ev("message", "j", "", max),
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn c11_9_sse_an_event_with_no_blank_line_at_the_end_is_dropped() {
    for body in [
        &b"data: a\n\ndata: b\n"[..],
        b"data: a\n\ndata: b",
        b"data: a\n\ndata: b\r",
        b"data: a\r\n\r\ndata: b\r\n",
    ] {
        let (events, error) = events_of(body).await;
        assert!(error.is_none(), "{body:?}");
        assert_eq!(events, vec![ev("message", "a", "", None)], "{body:?}");
    }
}

#[tokio::test(start_paused = true)]
async fn c11_9_sse_event_json_decodes_the_data() {
    let mut event = SseEvent::new("{\"n\":5}");
    event.event = "delta".into();
    assert_eq!(event.json::<Row>().ok(), Some(Row { n: 5 }));
    // Two data lines are one JSON text, joined by LF.
    let (events, _) = events_of(b"data: {\"n\":\ndata: 6}\n\n").await;
    let mut event = SseEvent::new(events[0].1.clone());
    event.id = "1".into();
    assert_eq!(event.json::<Row>().ok(), Some(Row { n: 6 }));
}

// C11.10: a reader of records never asks the server again ---------------------------------

#[tokio::test(start_paused = true)]
async fn c11_10_a_cut_body_is_a_stream_error_kept_whole_and_nothing_is_sent_again() {
    let script = Script::new();
    script
        .answer(200)
        .header("x-trace", "t1")
        .body(b"{\"n\":1}\n{\"n\":".to_vec())
        .cut();
    pieces(&script, &[b"{\"n\":9}\n"]);
    let mut rows = success(feed(&script).with_retry(Retry::standard()).lines().await);
    assert_eq!(record(rows.try_next().await), Row { n: 1 });
    let error = failed(rows.try_next().await);
    let RecordError::Stream(body) = &error else {
        panic!("expected a Stream error, got {}", name(&error));
    };
    assert_eq!(body.kind(), concord::error::Kind::Transport);
    assert!(body.is_status(200));
    assert_eq!(body.endpoint(), "feed.lines");
    assert_eq!(body.tries(), 1);
    assert_eq!(
        body.headers().and_then(|headers| headers.get("x-trace")),
        Some("t1")
    );
    let source = error.source().expect("the stream error is the source");
    assert!(source.downcast_ref::<StreamError>().is_some());
    end(rows.try_next().await);

    tokio::time::advance(secs(120)).await;
    assert_eq!(script.requests().len(), 1);
    assert_eq!(script.unused(), 1);
}

#[tokio::test(start_paused = true)]
async fn c11_10_no_framing_error_sends_the_request_again() {
    let bodies: [&[u8]; 4] = [
        b"{\"n\":1}\n{\"n\":\"x\"}\n",
        b"{\"n\":1}\n\n",
        b"{\"n\":1}\n{\"n\":1234567890123}\n",
        b"{\"n\":1}\n\xff\n",
    ];
    for body in bodies {
        let script = Script::new();
        pieces(&script, &[body]);
        pieces(&script, &[b"{\"n\":9}\n"]);
        let mut rows = success(feed(&script).with_retry(Retry::standard()).lines().await);
        let (records, error) = drain(&mut rows).await;
        assert_eq!(records, vec![Row { n: 1 }], "{body:?}");
        assert!(error.is_some(), "{body:?}");
        end(rows.try_next().await);
        tokio::time::advance(secs(120)).await;
        assert_eq!(script.requests().len(), 1, "{body:?}");
        assert_eq!(script.unused(), 1, "{body:?}");
    }
}

#[tokio::test(start_paused = true)]
async fn c11_10_the_status_and_the_headers_stay_readable_after_the_end_and_after_an_error() {
    let script = Script::new();
    script
        .answer(206)
        .header("x-trace", "t1")
        .header("content-type", "text/event-stream")
        .body(b"data: a\n\n".to_vec());
    let mut events = success(feed(&script).events().await);
    assert_eq!(events.status().as_u16(), 206);
    assert_eq!(events.headers().get("x-trace"), Some("t1"));
    let _ = record(events.try_next().await);
    end(events.try_next().await);
    assert_eq!(events.status().as_u16(), 206);
    assert_eq!(
        events.headers().get("content-type"),
        Some("text/event-stream")
    );

    let script = Script::new();
    script
        .answer(200)
        .header("x-trace", "t2")
        .body(b"data: a\n".to_vec())
        .cut();
    let mut events = success(feed(&script).events().await);
    let _ = failed(events.try_next().await);
    assert_eq!(events.status().as_u16(), 200);
    assert_eq!(events.headers().get("x-trace"), Some("t2"));
}

#[tokio::test(start_paused = true)]
async fn c11_10_a_pull_that_is_dropped_loses_no_byte() {
    // The first piece ends in the first byte of `é`; the rest comes 10 s later.
    let script = Script::new();
    script
        .answer(200)
        .body(b"{\"n\":1,\"s\":\"\xc3".to_vec())
        .pause(secs(10))
        .body(b"\xa9\"}\n{\"n\":2,\"s\":\"b\"}\n".to_vec());
    let start = Instant::now();
    let mut texts = success(feed(&script).texts().await);
    assert!(tokio::time::timeout(secs(5), texts.try_next())
        .await
        .is_err());
    assert!(tokio::time::timeout(secs(1), texts.try_next())
        .await
        .is_err());
    let first = record(texts.try_next().await);
    assert_eq!(
        first,
        Line {
            n: 1,
            s: "é".into()
        }
    );
    assert_eq!(start.elapsed(), secs(10));
    assert_eq!(record(texts.try_next().await).n, 2);
    end(texts.try_next().await);
    assert_eq!(script.requests().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn c11_10_a_poll_that_is_abandoned_loses_no_byte_of_an_event() {
    let script = Script::new();
    script
        .answer(200)
        .body(b"data: a\r".to_vec())
        .pause(secs(10))
        .body(b"\ndata: b\r\n\r\n".to_vec());
    let mut events = Box::pin(success(feed(&script).events().await));
    assert!(tokio::time::timeout(secs(5), poll_once(&mut events))
        .await
        .is_err());
    // `try_next` and `Stream` drive the same parser.
    let event = record(events.try_next().await);
    assert_eq!(seen(&event), ev("message", "a\nb", "", None));
    end(events.try_next().await);
}

#[tokio::test(start_paused = true)]
async fn c11_10_dropping_the_stream_releases_the_body_and_its_permit() {
    // `max_in_flight` holds its permit until the body ends, fails or is dropped (C11.7).
    let limit = concord::limit::Limit::new().max_in_flight(1);
    let script = Script::new();
    script.answer(200).body(b"{\"n\":1}\n".to_vec()).stall();
    pieces(&script, &[b"{\"n\":2}\n"]);
    let client = feed(&script).with_middleware(limit);

    let mut first = success(client.lines().await);
    assert_eq!(record(first.try_next().await), Row { n: 1 });
    assert!(tokio::time::timeout(secs(1), client.lines().into_future())
        .await
        .is_err());
    assert_eq!(script.requests().len(), 1);

    drop(first);
    let mut second = success(client.lines().await);
    assert_eq!(script.requests().len(), 2);
    assert_eq!(record(second.try_next().await), Row { n: 2 });
}

#[tokio::test(start_paused = true)]
async fn c11_10_the_idle_timeout_watches_bytes_and_not_records() {
    // A comment every 20 s keeps an event stream alive for 100 s with the default 30 s.
    let script = Script::new();
    {
        let mut answer = script.answer(200);
        for _ in 0..5 {
            answer = answer.pause(secs(20)).body(b": ping\n".to_vec());
        }
        answer.body(b"data: a\n\n".to_vec());
    }
    let start = Instant::now();
    let mut events = success(feed(&script).events().await);
    // `: ping` five times is 35 bytes: within the bound of 4096.
    let event = record(events.try_next().await);
    assert_eq!(seen(&event).1, "a");
    assert_eq!(start.elapsed(), secs(100));

    // A record that arrives one byte every 20 s is never silence.
    let script = Script::new();
    {
        let mut answer = script.answer(200);
        for byte in b"{\"n\":1}\n" {
            answer = answer.pause(secs(20)).body(vec![*byte]);
        }
    }
    let start = Instant::now();
    let mut rows = success(feed(&script).lines().await);
    assert_eq!(record(rows.try_next().await), Row { n: 1 });
    assert_eq!(start.elapsed(), secs(160));

    // 31 s of silence while a pull waits is the idle timeout, at 30 s.
    let script = Script::new();
    script
        .answer(200)
        .body(b"{\"n\":1}\n".to_vec())
        .pause(secs(31))
        .body(b"{\"n\":2}\n".to_vec());
    let mut rows = success(feed(&script).lines().await);
    assert_eq!(record(rows.try_next().await), Row { n: 1 });
    let start = Instant::now();
    let error = failed(rows.try_next().await);
    let RecordError::Stream(body) = &error else {
        panic!("expected a Stream error, got {}", name(&error));
    };
    assert!(body.is_timeout());
    assert!(body.is_status(200));
    assert_eq!(start.elapsed(), secs(30));
    end(rows.try_next().await);
    assert_eq!(script.requests().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn c11_10_the_time_between_two_pulls_is_not_silence() {
    let script = Script::new();
    script
        .answer(200)
        .body(b"{\"n\":1}\n".to_vec())
        .pause(secs(20))
        .body(b"{\"n\":2}\n".to_vec());
    let start = Instant::now();
    let mut rows = success(feed(&script).lines().with_idle_timeout(secs(30)).await);
    assert_eq!(record(rows.try_next().await), Row { n: 1 });
    tokio::time::advance(secs(60)).await;
    assert_eq!(record(rows.try_next().await), Row { n: 2 });
    assert!(start.elapsed() >= secs(60) && start.elapsed() <= secs(80));
}

#[tokio::test(start_paused = true)]
async fn c11_10_the_call_timeout_runs_until_the_end_of_the_records() {
    // Comments every 2 s: never silence, but the call timeout of 10 s ends it, at 10 s from
    // the start of the call.
    let script = Script::new();
    {
        let mut answer = script.answer(200);
        for _ in 0..20 {
            answer = answer.pause(secs(2)).body(b":\n".to_vec());
        }
        answer.body(b"data: late\n\n".to_vec());
    }
    let start = Instant::now();
    let mut events = success(feed(&script).events().with_timeout(secs(10)).await);
    let error = failed(events.try_next().await);
    let RecordError::Stream(body) = &error else {
        panic!("expected a Stream error, got {}", name(&error));
    };
    assert!(body.is_timeout());
    assert_eq!(start.elapsed(), secs(10));
    end(events.try_next().await);
    assert_eq!(script.requests().len(), 1);
}

//! Contract tests for bounded, cancel-safe record readers.

#![cfg(feature = "json")]

use concord::error::{Class, Kind};
use concord::read::{Incoming, Reader, RecordStream, Sse, SseEvent};
use concord_extras::read::{JsonLines, RecordError};
use concord_testkit::Script;
use futures_core::Stream;
use futures_util::StreamExt;
use serde::Deserialize;
use std::error::Error as _;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::time::Instant;

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct Row {
    n: u32,
    text: String,
}

concord::api! {
    client RecordsApi {
        base "https://records.test";
        GET json() -> RecordStream<Row> { path "json"; read JsonLines::new(64); }
        GET events() -> RecordStream<SseEvent> { path "sse"; read Sse::new(128); }
        GET events_ten() -> RecordStream<SseEvent> { path "sse"; read Sse::new(10); }
        GET events_one() -> RecordStream<SseEvent> { path "sse"; read Sse::new(1); }
        GET bytes() -> concord::ByteStream { path "bytes"; read concord::read::Stream; }
    }
}

fn assert_one_send(script: &Script) {
    let requests = script.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests.first().map(|request| request.info().number()),
        Some(1)
    );
}

fn poll_once<F: Future + ?Sized>(future: Pin<&mut F>) -> Poll<F::Output> {
    future.poll(&mut Context::from_waker(
        futures_util::task::noop_waker_ref(),
    ))
}

fn poll_stream<T>(records: &mut RecordStream<T>) -> Poll<Option<Result<T, RecordError>>> {
    Pin::new(records).poll_next(&mut Context::from_waker(
        futures_util::task::noop_waker_ref(),
    ))
}

const ONE: &str = "{\"n\":1,\"text\":\"é\"}";
fn seconds(n: u64) -> Duration {
    Duration::from_secs(n)
}

#[test]
fn c11_8_record_readers_require_an_explicit_bound() {
    // Inference has one possible marker for a type without Default, and is
    // ambiguous if Default is added. Neither reader may silently pick a bound.
    trait AmbiguousIfDefault<Marker> {
        fn check() {}
    }
    impl<T> AmbiguousIfDefault<()> for T {}
    impl<T: Default> AmbiguousIfDefault<u8> for T {}
    let _ = <JsonLines as AmbiguousIfDefault<_>>::check;
    let _ = <Sse as AmbiguousIfDefault<_>>::check;
    let _: fn(usize) -> JsonLines = JsonLines::new;
    let _: fn(usize) -> Sse = Sse::new;
}

#[tokio::test(start_paused = true)]
async fn c11_9_json_lines_every_split_and_eof() {
    let wire = format!("{ONE}\r\n{{\"n\":2,\"text\":\"tail\"}}");
    for split in 0..=wire.len() {
        let script = Script::new();
        script
            .answer(200)
            .body(wire.as_bytes()[..split].to_vec())
            .body(wire.as_bytes()[split..].to_vec());
        let mut records = RecordsApi::new()
            .with_transport(script.clone())
            .json()
            .await
            .unwrap();
        assert_eq!(
            records.try_next().await.unwrap().unwrap(),
            Row {
                n: 1,
                text: "é".into()
            }
        );
        assert_eq!(records.try_next().await.unwrap().unwrap().n, 2);
        assert!(records.try_next().await.unwrap().is_none());
        assert_one_send(&script);
    }
}

#[tokio::test(start_paused = true)]
async fn c11_8_records_memory_is_bounded() {
    let script = Script::new();
    script.answer(200).body(format!(
        "{}{}",
        format!("{ONE}\n").repeat(100),
        "x".repeat(65)
    ));
    let mut records = RecordsApi::new()
        .with_transport(script.clone())
        .json()
        .await
        .unwrap();
    for _ in 0..100 {
        assert!(records.try_next().await.unwrap().is_some());
    }
    assert!(matches!(
        records.try_next().await,
        Err(RecordError::TooLarge { limit: 64, .. })
    ));
    assert!(records.next().await.is_none());
    let script = Script::new();
    script
        .answer(200)
        .body("x".repeat(32))
        .body("x".repeat(33))
        .stall();
    let mut records = RecordsApi::new()
        .with_transport(script.clone())
        .json()
        .await
        .unwrap();
    assert!(matches!(
        records.try_next().await,
        Err(RecordError::TooLarge { .. })
    ));
}

#[tokio::test(start_paused = true)]
async fn c11_8_records_first_error_ends_stream() {
    for bad in [
        "not-json\n",
        "\n",
        "{\"n\":\"secret-value\",\"text\":\"\"}\n",
    ] {
        let script = Script::new();
        script.answer(200).body(format!("{ONE}\n{bad}{ONE}\n"));
        let mut records = RecordsApi::new()
            .with_transport(script.clone())
            .json()
            .await
            .unwrap();
        assert!(records.try_next().await.unwrap().is_some());
        let error = records.try_next().await.unwrap_err();
        assert!(matches!(error, RecordError::Decode { .. }));
        assert!(!format!("{error:?} {error}").contains("secret-value"));
        assert!(records.next().await.is_none());
    }
}

#[tokio::test(start_paused = true)]
async fn c11_10_records_keep_transport_failure_and_head() {
    for mode in 0..3 {
        let script = Script::new();
        let answer = script
            .answer(200)
            .header("x-request-id", "r1")
            .body(format!("{ONE}\n"));
        if mode == 0 {
            answer.cut();
        } else {
            answer.stall();
        }
        let client = RecordsApi::new().with_transport(script.clone());
        let call = client.json().with_idle_timeout(seconds(2));
        let call = if mode == 2 {
            call.with_timeout(seconds(1))
        } else {
            call
        };
        let start = Instant::now();
        let mut records = call.await.unwrap();
        assert!(records.try_next().await.unwrap().is_some());
        let wrapped = records.try_next().await.unwrap_err();
        let RecordError::Stream(e) = &wrapped else {
            panic!("expected I/O error")
        };
        assert!(std::ptr::eq(
            wrapped
                .source()
                .unwrap()
                .downcast_ref::<concord::read::StreamError>()
                .unwrap(),
            e
        ));
        assert_eq!(e.kind(), Kind::Transport);
        assert_eq!(
            e.class(),
            Some(if mode == 0 {
                Class::Body
            } else {
                Class::Timeout
            })
        );
        assert_eq!(e.endpoint(), "records_api.json");
        assert_eq!(e.tries(), 1);
        assert!(e.is_status(200));
        assert_eq!(e.headers().unwrap().get("x-request-id"), Some("r1"));
        assert_eq!(records.status(), concord::http::StatusCode::OK);
        assert_eq!(records.headers().get("x-request-id"), Some("r1"));
        assert_eq!(
            start.elapsed(),
            seconds(match mode {
                0 => 0,
                1 => 2,
                _ => 1,
            })
        );
        assert!(records.next().await.is_none());
        assert_one_send(&script);
    }
}

#[tokio::test(start_paused = true)]
async fn c11_10_records_timeouts_follow_byte_stream_fragments() {
    let script = Script::new();
    script
        .answer(200)
        .body("{\"n\":")
        .pause(seconds(1))
        .body("1,")
        .pause(seconds(1))
        .body("\"text\":")
        .pause(seconds(1))
        .body("\"ok\"}\n");
    let start = Instant::now();
    let mut records = RecordsApi::new()
        .with_transport(script.clone())
        .json()
        .with_idle_timeout(seconds(2))
        .await
        .unwrap();
    assert_eq!(records.try_next().await.unwrap().unwrap().text, "ok");
    assert_eq!(start.elapsed(), seconds(3));
    tokio::time::sleep(seconds(100)).await; // caller work has no idle timer
    assert!(records.try_next().await.unwrap().is_none());
    assert_one_send(&script);
}

#[tokio::test(start_paused = true)]
async fn c11_10_records_timeouts_follow_byte_stream_comments() {
    let script = Script::new();
    script
        .answer(200)
        .body(": heartbeat\n")
        .pause(seconds(1))
        .body(": heartbeat\n")
        .pause(seconds(1))
        .body(": heartbeat\n")
        .pause(seconds(1))
        .body("data: ready\n\n");
    let start = Instant::now();
    let mut events = RecordsApi::new()
        .with_transport(script.clone())
        .events()
        .with_idle_timeout(seconds(2))
        .await
        .unwrap();
    assert_eq!(events.try_next().await.unwrap().unwrap().data, "ready");
    assert_eq!(start.elapsed(), seconds(3));
    assert_one_send(&script);
}

#[tokio::test(start_paused = true)]
async fn c11_10_records_timeouts_follow_byte_stream_buffered_work() {
    let script = Script::new();
    script.answer(200).body(format!("{ONE}\n{ONE}\n")).stall();
    let mut records = RecordsApi::new()
        .with_transport(script.clone())
        .json()
        .with_timeout(seconds(1))
        .await
        .unwrap();
    assert!(records.try_next().await.unwrap().is_some());
    tokio::time::sleep(seconds(3)).await;
    assert!(records.try_next().await.unwrap().is_some());
    let RecordError::Stream(error) = records.try_next().await.unwrap_err() else {
        panic!("expected timeout")
    };
    assert!(error.is_timeout());
    assert_one_send(&script);
}

#[tokio::test(start_paused = true)]
async fn c11_10_records_never_resend_after_handoff() {
    let script = Script::new();
    script.fail(concord::transport::TransportError::connect());
    let error = match RecordsApi::new()
        .with_transport(script.clone())
        .json()
        .await
    {
        Err(e) => e,
        Ok(_) => panic!("expected call error"),
    };
    assert_eq!(error.class(), Some(Class::Connect));
    assert_eq!(error.tries(), 1);
    assert_one_send(&script);
    let script = Script::new();
    script.answer(200).body(format!("{ONE}\n")).cut();
    script.answer(200).body(format!("{ONE}\n"));
    let mut records = RecordsApi::new()
        .with_transport(script.clone())
        .with_retry(concord::Retry::standard())
        .json()
        .await
        .unwrap();
    assert!(records.try_next().await.unwrap().is_some());
    assert!(records.try_next().await.is_err());
    assert_eq!(script.unused(), 1);
    assert_one_send(&script);
}

#[tokio::test(start_paused = true)]
async fn c11_9_sse_every_split_and_control_fields() {
    let wire = "\u{feff}: hello\r\nid: 7\r\nretry: 1500\r\nevent: delta\r\ndata: hé\r\ndata: there\r\n\r\n\
                : comment\nid: bad\0id\nretry: 1x\nunknown: ignored\ndata:\n\n\
                id:\nretry: 2000\n\ndata: last\n\n";
    for split in 0..=wire.len() {
        let script = Script::new();
        script
            .answer(200)
            .body(wire.as_bytes()[..split].to_vec())
            .body(wire.as_bytes()[split..].to_vec());
        let mut records = RecordsApi::new()
            .with_transport(script.clone())
            .events()
            .await
            .unwrap();
        let first = records.try_next().await.unwrap().unwrap();
        assert_eq!(
            (
                first.event.as_str(),
                first.data.as_str(),
                first.id.as_str(),
                first.retry
            ),
            ("delta", "hé\nthere", "7", Some(Duration::from_millis(1500)))
        );
        let second = records.try_next().await.unwrap().unwrap();
        assert_eq!(
            (
                second.event.as_str(),
                second.data.as_str(),
                second.id.as_str(),
                second.retry
            ),
            ("message", "", "7", Some(Duration::from_millis(1500)))
        );
        let third = records.try_next().await.unwrap().unwrap();
        assert_eq!(
            (third.data.as_str(), third.id.as_str(), third.retry),
            ("last", "", Some(Duration::from_millis(2000)))
        );
        assert!(records.try_next().await.unwrap().is_none());
    }
}

#[tokio::test(start_paused = true)]
async fn c11_9_sse_cr_only_eof_and_invalid_utf8() {
    let script = Script::new();
    script.answer(200).body("data: one\r\rdata: discarded\n");
    let mut events = RecordsApi::new()
        .with_transport(script.clone())
        .events()
        .await
        .unwrap();
    assert_eq!(events.try_next().await.unwrap().unwrap().data, "one");
    assert!(events.try_next().await.unwrap().is_none());
    let script = Script::new();
    script.answer(200).body(b"data: \xff\n\n".to_vec());
    let mut events = RecordsApi::new()
        .with_transport(script.clone())
        .events()
        .await
        .unwrap();
    assert!(matches!(
        events.try_next().await,
        Err(RecordError::Utf8 { .. })
    ));
}

#[tokio::test(start_paused = true)]
async fn c11_8_sse_whole_event_bound_counts_ignored_fields() {
    for wire in [
        "data: a\n".repeat(20),
        ": ignored\n".repeat(20),
        "id: a\n".repeat(30),
    ] {
        let script = Script::new();
        script.answer(200).body(wire).stall();
        let mut events = RecordsApi::new()
            .with_transport(script.clone())
            .events()
            .await
            .unwrap();
        assert!(matches!(
            events.try_next().await,
            Err(RecordError::TooLarge { limit: 128, .. })
        ));
    }
}

#[tokio::test(start_paused = true)]
async fn c11_8_records_keep_safe_causes_and_optional_positions() {
    let bad = b"data: \xff\n\n";
    let mut events = Sse::new(128)
        .read(Incoming::new(concord::reply(200, bad.to_vec())))
        .await
        .unwrap();
    let error = events.try_next().await.unwrap_err();
    let RecordError::Utf8 { record, source, .. } = &error else {
        panic!("expected UTF-8 failure")
    };
    assert_eq!(*record, 1);
    assert_eq!(source.valid_up_to(), 6);
    assert_eq!(source.error_len(), Some(1));
    assert!(std::ptr::eq(
        error
            .source()
            .unwrap()
            .downcast_ref::<std::str::Utf8Error>()
            .unwrap(),
        source
    ));
    assert!(events.next().await.is_none());

    let invalid = "{\"n\":\"private-decoder-value\",\"text\":\"private-text\"}";
    let mut records: RecordStream<Row> = JsonLines::new(128)
        .read(Incoming::new(concord::reply(
            200,
            format!("{ONE}\n{invalid}\n"),
        )))
        .await
        .unwrap();
    records.try_next().await.unwrap().unwrap();
    let error = records.try_next().await.unwrap_err();
    let decoder = serde_json::from_str::<Row>(invalid).unwrap_err();
    assert!(matches!(error, RecordError::Decode {
        record: Some(2), line, column, ..
    } if (line, column) == (decoder.line(), decoder.column())));
    assert!(error.source().is_none());
    let event = SseEvent::new(invalid);
    let error = event.json::<Row>().unwrap_err();
    assert!(matches!(error, RecordError::Decode {
        record: None, line, column, ..
    } if (line, column) == (decoder.line(), decoder.column())));
    assert!(error.source().is_none());
    assert!(matches!(
        SseEvent::new("").json::<Row>(),
        Err(RecordError::Decode {
            record: None,
            line: 1,
            column: 0,
            ..
        })
    ));
    assert!(records.try_next().await.unwrap().is_none());
}

#[tokio::test(start_paused = true)]
async fn c11_8_records_never_print_values() {
    let secret = "private-record-value";
    let mut event = SseEvent::new(secret);
    event.event = secret.into();
    event.id = secret.into();
    assert_eq!(format!("{event:?}"), "SseEvent { .. }");
    assert_eq!(format!("{:?}", JsonLines::new(123)), "JsonLines");
    assert_eq!(format!("{:?}", Sse::new(456)), "Sse");
    for (wire, limit) in [
        (
            format!("{{\"n\":\"{secret}\",\"text\":\"{secret}\"}}\n").into_bytes(),
            128,
        ),
        (format!("{secret}\n").into_bytes(), 1),
        (b"\xff\n".to_vec(), 128),
    ] {
        let script = Script::new();
        script.answer(206).header("x-private", secret).body(wire);
        let response = concord::Transport::send(
            &script,
            concord::reqwest::Request::new(
                concord::http::Method::GET,
                "https://records.test/".parse().unwrap(),
            ),
            concord::transport::TryInfo::new("records.safe", 1),
        )
        .await
        .unwrap();
        let mut records: RecordStream<Row> = JsonLines::new(limit)
            .read(Incoming::new(response))
            .await
            .unwrap();
        assert!(!format!("{records:?}").contains(secret));
        let error = records.try_next().await.unwrap_err();
        let mut cause: Option<&dyn std::error::Error> = Some(&error);
        while let Some(error) = cause {
            assert!(!format!("{error}: {error:?}").contains(secret));
            cause = error.source();
        }
        assert_eq!(records.status(), concord::http::StatusCode::PARTIAL_CONTENT);
        assert_eq!(records.headers().get("x-private"), Some(secret));
        assert!(!format!("{records:?}").contains(secret));
        assert!(records.next().await.is_none());
        assert_one_send(&script);
    }
}

#[tokio::test(start_paused = true)]
async fn c11_8_zero_and_exact_bounds_include_cr_and_normalized_delimiters() {
    for (wire, limit, succeeds) in [("1\n", 1, true), ("1\r\n", 1, false), ("1\r\n", 2, true)] {
        let mut records: RecordStream<u32> = JsonLines::new(limit)
            .read(Incoming::new(concord::reply(200, wire)))
            .await
            .unwrap();
        if succeeds {
            assert_eq!(records.try_next().await.unwrap(), Some(1));
        } else {
            assert!(matches!(
                records.try_next().await,
                Err(RecordError::TooLarge { limit: 1, .. })
            ));
        }
        assert!(records.try_next().await.unwrap().is_none());
    }
    for wire in ["", "\n", "1"] {
        let mut records: RecordStream<u32> = JsonLines::new(0)
            .read(Incoming::new(concord::reply(200, wire)))
            .await
            .unwrap();
        match wire {
            "" => assert!(records.try_next().await.unwrap().is_none()),
            "\n" => assert!(matches!(
                records.try_next().await,
                Err(RecordError::Decode {
                    record: Some(1),
                    ..
                })
            )),
            _ => assert!(matches!(
                records.try_next().await,
                Err(RecordError::TooLarge { limit: 0, .. })
            )),
        }
        assert!(records.next().await.is_none());
    }
    for delimiter in ["\n", "\r", "\r\n"] {
        for limit in [5, 6] {
            let mut events = Sse::new(limit)
                .read(Incoming::new(concord::reply(
                    200,
                    format!("data:{delimiter}{delimiter}"),
                )))
                .await
                .unwrap();
            if limit == 6 {
                assert_eq!(events.try_next().await.unwrap().unwrap().data, "");
            } else {
                assert!(matches!(
                    events.try_next().await,
                    Err(RecordError::TooLarge { limit: 5, .. })
                ));
            }
            assert!(events.next().await.is_none());
        }
    }
    for wire in ["", "\n\r\n", ":", "data:\n\n"] {
        let mut events = Sse::new(0)
            .read(Incoming::new(concord::reply(200, wire)))
            .await
            .unwrap();
        if wire.is_empty() || wire.starts_with('\n') {
            assert!(events.try_next().await.unwrap().is_none());
        } else {
            assert!(matches!(
                events.try_next().await,
                Err(RecordError::TooLarge { limit: 0, .. })
            ));
        }
        assert!(events.next().await.is_none());
    }
}

#[tokio::test(start_paused = true)]
async fn c11_8_sse_bounds_fragmented_unfinished_lines_and_unknown_fields() {
    for chunks in [
        vec!["x".repeat(1_000_000)],
        vec!["x".repeat(64), "x".repeat(65)],
        vec!["unknown: a\n".repeat(13)],
        vec![": ignored\n".repeat(13)],
        vec!["data: a\n".repeat(17)],
    ] {
        let script = Script::new();
        let mut answer = script.answer(200);
        for chunk in chunks {
            answer = answer.body(chunk);
        }
        answer.stall();
        let mut records = RecordsApi::new()
            .with_transport(script.clone())
            .events()
            .await
            .unwrap();
        assert!(matches!(
            records.try_next().await,
            Err(RecordError::TooLarge { limit: 128, .. })
        ));
        assert!(records.next().await.is_none());
        assert_one_send(&script);
    }
}

#[tokio::test(start_paused = true)]
async fn c11_9_sse_retry_is_duration_and_provider_data_is_uninterpreted() {
    let mut events = Sse::new(512)
        .read(Incoming::new(concord::reply(
            200,
            "retry: 0\nid: first\n\ndata: [DONE]\n\n\
         retry: -1\nretry: +1\nretry: 1.5\nretry:  3\nretry:\n\
         retry: 18446744073709551616\nid: bad\0id\n\
         event: error\ndata: private-api-error\n\n\
         retry: 18446744073709551615\ndata: last\n\n",
        )))
        .await
        .unwrap();
    let first = events.try_next().await.unwrap().unwrap();
    assert_eq!(
        (first.data.as_str(), first.id.as_str(), first.retry),
        ("[DONE]", "first", Some(Duration::ZERO))
    );
    let second = events.try_next().await.unwrap().unwrap();
    assert_eq!(
        (second.event.as_str(), second.data.as_str(), second.retry),
        ("error", "private-api-error", Some(Duration::ZERO))
    );
    assert!(second.json::<Row>().is_err());
    let third = events.try_next().await.unwrap().unwrap();
    assert_eq!(third.retry, Some(Duration::from_millis(u64::MAX)));
    assert!(events.try_next().await.unwrap().is_none());
}

#[tokio::test(start_paused = true)]
async fn c11_9_single_byte_chunks_and_unterminated_sse_blocks() {
    let wire = "\u{feff}data: hé\r\ndata: there\r\n\r\n";
    let script = Script::new();
    let mut answer = script.answer(200);
    for byte in wire.bytes() {
        answer = answer.body(vec![byte]);
    }
    drop(answer);
    let mut events = RecordsApi::new()
        .with_transport(script.clone())
        .events()
        .await
        .unwrap();
    assert_eq!(events.try_next().await.unwrap().unwrap().data, "hé\nthere");
    assert!(events.next().await.is_none());
    assert_one_send(&script);
    for wire in [
        "data: discarded",
        "data: discarded\n",
        "data: discarded\r\n",
        "data:",
        "\u{feff}",
    ] {
        let mut events = Sse::new(128)
            .read(Incoming::new(concord::reply(200, wire)))
            .await
            .unwrap();
        assert!(events.try_next().await.unwrap().is_none());
    }
}

#[tokio::test(start_paused = true)]
async fn c11_10_records_canceled_pull_keeps_partial_record_every_split() {
    let json = format!("{ONE}\r\n");
    let sse =
        "\u{feff}: beat\r\nid: 7\r\nretry: 1500\r\nevent: delta\r\ndata: hé\r\ndata: there\r\n\r\n";
    for wire in [json.as_str(), sse] {
        // A split before the final CR can already dispatch an SSE event. Use
        // LF here so every tested first chunk has an incomplete record.
        let wire = if wire.starts_with('{') {
            wire.to_owned()
        } else {
            wire.replace("\r\n", "\n")
        };
        for split in 0..wire.len() {
            let script = Script::new();
            script
                .answer(200)
                .body(wire.as_bytes()[..split].to_vec())
                .pause(seconds(1))
                .body(wire.as_bytes()[split..].to_vec());
            if wire.starts_with('{') {
                let mut records = RecordsApi::new()
                    .with_transport(script.clone())
                    .json()
                    .await
                    .unwrap();
                let mut pull = Box::pin(records.try_next());
                assert!(poll_once(pull.as_mut()).is_pending());
                drop(pull);
                tokio::time::advance(Duration::from_millis(500)).await;
                assert!(poll_stream(&mut records).is_pending());
                tokio::time::advance(Duration::from_millis(500)).await;
                assert_eq!(
                    records.next().await.unwrap().unwrap(),
                    Row {
                        n: 1,
                        text: "é".into()
                    }
                );
                assert!(records.try_next().await.unwrap().is_none());
            } else {
                let mut records = RecordsApi::new()
                    .with_transport(script.clone())
                    .events()
                    .await
                    .unwrap();
                let mut pull = Box::pin(records.next());
                assert!(poll_once(pull.as_mut()).is_pending());
                drop(pull);
                tokio::time::advance(Duration::from_millis(500)).await;
                let mut pull = Box::pin(records.try_next());
                assert!(poll_once(pull.as_mut()).is_pending());
                drop(pull);
                tokio::time::advance(Duration::from_millis(500)).await;
                let event = records.try_next().await.unwrap().unwrap();
                assert_eq!(
                    (
                        event.event.as_str(),
                        event.data.as_str(),
                        event.id.as_str(),
                        event.retry
                    ),
                    ("delta", "hé\nthere", "7", Some(Duration::from_millis(1500)))
                );
                assert!(records.next().await.is_none());
            }
            assert_one_send(&script);
        }
    }
}

async fn canceled_idle<T: std::fmt::Debug>(mut records: RecordStream<T>) {
    let mut pull = Box::pin(records.try_next());
    assert!(poll_once(pull.as_mut()).is_pending());
    tokio::time::advance(seconds(1)).await;
    drop(pull);
    tokio::time::advance(seconds(100)).await;
    let start = Instant::now();
    let mut pull = Box::pin(records.try_next());
    assert!(poll_once(pull.as_mut()).is_pending());
    tokio::time::advance(seconds(1)).await;
    assert!(poll_once(pull.as_mut()).is_pending());
    assert!(matches!(pull.await, Err(RecordError::Stream(error)) if error.is_timeout()));
    assert_eq!(start.elapsed(), seconds(2));
    assert!(records.next().await.is_none());
}

#[tokio::test(start_paused = true)]
async fn c11_10_records_canceled_try_next_stops_idle_timer_like_byte_stream() {
    let script = Script::new();
    script.answer(200).body("{\"n\":").stall();
    let records = RecordsApi::new()
        .with_transport(script.clone())
        .json()
        .with_idle_timeout(seconds(2))
        .await
        .unwrap();
    canceled_idle(records).await;
    assert_one_send(&script);
    let script = Script::new();
    script.answer(200).body(": beat\ndata: par").stall();
    let records = RecordsApi::new()
        .with_transport(script.clone())
        .events()
        .with_idle_timeout(seconds(2))
        .await
        .unwrap();
    canceled_idle(records).await;
    assert_one_send(&script);

    let script = Script::new();
    script.answer(200).body("partial").stall();
    let mut bytes = RecordsApi::new()
        .with_transport(script.clone())
        .bytes()
        .with_idle_timeout(seconds(2))
        .await
        .unwrap();
    assert_eq!(bytes.try_next().await.unwrap().unwrap(), "partial");
    let mut pull = Box::pin(bytes.try_next());
    assert!(poll_once(pull.as_mut()).is_pending());
    tokio::time::advance(seconds(1)).await;
    drop(pull);
    tokio::time::advance(seconds(100)).await;
    let start = Instant::now();
    let mut pull = Box::pin(bytes.try_next());
    assert!(poll_once(pull.as_mut()).is_pending());
    assert!(pull.await.unwrap_err().is_timeout());
    assert_eq!(start.elapsed(), seconds(2));
    assert_one_send(&script);
}

#[tokio::test(start_paused = true)]
async fn c11_10_records_stream_poll_timer_and_total_deadline_match_bytes() {
    for total in [false, true] {
        let script = Script::new();
        script.answer(200).body("data: par").stall();
        let call = RecordsApi::new()
            .with_transport(script.clone())
            .events()
            .with_idle_timeout(seconds(2));
        let mut records = if total {
            call.with_timeout(seconds(1)).await.unwrap()
        } else {
            call.await.unwrap()
        };
        if total {
            let mut pull = Box::pin(records.try_next());
            assert!(poll_once(pull.as_mut()).is_pending());
        } else {
            let mut pull = Box::pin(records.next());
            assert!(poll_once(pull.as_mut()).is_pending());
        }
        tokio::time::advance(seconds(100)).await;
        let start = Instant::now();
        let RecordError::Stream(error) = records.try_next().await.unwrap_err() else {
            panic!("expected timeout")
        };
        assert!(error.is_timeout());
        assert_eq!(start.elapsed(), Duration::ZERO);
        assert!(records.next().await.is_none());
        assert_one_send(&script);

        let script = Script::new();
        script.answer(200).stall();
        let call = RecordsApi::new()
            .with_transport(script.clone())
            .bytes()
            .with_idle_timeout(seconds(2));
        let mut bytes = if total {
            call.with_timeout(seconds(1)).await.unwrap()
        } else {
            call.await.unwrap()
        };
        if total {
            let mut pull = Box::pin(bytes.try_next());
            assert!(poll_once(pull.as_mut()).is_pending());
        } else {
            let mut pull = Box::pin(bytes.next());
            assert!(poll_once(pull.as_mut()).is_pending());
        }
        tokio::time::advance(seconds(100)).await;
        let start = Instant::now();
        assert!(bytes.try_next().await.unwrap_err().is_timeout());
        assert_eq!(start.elapsed(), Duration::ZERO);
        assert_one_send(&script);
    }
}

#[derive(Clone)]
struct BodyLease(std::sync::Arc<tokio::sync::Semaphore>);

struct LeasedStream {
    body:
        Pin<Box<dyn Stream<Item = Result<concord::bytes::Bytes, concord::reqwest::Error>> + Send>>,
    _permit: tokio::sync::OwnedSemaphorePermit,
}

impl Stream for LeasedStream {
    type Item = Result<concord::bytes::Bytes, concord::reqwest::Error>;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.get_mut().body.as_mut().poll_next(cx)
    }
}

impl concord::Middleware for BodyLease {
    async fn handle(
        &self,
        request: concord::reqwest::Request,
        info: concord::transport::TryInfo,
        next: concord::Next<'_>,
    ) -> Result<concord::reqwest::Response, concord::transport::TransportError> {
        let permit = self
            .0
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| concord::transport::TransportError::blocked())?;
        let response = next.send(request, info).await?;
        let status = response.status();
        let headers = response.headers().clone();
        let body = LeasedStream {
            body: Box::pin(response.bytes_stream()),
            _permit: permit,
        };
        let mut response = concord::http::Response::new(concord::reqwest::Body::wrap_stream(body));
        *response.status_mut() = status;
        *response.headers_mut() = headers;
        Ok(response.into())
    }
}

#[tokio::test(start_paused = true)]
async fn c11_10_records_drop_leaves_no_body_or_permit_state() {
    for mode in 0..7 {
        let script = Script::new();
        match mode {
            0..=2 => {
                script.answer(200).body("data: partial").stall();
            }
            3 => {
                script.answer(200).body("data: good\n\n");
            }
            4 => {
                script.answer(200).body("x".repeat(129)).stall();
            }
            5 => {
                script.answer(200).body(b"\xff\n".to_vec()).stall();
            }
            _ => {
                script.answer(200).cut();
            }
        }
        let permits = std::sync::Arc::new(tokio::sync::Semaphore::new(1));
        let api = RecordsApi::new()
            .with_transport(script.clone())
            .with_middleware(BodyLease(permits.clone()));
        let mut records = api.events().await.unwrap();
        assert_eq!(permits.available_permits(), 0);
        match mode {
            0 => {
                drop(records);
            }
            1 => {
                let mut pull = Box::pin(records.try_next());
                assert!(poll_once(pull.as_mut()).is_pending());
                drop(pull);
                assert_eq!(permits.available_permits(), 0);
                drop(records);
            }
            2 => {
                let mut pull = Box::pin(records.next());
                assert!(poll_once(pull.as_mut()).is_pending());
                drop(pull);
                assert_eq!(permits.available_permits(), 0);
                drop(records);
            }
            3 => {
                assert_eq!(records.try_next().await.unwrap().unwrap().data, "good");
                assert!(records.try_next().await.unwrap().is_none());
                assert_eq!(permits.available_permits(), 1);
                assert!(records.next().await.is_none());
            }
            _ => {
                assert!(records.try_next().await.is_err());
                assert_eq!(permits.available_permits(), 1);
                assert!(records.next().await.is_none());
            }
        }
        assert_eq!(permits.available_permits(), 1);
        assert_one_send(&script);
    }
    // The JSON parser also releases a still-open body on decoder failure.
    let script = Script::new();
    script.answer(200).body("invalid\n").stall();
    let permits = std::sync::Arc::new(tokio::sync::Semaphore::new(1));
    let mut records = RecordsApi::new()
        .with_transport(script.clone())
        .with_middleware(BodyLease(permits.clone()))
        .json()
        .await
        .unwrap();
    assert_eq!(permits.available_permits(), 0);
    assert!(matches!(
        records.try_next().await,
        Err(RecordError::Decode { .. })
    ));
    assert_eq!(permits.available_permits(), 1);
    assert_one_send(&script);
}

#[tokio::test(start_paused = true)]
async fn c11_10_canceled_sse_pull_keeps_split_crlf_and_control_state() {
    let script = Script::new();
    script
        .answer(200)
        .body("\u{feff}id: 7\r\nretry: 15\r\ndata: hé\r")
        .pause(seconds(1))
        .body("\n")
        .pause(seconds(1))
        .body("\r")
        .pause(seconds(1))
        .body("\ndata: second\r\r");
    let mut events = RecordsApi::new()
        .with_transport(script.clone())
        .events()
        .await
        .unwrap();
    let mut pull = Box::pin(events.try_next());
    assert!(poll_once(pull.as_mut()).is_pending());
    drop(pull);
    tokio::time::advance(seconds(1)).await;
    assert!(poll_stream(&mut events).is_pending());
    tokio::time::advance(seconds(1)).await;
    let first = events.try_next().await.unwrap().unwrap();
    assert_eq!(
        (first.data.as_str(), first.id.as_str(), first.retry),
        ("hé", "7", Some(Duration::from_millis(15)))
    );
    let mut pull = Box::pin(events.next());
    assert!(poll_once(pull.as_mut()).is_pending());
    drop(pull);
    tokio::time::advance(seconds(1)).await;
    let second = events.try_next().await.unwrap().unwrap();
    assert_eq!(
        (second.data.as_str(), second.id.as_str(), second.retry),
        ("second", "7", Some(Duration::from_millis(15)))
    );
    assert!(events.next().await.is_none());
    assert_one_send(&script);
}

#[tokio::test(start_paused = true)]
async fn c11_8_stream_errors_are_terminal_with_record_positions() {
    for bad in [b"\xff\n".as_slice(), b"\n", b"no-json\n"] {
        let wire = [
            format!("{ONE}\n").as_bytes(),
            bad,
            format!("{ONE}\n").as_bytes(),
        ]
        .concat();
        let mut records: RecordStream<Row> = JsonLines::new(64)
            .read(Incoming::new(concord::reply(200, wire)))
            .await
            .unwrap();
        assert_eq!(records.try_next().await.unwrap().unwrap().n, 1);
        let error = records.next().await.unwrap().unwrap_err();
        assert!(matches!(
            error,
            RecordError::Utf8 { record: 2, .. }
                | RecordError::Decode {
                    record: Some(2),
                    ..
                }
        ));
        assert!(records.try_next().await.unwrap().is_none());
        assert!(records.next().await.is_none());
    }
}

#[tokio::test(start_paused = true)]
async fn c11_10_sse_never_reconnects_or_prints_body_failure_values() {
    for timeout in [false, true] {
        let script = Script::new();
        let answer = script
            .answer(206)
            .header("x-private", "private-header")
            .body("data: private-record\n\n");
        if timeout {
            answer.stall();
        } else {
            answer.cut();
        }
        script.answer(200).body("data: extra\n\n");
        let mut events = RecordsApi::new()
            .with_transport(script.clone())
            .with_retry(concord::Retry::standard())
            .events()
            .with_idle_timeout(seconds(2))
            .await
            .unwrap();
        assert_eq!(events.next().await.unwrap().unwrap().data, "private-record");
        let start = Instant::now();
        let error = events.next().await.unwrap().unwrap_err();
        let RecordError::Stream(body) = &error else {
            panic!("expected body failure")
        };
        assert_eq!(body.endpoint(), "records_api.events");
        assert_eq!(body.tries(), 1);
        assert_eq!(
            body.class(),
            Some(if timeout { Class::Timeout } else { Class::Body })
        );
        assert_eq!(
            body.headers().unwrap().get("x-private"),
            Some("private-header")
        );
        assert_eq!(
            start.elapsed(),
            if timeout { seconds(2) } else { Duration::ZERO }
        );
        let mut cause: Option<&dyn std::error::Error> = Some(&error);
        while let Some(error) = cause {
            assert!(!format!("{error}: {error:?}").contains("private-"));
            cause = error.source();
        }
        assert!(events.try_next().await.unwrap().is_none());
        assert_eq!(events.status(), concord::http::StatusCode::PARTIAL_CONTENT);
        assert_eq!(events.headers().get("x-private"), Some("private-header"));
        assert_one_send(&script);
        assert_eq!(script.unused(), 1);
    }
}

#[tokio::test(start_paused = true)]
async fn c11_9_sse_initial_bom_precedes_framing_and_does_not_use_the_bound() {
    for (wire, limit, data) in [
        ("\u{feff}", 0, None),
        ("\u{feff}\n\r\n", 0, None),
        ("\u{feff}data:\n\n", 6, Some("")),
        ("\u{feff}data:\r\n\r\n", 6, Some("")),
        ("\u{feff}data:\r\r", 6, Some("")),
        ("\u{feff}:\ndata:\n\n", 8, Some("")),
    ] {
        for split in 0..=wire.len() {
            let script = Script::new();
            script
                .answer(200)
                .body(wire.as_bytes()[..split].to_vec())
                .body(wire.as_bytes()[split..].to_vec());
            let response = concord::Transport::send(
                &script,
                concord::reqwest::Request::new(
                    concord::http::Method::GET,
                    "https://records.test/".parse().unwrap(),
                ),
                concord::transport::TryInfo::new("records.bom", 1),
            )
            .await
            .unwrap();
            let mut events = Sse::new(limit).read(Incoming::new(response)).await.unwrap();
            assert_eq!(
                events
                    .try_next()
                    .await
                    .unwrap()
                    .map(|event| event.data)
                    .as_deref(),
                data,
                "split {split}, bound {limit}"
            );
            assert!(events.try_next().await.unwrap().is_none());
            assert_one_send(&script);
        }
    }
}

#[tokio::test(start_paused = true)]
async fn c11_8_empty_json_lines_decode_at_every_bound_including_zero() {
    for limit in [0, 1, 64] {
        for wire in ["\n", "\r\n"] {
            let mut records: RecordStream<u32> = JsonLines::new(limit)
                .read(Incoming::new(concord::reply(200, wire)))
                .await
                .unwrap();
            let error = records.try_next().await.unwrap_err();
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
                "bound {limit}: {error:?}"
            );
            assert!(error.source().is_none());
            assert!(records.next().await.is_none());
        }
    }
    for wire in ["\r", "\rx\n", " \n", "\r\r\n"] {
        let mut records: RecordStream<u32> = JsonLines::new(0)
            .read(Incoming::new(concord::reply(200, wire)))
            .await
            .unwrap();
        assert!(matches!(
            records.try_next().await,
            Err(RecordError::TooLarge { limit: 0, .. })
        ));
        assert!(records.next().await.is_none());
    }
}

#[tokio::test(start_paused = true)]
async fn c11_9_sse_only_skips_a_complete_bom_at_the_start() {
    for wire in [b"\xef".as_slice(), b"\xef\xbb", b"\xefx\n", b"\xef\xbbx\n"] {
        let mut events = Sse::new(32)
            .read(Incoming::new(concord::reply(200, wire.to_vec())))
            .await
            .unwrap();
        let RecordError::Utf8 { record, source, .. } = events.try_next().await.unwrap_err() else {
            panic!("expected UTF-8 failure")
        };
        assert_eq!(record, 1);
        assert_eq!(source.valid_up_to(), 0);
        assert!(events.next().await.is_none());
    }
    // The second BOM is ordinary text, including its three bytes in the bound.
    for limit in [8, 9] {
        let mut events = Sse::new(limit)
            .read(Incoming::new(concord::reply(
                200,
                "\u{feff}\u{feff}data:\n\n",
            )))
            .await
            .unwrap();
        if limit == 9 {
            assert!(events.try_next().await.unwrap().is_none());
        } else {
            assert!(matches!(
                events.try_next().await,
                Err(RecordError::TooLarge { limit: 8, .. })
            ));
        }
    }
    let mut events = Sse::new(32)
        .read(Incoming::new(concord::reply(200, "data: \u{feff}kept\n\n")))
        .await
        .unwrap();
    assert_eq!(
        events.try_next().await.unwrap().unwrap().data,
        "\u{feff}kept"
    );
    let mut records: RecordStream<u32> = JsonLines::new(32)
        .read(Incoming::new(concord::reply(200, "\u{feff}1\n")))
        .await
        .unwrap();
    assert!(matches!(
        records.try_next().await,
        Err(RecordError::Decode {
            record: Some(1),
            ..
        })
    ));
}

#[tokio::test(start_paused = true)]
async fn c11_10_canceled_pulls_keep_bom_prefix_and_empty_crlf_state() {
    for use_stream in [false, true] {
        for split in [1, 2] {
            let wire = "\u{feff}data:\n\n";
            let script = Script::new();
            script
                .answer(200)
                .body(wire.as_bytes()[..split].to_vec())
                .pause(seconds(1))
                .body(wire.as_bytes()[split..].to_vec());
            let response = concord::Transport::send(
                &script,
                concord::reqwest::Request::new(
                    concord::http::Method::GET,
                    "https://records.test/".parse().unwrap(),
                ),
                concord::transport::TryInfo::new("records.bom", 1),
            )
            .await
            .unwrap();
            let mut events = Sse::new(6).read(Incoming::new(response)).await.unwrap();
            if use_stream {
                let mut pull = Box::pin(events.next());
                assert!(poll_once(pull.as_mut()).is_pending());
            } else {
                let mut pull = Box::pin(events.try_next());
                assert!(poll_once(pull.as_mut()).is_pending());
            }
            tokio::time::advance(seconds(1)).await;
            assert_eq!(events.try_next().await.unwrap().unwrap().data, "");
            assert!(events.next().await.is_none());
            assert_one_send(&script);
        }
        let script = Script::new();
        script.answer(200).body("\r").pause(seconds(1)).body("\n");
        let response = concord::Transport::send(
            &script,
            concord::reqwest::Request::new(
                concord::http::Method::GET,
                "https://records.test/".parse().unwrap(),
            ),
            concord::transport::TryInfo::new("records.empty", 1),
        )
        .await
        .unwrap();
        let mut records: RecordStream<u32> = JsonLines::new(0)
            .read(Incoming::new(response))
            .await
            .unwrap();
        if use_stream {
            let mut pull = Box::pin(records.next());
            assert!(poll_once(pull.as_mut()).is_pending());
        } else {
            let mut pull = Box::pin(records.try_next());
            assert!(poll_once(pull.as_mut()).is_pending());
        }
        tokio::time::advance(seconds(1)).await;
        assert!(matches!(
            records.next().await.unwrap(),
            Err(RecordError::Decode {
                record: Some(1),
                line: 1,
                column: 0,
                ..
            })
        ));
        assert!(records.try_next().await.unwrap().is_none());
        assert_one_send(&script);
    }
}

#[derive(Default)]
struct CountWake(std::sync::atomic::AtomicUsize);

impl std::task::Wake for CountWake {
    fn wake(self: std::sync::Arc<Self>) {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }

    fn wake_by_ref(self: &std::sync::Arc<Self>) {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}

// A finite ready prefix followed by Pending exposes the infinite-stream fault
// without hanging the test process. The prefix is much larger than one poll's
// chunk budget, and each individual body poll completes promptly.
fn ready_prefix(
    chunk: concord::bytes::Bytes,
    polls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
) -> Incoming {
    let stream = futures_util::stream::poll_fn(move |_| {
        let number = polls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if number < 1024 {
            Poll::Ready(Some(Ok::<_, std::convert::Infallible>(chunk.clone())))
        } else {
            Poll::Pending
        }
    });
    let body = concord::reqwest::Body::wrap_stream(stream);
    Incoming::new(concord::http::Response::new(body).into())
}

#[tokio::test(start_paused = true)]
async fn c11_10_ready_chunks_yield() {
    for chunk in [b":\n\n".as_slice(), b"x\n\n"] {
        let polls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut events = Sse::new(2)
            .read(ready_prefix(
                concord::bytes::Bytes::copy_from_slice(chunk),
                polls.clone(),
            ))
            .await
            .unwrap();
        let wakes = std::sync::Arc::new(CountWake::default());
        let waker = std::task::Waker::from(wakes.clone());
        let mut pull = Box::pin(events.try_next());
        assert!(pull
            .as_mut()
            .poll(&mut Context::from_waker(&waker))
            .is_pending());
        assert!(
            polls.load(std::sync::atomic::Ordering::SeqCst) <= 64,
            "one poll consumed {} body chunks",
            polls.load(std::sync::atomic::Ordering::SeqCst)
        );
        assert_eq!(wakes.0.load(std::sync::atomic::Ordering::SeqCst), 1);
    }
    let data = "é".repeat(100);
    let chunks = format!("data: {data}\n\n")
        .into_bytes()
        .into_iter()
        .map(|byte| Ok::<_, std::convert::Infallible>(concord::bytes::Bytes::from(vec![byte])));
    let response = concord::http::Response::new(concord::reqwest::Body::wrap_stream(
        futures_util::stream::iter(chunks),
    ));
    let mut events = Sse::new(512)
        .read(Incoming::new(response.into()))
        .await
        .unwrap();
    let mut pull = Box::pin(events.try_next());
    assert!(poll_once(pull.as_mut()).is_pending());
    drop(pull);
    assert_eq!(events.next().await.unwrap().unwrap().data, data);
    assert!(events.try_next().await.unwrap().is_none());
}

#[tokio::test(start_paused = true)]
async fn c11_10_json_chunks_yield() {
    // Empty chunks can spin the JSON Lines framer without using any byte bound.
    let polls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut records: RecordStream<u32> = JsonLines::new(16)
        .read(ready_prefix(concord::bytes::Bytes::new(), polls.clone()))
        .await
        .unwrap();
    let wakes = std::sync::Arc::new(CountWake::default());
    let waker = std::task::Waker::from(wakes.clone());
    let mut pull = Box::pin(records.try_next());
    assert!(pull
        .as_mut()
        .poll(&mut Context::from_waker(&waker))
        .is_pending());
    assert!(
        polls.load(std::sync::atomic::Ordering::SeqCst) <= 64,
        "one poll consumed {} empty chunks",
        polls.load(std::sync::atomic::Ordering::SeqCst)
    );
    assert_eq!(wakes.0.load(std::sync::atomic::Ordering::SeqCst), 1);
    drop(pull);

    let data = "é".repeat(100);
    let chunks = format!("\"{data}\"\n")
        .into_bytes()
        .into_iter()
        .map(|byte| Ok::<_, std::convert::Infallible>(concord::bytes::Bytes::from(vec![byte])));
    let response = concord::http::Response::new(concord::reqwest::Body::wrap_stream(
        futures_util::stream::iter(chunks),
    ));
    let mut records: RecordStream<String> = JsonLines::new(512)
        .read(Incoming::new(response.into()))
        .await
        .unwrap();
    let mut pull = Box::pin(records.try_next());
    assert!(poll_once(pull.as_mut()).is_pending());
    drop(pull);
    assert_eq!(records.next().await.unwrap().unwrap(), data);
    assert!(records.try_next().await.unwrap().is_none());
}

#[tokio::test(start_paused = true)]
async fn c11_10_ready_bytes_yield() {
    let data = "é".repeat(33_000);
    for (wire, expected) in [
        (
            format!(":{}\n\ndata: ready\n\n", "x".repeat(65_536)),
            "ready",
        ),
        (
            format!("{}data: ready\n\n", ":\n\n".repeat(30_000)),
            "ready",
        ),
        (format!("data: {data}\n\n"), data.as_str()),
    ] {
        let mut events = Sse::new(100_000)
            .read(Incoming::new(concord::reply(200, wire)))
            .await
            .unwrap();
        let wakes = std::sync::Arc::new(CountWake::default());
        let waker = std::task::Waker::from(wakes.clone());
        let mut pull = Box::pin(events.try_next());
        let polled = pull.as_mut().poll(&mut Context::from_waker(&waker));
        assert!(
            polled.is_pending(),
            "one poll scanned the entire ready chunk: {polled:?}"
        );
        assert_eq!(wakes.0.load(std::sync::atomic::Ordering::SeqCst), 1);
        drop(pull);
        assert_eq!(events.next().await.unwrap().unwrap().data, expected);
        assert!(events.try_next().await.unwrap().is_none());
    }
}

#[tokio::test(start_paused = true)]
async fn c11_10_json_bytes_yield() {
    let mut records: RecordStream<u32> = JsonLines::new(100_000)
        .read(Incoming::new(concord::reply(
            200,
            format!("{}1\n", " ".repeat(65_536)),
        )))
        .await
        .unwrap();
    let wakes = std::sync::Arc::new(CountWake::default());
    let waker = std::task::Waker::from(wakes.clone());
    let mut pull = Box::pin(records.try_next());
    let polled = pull.as_mut().poll(&mut Context::from_waker(&waker));
    assert!(
        polled.is_pending(),
        "one poll scanned the entire JSON line: {polled:?}"
    );
    assert_eq!(wakes.0.load(std::sync::atomic::Ordering::SeqCst), 1);
    drop(pull);
    assert_eq!(records.next().await.unwrap().unwrap(), 1);
    assert!(records.try_next().await.unwrap().is_none());

    let data = "é".repeat(33_000);
    let mut records: RecordStream<String> = JsonLines::new(100_000)
        .read(Incoming::new(concord::reply(200, format!("\"{data}\"\n"))))
        .await
        .unwrap();
    let mut pull = Box::pin(records.try_next());
    assert!(poll_once(pull.as_mut()).is_pending());
    drop(pull);
    assert_eq!(records.next().await.unwrap().unwrap(), data);
    assert!(records.try_next().await.unwrap().is_none());
}

#[tokio::test(start_paused = true)]
async fn c11_8_partial_event_bound() {
    let script = Script::new();
    script.answer(200).body("data: abc\nx").stall();
    let mut events = RecordsApi::new()
        .with_transport(script.clone())
        .events_ten()
        .await
        .unwrap();
    let start = Instant::now();
    let result = events.try_next().await;
    assert!(
        matches!(result, Err(RecordError::TooLarge { limit: 10, .. })),
        "{result:?} after {:?}",
        start.elapsed()
    );
    assert_eq!(start.elapsed(), Duration::ZERO);
    assert!(events.next().await.is_none());
    assert_one_send(&script);
}

#[tokio::test(start_paused = true)]
async fn c11_10_unpolled_pull_clears_idle() {
    let script = Script::new();
    script.answer(200).body("data: partial").stall();
    let mut events = RecordsApi::new()
        .with_transport(script.clone())
        .events()
        .with_idle_timeout(seconds(2))
        .await
        .unwrap();
    assert!(poll_stream(&mut events).is_pending());
    drop(events.try_next());
    tokio::time::advance(seconds(3)).await;
    let start = Instant::now();
    let mut pull = Box::pin(events.try_next());
    let polled = poll_once(pull.as_mut());
    assert!(
        polled.is_pending(),
        "unpolled drop retained the previous idle timer: {polled:?}"
    );
    let RecordError::Stream(error) = pull.await.unwrap_err() else {
        panic!("expected idle timeout")
    };
    assert!(error.is_timeout());
    assert_eq!(start.elapsed(), seconds(2));
    assert!(events.next().await.is_none());
    assert_one_send(&script);
}

#[tokio::test(start_paused = true)]
async fn c11_8_partial_delimiter_bound() {
    // The first unfinished line also needs its normalized delimiter allowance.
    let script = Script::new();
    script.answer(200).body("x").stall();
    let mut events = RecordsApi::new()
        .with_transport(script.clone())
        .events_one()
        .await
        .unwrap();
    let start = Instant::now();
    let result = events.try_next().await;
    assert!(
        matches!(result, Err(RecordError::TooLarge { limit: 1, .. })),
        "{result:?} after {:?}",
        start.elapsed()
    );
    assert_eq!(start.elapsed(), Duration::ZERO);
    assert!(events.next().await.is_none());
    assert_one_send(&script);
}

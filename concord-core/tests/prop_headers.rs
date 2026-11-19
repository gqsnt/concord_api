//! C4.3, C5.4 and C2.12: the engine's readers of `Retry-After` and of `Content-Length`,
//! against the grammar of their contracts, for every spelling of a number.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use concord_core::{
    __private::{read_body, Core, Draft, Idempotent, Options, Piece},
    bytes::Bytes,
    error::Kind,
    http::{HeaderMap, HeaderValue, Method, Response, StatusCode},
    read::{Incoming, ReadError},
    reqwest,
    retry::{again, Outcome},
    transport::TryInfo,
    Body, Error, Headers, Reader, Retry,
};
use proptest::prelude::*;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[path = "corpus/read.rs"]
mod corpus;

/// A number of seconds or bytes as C4.3 and C5.4 read it: ASCII digits only.
fn digits_model(text: &str) -> Option<Result<u64, ()>> {
    (!text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()))
        .then(|| text.parse().map_err(|_| ()))
}

/// A number as a server might write it: digits of every length from 1 to 40, signs, leading
/// zeros, every kind of whitespace around it, and near misses.
fn number_text() -> impl Strategy<Value = String> {
    let space = prop::sample::select(vec!["", "", " ", "\t", "  ", " \t", "\t "]);
    (
        space.clone(),
        prop_oneof![
            4 => "[0-9]{1,40}",
            1 => ("0{1,30}", 0u64..1000).prop_map(|(zeros, n)| format!("{zeros}{n}")),
            1 => prop::sample::select(vec![
                "", "+1", "-1", "-0", "1.5", "1e3", "0x10", "1 2", "1,2", "١٢", "18446744073709551615",
                "18446744073709551616", "99999999999999999999", "00000000000000000000000000000000000000001",
            ]).prop_map(str::to_owned),
        ],
        space,
    )
        .prop_map(|(before, number, after)| format!("{before}{number}{after}"))
}
/// The same, and signed numbers written alone, which Rust's own parser would take.
fn length_text() -> impl Strategy<Value = String> {
    prop_oneof![
        3 => number_text(),
        1 => (prop::sample::select(vec!["+", "-", "+0", "++"]), "[0-9]{1,20}")
            .prop_map(|(sign, digits)| format!("{sign}{digits}")),
    ]
}

/// Texts near the HTTP-date grammar: the three forms, a leap second, and their near misses.
fn date_text() -> impl Strategy<Value = String> {
    let dates = prop::sample::select(vec![
        "Sun, 06 Nov 1994 08:49:37 GMT",
        "Sunday, 06-Nov-94 08:49:37 GMT",
        "Sun Nov  6 08:49:37 1994",
        "Sun Nov 06 08:49:37 1994",
        "Sat, 31 Dec 2016 23:59:60 GMT",
        "Tue, 01 Jan 2030 00:00:00 GMT",
        "Wednesday, 01-Jan-76 00:00:00 GMT",
        "Fri, 31 Dec 9999 23:59:59 GMT",
        "Mon, 01 Jan 0001 00:00:00 GMT",
        "Mon, 06 Nov 1994 08:49:37 GMT",
        "Sun, 06 Nov 1994 08:49:37 UTC",
        "Sun, 6 Nov 1994 08:49:37 GMT",
        "Thu, 29 Feb 2024 00:00:00 GMT",
        "Thu, 29 Feb 2023 00:00:00 GMT",
    ]);
    (dates, any::<u8>(), any::<usize>(), 0x20u8..0x7f).prop_map(|(date, how, at, byte)| {
        let mut text = date.as_bytes().to_vec();
        let at = at % text.len();
        match how % 6 {
            0 | 1 => {}
            2 => text[at] = byte,
            3 => {
                text.remove(at);
            }
            4 => text.insert(at, byte),
            _ => text = format!(" \t{date} ").into_bytes(),
        }
        String::from_utf8(text).unwrap()
    })
}

fn decide(headers: &Headers, now: SystemTime) -> Option<Duration> {
    let policy = Retry::<()>::standard();
    let outcome = Outcome {
        kind: Kind::Refused,
        status: Some(StatusCode::TOO_MANY_REQUESTS),
        class: None,
        headers: Some(headers),
        problem: None,
    };
    again(
        &policy,
        outcome,
        1,
        &Method::GET,
        Idempotent::ByMethod,
        true,
        None,
        now,
        0.5,
    )
    .header_wait
}

proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn c4_3_retry_after_in_seconds_reads_as_the_grammar_says_in_both_readers(
        text in prop_oneof![3 => length_text(), 2 => date_text()],
        now in prop_oneof![Just(0u64), 0u64..4_000_000_000],
    ) {
        let now = UNIX_EPOCH + Duration::from_secs(now);
        let mut map = HeaderMap::new();
        map.insert("retry-after", HeaderValue::from_bytes(text.as_bytes()).unwrap());
        let headers = Headers::from(map);
        let read = headers.retry_after_at(now);
        // The header alone, and the decision to retry: the same reading.
        prop_assert_eq!(decide(&headers, now), read, "{:?}", text);
        match digits_model(text.trim_matches([' ', '\t'])) {
            // A number too large for `u64` is as long a wait as can be written.
            Some(seconds) => prop_assert_eq!(read, Some(Duration::from_secs(seconds.unwrap_or(u64::MAX))), "{:?}", text),
            None if !text.contains(|c: char| c.is_ascii_alphabetic()) => prop_assert_eq!(read, None, "{:?}", text),
            // Dates are compared with the grammar in the `Date` reader's tests.
            None => {}
        }
    }

    #[test]
    fn c4_3_retry_after_never_reads_bytes_that_are_not_text(
        bytes in proptest::collection::vec(any::<u8>(), 0..64),
    ) {
        if let Ok(value) = HeaderValue::from_bytes(&bytes) {
            let mut map = HeaderMap::new();
            map.insert("retry-after", value.clone());
            let headers = Headers::from(map);
            let read = headers.retry_after_at(UNIX_EPOCH);
            prop_assert_eq!(decide(&headers, UNIX_EPOCH), read);
            if value.to_str().is_err() {
                prop_assert_eq!(read, None);
            }
        }
    }
}

struct Buffered(usize);
impl Reader<Bytes> for Buffered {
    async fn read(&self, incoming: Incoming) -> Result<Bytes, ReadError> {
        incoming.bytes(self.0).await
    }
}

/// Reads an empty body announced by `length`, with this limit. The body is a stream, so the
/// transport reports no size: the header alone announces one.
fn read_empty(
    length: &HeaderValue,
    limit: usize,
    rt: &tokio::runtime::Runtime,
) -> Result<Bytes, Error> {
    let source = futures_util::stream::iter(Vec::<Result<Bytes, std::io::Error>>::new());
    let mut response = Response::new(reqwest::Body::wrap_stream(source));
    response
        .headers_mut()
        .insert("content-length", length.clone());
    rt.block_on(read_body(
        &Buffered(limit),
        reqwest::Response::from(response),
        &TryInfo::new("props.length", 1),
    ))
}

proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn c5_4_the_announced_size_is_the_content_length_as_the_grammar_reads_it(
        text in length_text(), hint in prop::option::of(0usize..4),
    ) {
        let rt = tokio::runtime::Builder::new_current_thread().enable_time().build().unwrap();
        let value = HeaderValue::from_bytes(text.as_bytes()).unwrap();
        match digits_model(&text) {
            Some(Ok(0)) => prop_assert!(read_empty(&value, 0, &rt).is_ok()),
            Some(Ok(length)) => {
                // Over the limit: a Mismatch before anything is read; within it, a body that
                // ends before its announced size: a Transport error.
                // A length beyond `usize` (on a 32-bit target) is over every limit.
                let under = usize::try_from(length - 1).unwrap_or(usize::MAX);
                prop_assert_eq!(read_empty(&value, under, &rt).unwrap_err().kind(), Kind::Mismatch, "{:?}", text);
                if let Ok(exact) = usize::try_from(length) {
                    prop_assert_eq!(read_empty(&value, exact, &rt).unwrap_err().kind(), Kind::Transport, "{:?}", text);
                }
            }
            // Not a length: nothing is announced, and the transport's exact size counts.
            _ => {
                prop_assert!(read_empty(&value, 0, &rt).unwrap().is_empty(), "{:?}", text);
                if let Some(size) = hint {
                    let mut response = Response::new(reqwest::Body::from(vec![b'x'; size]));
                    response.headers_mut().insert("content-length", value.clone());
                    let read = rt.block_on(read_body::<Bytes, (), _>(
                        &Buffered(size.saturating_sub(1)),
                        reqwest::Response::from(response),
                        &TryInfo::new("props.length", 1),
                    ));
                    prop_assert_eq!(read.is_err(), size > 0);
                }
            }
        }
    }

    #[test]
    fn c2_12_a_written_content_length_must_be_the_length_of_the_body(
        (text, length) in prop_oneof![0usize..3, Just(5), Just(1000)].prop_flat_map(|length| (
            prop_oneof![
                1 => length_text().prop_filter("a header value", |text| text.is_ascii()),
                // The body's own length, written in every way.
                1 => (prop::sample::select(vec!["", "", "+", "-", "0", "00", " ", "\t"]),
                      prop::sample::select(vec!["", "", " ", "\t", ".0", "+"]))
                    .prop_map(move |(before, after)| format!("{before}{length}{after}")),
            ],
            Just(length),
        )),
    ) {
        let mut draft = Draft::new(Method::POST);
        draft.base(&[Piece::Lit("https://example.test/")]);
        draft.body(Body::from(vec![b'x'; length]));
        draft.header("content-length", &[Piece::Val("size", &text)]);
        let built = draft.finish(&Core::<()>::default(), &Options::default(), "props.length");
        let matches = digits_model(&text) == Some(Ok(length as u64));
        prop_assert_eq!(built.is_ok(), matches, "{:?} for {} bytes", text, length);
        if let Err(error) = built {
            prop_assert_eq!(error.to_string(), "build error: the content-length does not match the body length, in props.length");
        }
    }
}

#[test]
fn c4_3_a_retry_after_of_twenty_digits_is_the_longest_wait_and_no_retry() {
    for text in [
        "99999999999999999999",
        "18446744073709551616",
        "000000000000000000000000000000000000000000001",
    ] {
        let mut map = HeaderMap::new();
        map.insert("retry-after", HeaderValue::from_static(text));
        let headers = Headers::from(map);
        let expected = if text.ends_with("001") { 1 } else { u64::MAX };
        assert_eq!(
            headers.retry_after_at(UNIX_EPOCH),
            Some(Duration::from_secs(expected))
        );
        let policy = Retry::<()>::standard();
        let outcome = Outcome {
            kind: Kind::Refused,
            status: Some(StatusCode::TOO_MANY_REQUESTS),
            class: None,
            headers: Some(&headers),
            problem: None,
        };
        let decision = again(
            &policy,
            outcome,
            1,
            &Method::GET,
            Idempotent::ByMethod,
            true,
            None,
            UNIX_EPOCH,
            0.5,
        );
        assert_eq!(decision.header_wait, Some(Duration::from_secs(expected)));
        // Longer than the ceiling of 60 seconds: no further attempt.
        assert_eq!(decision.wait.is_none(), expected > 60);
    }
}

#[test]
fn c5_4_the_corpus_of_numbers_reads_as_written() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    for (line, fields) in corpus::cases(include_str!("corpus/numbers.txt")) {
        let (secs, len) = corpus::text(&fields[0]).split_once(' ').unwrap();
        let value = HeaderValue::from_bytes(&fields[1]).unwrap();
        let mut map = HeaderMap::new();
        map.insert("retry-after", value.clone());
        let headers = Headers::from(map);
        let read = headers.retry_after_at(UNIX_EPOCH);
        assert_eq!(decide(&headers, UNIX_EPOCH), read, "line {line}");
        match secs.strip_prefix("secs=").unwrap() {
            "none" => assert_eq!(read, None, "line {line}"),
            n => assert_eq!(
                read,
                Some(Duration::from_secs(n.parse().unwrap())),
                "line {line}"
            ),
        }
        match len.strip_prefix("len=").unwrap() {
            "none" => assert!(read_empty(&value, 0, &rt).is_ok(), "line {line}"),
            "0" => assert!(read_empty(&value, 0, &rt).is_ok(), "line {line}"),
            n => {
                let n: u64 = n.parse().unwrap();
                // A length beyond `usize` (on a 32-bit target) is over every limit.
                let under = usize::try_from(n - 1).unwrap_or(usize::MAX);
                assert_eq!(
                    read_empty(&value, under, &rt).unwrap_err().kind(),
                    Kind::Mismatch,
                    "line {line}"
                );
            }
        }
    }
}

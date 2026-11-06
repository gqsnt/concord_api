//! Readers over a source the caller checked or wrapped, uniformly for the CSV, JSON Lines, JSON
//! values and SSE readers:
//!
//! - `check_head` accepts and refuses exactly as `open` does (same outcome, same label), over
//!   real loopback responses held as `Incoming`;
//! - `read_for(source, operation)` names the operation in a failure in the middle of the body
//!   (a source failure, a malformed record), where `read` names none;
//! - an `Incoming` read as a `ByteSource` keeps Concord's own `Transport` error for a body that
//!   breaks off mid-stream (the testserver's `Body::FailAfter` shape: chunks, then the connection
//!   is dropped without the terminating chunk), recoverable by value with
//!   `ReadError::into_source_unredacted` + `downcast`;
//! - a provider-typed source error (not `Clone`) round-trips by value the same way;
//! - `Display`/`Debug` stay redacted: no URL, no body content, no source text.
//!
//! Each test owns its loopback peers (one connection each, joined or aborted).
#![cfg(feature = "stream")]
// With `stream` alone only the reader-free tests at the end run.
#![cfg_attr(
    not(any(feature = "csv", feature = "jsonl", feature = "sse")),
    allow(dead_code, unused_imports, unused_macros)
)]

mod support;

use std::error::Error as StdError;
use std::fmt;

use concord_codecs::stream::{
    ByteSource, Chunks, OpenError, OpenErrorKind, ReadError, ReadErrorKind, SourceError,
};
use concord_core::bytes::Bytes;
use concord_core::{CodecError, ErrorKind, Incoming, Operation, ResponseHead};
use support::{Peer, client, read_head, until_closed, write_chunk, write_head};
use tokio::io::AsyncWriteExt;

/// A synthetic secret in the request path: it must not appear in any diagnostic.
const SECRET: &str = "SECRET-7f3a91";
const OP: &str = "Exports.download";

/// A provider's own condition raised by a byte source (ClickHouse's in-body exception, say).
/// Deliberately not `Clone`: it can only come back by value.
#[derive(Debug, PartialEq, Eq)]
struct ProviderError {
    code: u32,
    detail: String,
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "provider error {}: {}", self.code, self.detail)
    }
}

impl StdError for ProviderError {}

/// The given chunks, then a failure carrying a [`ProviderError`] that quotes the secret.
struct Provider {
    chunks: Chunks,
    fail: Option<ProviderError>,
}

impl Provider {
    fn new(prefix: &'static [u8]) -> Self {
        Self {
            chunks: Chunks::new([Bytes::from_static(prefix)]),
            fail: Some(ProviderError {
                code: 241,
                detail: format!("quota exceeded for {SECRET}"),
            }),
        }
    }
}

impl ByteSource for Provider {
    async fn next_chunk(&mut self) -> Result<Option<Bytes>, SourceError> {
        if let Some(chunk) = self.chunks.next_chunk().await? {
            return Ok(Some(chunk));
        }
        match self.fail.take() {
            Some(error) => Err(SourceError::new("the provider ended the body").with_source(error)),
            None => Ok(None),
        }
    }
}

/// One reader under test, seen through what these tests need.
trait Reader: 'static {
    /// One complete record, then the start of a second one.
    const PREFIX: &'static [u8];
    /// One complete record, then a record the reader refuses (`Format` or `Decode`).
    const MALFORMED: &'static [u8];
    /// The `Content-Type` of a body this reader reads.
    const CONTENT_TYPE: &'static str;
    /// `Content-Type` cases, with whether the reader's documented policy accepts them.
    const HEADS: &'static [(Option<&'static str>, bool)];

    fn check_head(&self, head: &ResponseHead<'_>) -> Result<(), CodecError>;
    /// `open(body)`, the records dropped unread.
    fn open(&self, body: Incoming) -> Result<(), OpenError>;
    /// Reads `source` with `read_for(source, operation)` (`read` for `None`): the records
    /// delivered, then the first error (the reader must be fused after it).
    async fn drain<S: ByteSource + 'static>(
        &self,
        source: S,
        operation: Option<&'static str>,
    ) -> (u64, Option<ReadError>);
}

/// Serves one 200 response with `content_type` (or none) and a fixed body.
async fn serve(content_type: Option<&'static str>, body: &'static [u8]) -> Peer<()> {
    Peer::start(move |mut s| async move {
        read_head(&mut s).await;
        let mut headers = vec![format!("content-length: {}", body.len())];
        if let Some(ct) = content_type {
            headers.push(format!("content-type: {ct}"));
        }
        let headers: Vec<&str> = headers.iter().map(String::as_str).collect();
        write_head(&mut s, "200 OK", &headers).await;
        s.write_all(body).await.unwrap();
        until_closed(&mut s).await;
    })
    .await
}

/// `check_head` on the head of an `Incoming` gives the outcome and label `open` gives on it.
async fn check_head_matches_open<R: Reader>(reader: &R) {
    for &(content_type, accepted) in R::HEADS {
        let peer = serve(content_type, R::PREFIX).await;
        let client = client();
        let response = client.get(peer.url()).send().await.unwrap();
        let incoming = Incoming::new(response, Some(Operation::label(OP))).unwrap();
        let checked = reader.check_head(&incoming.head()).map_err(|e| e.label());
        let opened = reader.open(incoming).map_err(|e| {
            assert_eq!(e.kind(), OpenErrorKind::Metadata, "{content_type:?}");
            assert_eq!(e.operation(), Some(OP));
            e.codec().expect("a metadata refusal").label()
        });
        assert_eq!(checked, opened, "{content_type:?}");
        assert_eq!(checked.is_ok(), accepted, "{content_type:?}: {checked:?}");
        drop(client);
        peer.join().await;
    }
}

/// `read_for` names the operation in a source failure and a refused record after a delivered
/// one; `read` names none (unchanged).
async fn read_for_names_the_operation<R: Reader>(reader: &R) {
    let injected = || Chunks::new([Bytes::from_static(R::PREFIX)]).then_fail("injected failure");
    let (records, error) = reader.drain(injected(), Some(OP)).await;
    let error = error.expect("the source fails");
    assert_eq!(records, 1);
    assert_eq!(error.kind(), ReadErrorKind::Transport);
    assert_eq!(error.operation(), Some(OP));
    assert_eq!(error.position().records, 1);
    assert_eq!(
        error.to_string(),
        format!(
            "{OP}: reading failed: injected failure (after 1 records, {} bytes)",
            error.position().bytes
        )
    );
    assert!(
        error.into_source_unredacted().is_none(),
        "no underlying error"
    );

    let (records, error) = reader.drain(injected(), None).await;
    let error = error.expect("the source fails");
    assert_eq!((records, error.operation()), (1, None));
    assert!(
        error
            .to_string()
            .starts_with("reading failed: injected failure")
    );

    let malformed = Chunks::new([Bytes::from_static(R::MALFORMED)]);
    let (records, error) = reader.drain(malformed, Some(OP)).await;
    let error = error.expect("the second record is refused");
    assert_eq!(records, 1);
    assert!(
        matches!(error.kind(), ReadErrorKind::Format | ReadErrorKind::Decode),
        "{error:?}"
    );
    assert_eq!(error.operation(), Some(OP));
    assert!(error.to_string().starts_with(&format!("{OP}: ")), "{error}");
}

/// The body breaks off after `PREFIX` (no terminating chunk). Read through the `Incoming`, the
/// `Transport` error names the operation and gives back Concord's own error by value, URL-free.
async fn incoming_keeps_concords_error<R: Reader>(reader: &R) {
    let content_type = format!("content-type: {}", R::CONTENT_TYPE);
    let prefix = R::PREFIX;
    let peer = Peer::start(move |mut s| async move {
        read_head(&mut s).await;
        write_head(
            &mut s,
            "200 OK",
            &[&content_type, "transfer-encoding: chunked"],
        )
        .await;
        write_chunk(&mut s, prefix).await;
        // Dropped without the terminating chunk.
    })
    .await;
    let client = client();
    let url = format!("{}{SECRET}/export", peer.url());
    let response = client.get(&url).send().await.unwrap();
    let incoming = Incoming::new(response, Some(Operation::label(OP))).unwrap();
    reader.check_head(&incoming.head()).unwrap();
    let (records, error) = reader.drain(incoming, Some(OP)).await;
    let error = error.expect("the body breaks off");
    assert_eq!(records, 1);
    assert_eq!(error.kind(), ReadErrorKind::Transport);
    assert!(error.is_terminal());
    assert_eq!(error.operation(), Some(OP));
    assert_eq!(error.position().records, 1);
    assert_eq!(error.position().bytes, R::PREFIX.len() as u64);
    assert_eq!(
        error.to_string(),
        format!(
            "{OP}: reading failed: reading the response body failed (after 1 records, {} bytes)",
            R::PREFIX.len()
        )
    );
    let shown = format!("{error} {error:?}");
    assert!(shown.contains("<withheld>"), "{shown}");
    assert!(!shown.contains(SECRET), "{shown}");
    let source = error.source_unredacted().expect("Concord's error");
    assert!(source.is::<concord_core::Error>());

    let concord = *error
        .into_source_unredacted()
        .expect("the byte source's error")
        .downcast::<concord_core::Error>()
        .expect("Concord's own error");
    assert_eq!(concord.kind(), ErrorKind::Transport);
    assert_eq!(concord.operation().map(|op| op.name()), Some(OP));
    assert_eq!(
        concord.to_string(),
        format!("{OP}: transport error (reading the response body)")
    );
    let transport = concord.transport().expect("the reqwest error");
    let shown = format!("{concord:?} {transport} {transport:?}");
    assert!(!shown.contains(SECRET), "{shown}");
    drop(client);
    peer.join().await;
}

/// A provider-typed error attached by a source comes back by value; the reader's diagnostics
/// never show it.
async fn provider_error_round_trips<R: Reader>(reader: &R) {
    let (records, error) = reader.drain(Provider::new(R::PREFIX), Some(OP)).await;
    let error = error.expect("the provider ends the body");
    assert_eq!(records, 1);
    assert_eq!(error.kind(), ReadErrorKind::Transport);
    assert_eq!(error.operation(), Some(OP));
    assert_eq!(
        error.codec().map(CodecError::label),
        Some("the provider ended the body")
    );
    let shown = format!("{error} {error:?}");
    assert!(!shown.contains(SECRET), "{shown}");
    assert!(!shown.contains("quota"), "{shown}");
    assert!(
        error
            .source_unredacted()
            .is_some_and(|source| source.is::<ProviderError>())
    );
    let provider = error
        .into_source_unredacted()
        .expect("the byte source's error")
        .downcast::<ProviderError>()
        .expect("the provider's own type");
    assert_eq!(
        *provider,
        ProviderError {
            code: 241,
            detail: format!("quota exceeded for {SECRET}"),
        }
    );
}

#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
struct Row {
    id: u32,
}

/// Collects `next()` results until the first error, then checks that the reader is fused.
macro_rules! drain {
    ($records:expr) => {{
        let mut records = $records;
        let mut delivered = 0;
        loop {
            match records.next().await {
                None => break (delivered, None),
                Some(Ok(_)) => delivered += 1,
                Some(Err(error)) => {
                    assert!(records.next().await.is_none(), "fused after {error:?}");
                    break (delivered, Some(error));
                }
            }
        }
    }};
}

#[cfg(feature = "csv")]
mod csv {
    use concord_codecs::csv::{CsvOptions, CsvReader, HeaderMode};

    use super::*;

    struct Csv(CsvReader);

    fn reader() -> Csv {
        Csv(CsvReader::new(CsvOptions::new(HeaderMode::Present, b',')).unwrap())
    }

    impl Reader for Csv {
        const PREFIX: &'static [u8] = b"id\n1\n2";
        const MALFORMED: &'static [u8] = b"id\n1\nnot-a-number\n";
        const CONTENT_TYPE: &'static str = "text/csv; header=present";
        const HEADS: &'static [(Option<&'static str>, bool)] = &[
            (Some("text/csv"), true),
            (Some("text/csv; header=present; charset=utf-8"), true),
            (Some("text/csv; header=absent"), false),
            (Some("text/csv; charset=latin1"), false),
            (Some("application/json"), false),
            (None, false),
        ];

        fn check_head(&self, head: &ResponseHead<'_>) -> Result<(), CodecError> {
            self.0.check_head(head)
        }

        fn open(&self, body: Incoming) -> Result<(), OpenError> {
            self.0.open::<Row>(body).map(drop)
        }

        async fn drain<S: ByteSource + 'static>(
            &self,
            source: S,
            operation: Option<&'static str>,
        ) -> (u64, Option<ReadError>) {
            match operation {
                Some(op) => drain!(self.0.read_for::<Row, _>(source, op)),
                None => drain!(self.0.read::<Row, _>(source)),
            }
        }
    }

    #[tokio::test]
    async fn check_head_matches_open() {
        super::check_head_matches_open(&reader()).await;
    }

    #[tokio::test]
    async fn read_for_names_the_operation() {
        super::read_for_names_the_operation(&reader()).await;
    }

    #[tokio::test]
    async fn incoming_keeps_concords_error() {
        super::incoming_keeps_concords_error(&reader()).await;
    }

    #[tokio::test]
    async fn provider_error_round_trips() {
        super::provider_error_round_trips(&reader()).await;
    }

    /// The raw-record variant names the operation too.
    #[tokio::test]
    async fn read_raw_for_names_the_operation() {
        let source = Chunks::new([Bytes::from_static(b"id\n1\n")]).then_fail("injected failure");
        let (records, error) = drain!(reader().0.read_raw_for(source, OP));
        assert_eq!(records, 1);
        assert_eq!(error.expect("the source fails").operation(), Some(OP));
    }
}

#[cfg(feature = "jsonl")]
mod jsonl {
    use concord_codecs::jsonl::{JsonLinesOptions, JsonLinesReader, MediaPolicy};

    use super::*;

    struct Lines(JsonLinesReader);

    /// NDJSON with a deliberate extra media type: `check_head` applies the configured policy.
    fn reader() -> Lines {
        Lines(JsonLinesReader::new(JsonLinesOptions::ndjson().media(
            MediaPolicy::standard_or(&["application/octet-stream"]),
        )))
    }

    impl Reader for Lines {
        const PREFIX: &'static [u8] = b"{\"id\":1}\n{\"id\":";
        const MALFORMED: &'static [u8] = b"{\"id\":1}\n{\"id\":\"x\"}\n";
        const CONTENT_TYPE: &'static str = "application/x-ndjson";
        const HEADS: &'static [(Option<&'static str>, bool)] = &[
            (Some("application/x-ndjson"), true),
            (Some("application/ndjson; charset=utf-8"), true),
            (Some("application/octet-stream"), true),
            (Some("application/jsonl"), false),
            (Some("application/x-ndjson; charset=latin1"), false),
            (None, false),
        ];

        fn check_head(&self, head: &ResponseHead<'_>) -> Result<(), CodecError> {
            self.0.check_head(head)
        }

        fn open(&self, body: Incoming) -> Result<(), OpenError> {
            self.0.open::<Row>(body).map(drop)
        }

        async fn drain<S: ByteSource + 'static>(
            &self,
            source: S,
            operation: Option<&'static str>,
        ) -> (u64, Option<ReadError>) {
            match operation {
                Some(op) => drain!(self.0.read_for::<Row, _>(source, op)),
                None => drain!(self.0.read::<Row, _>(source)),
            }
        }
    }

    #[tokio::test]
    async fn check_head_matches_open() {
        super::check_head_matches_open(&reader()).await;
    }

    #[tokio::test]
    async fn read_for_names_the_operation() {
        super::read_for_names_the_operation(&reader()).await;
    }

    #[tokio::test]
    async fn incoming_keeps_concords_error() {
        super::incoming_keeps_concords_error(&reader()).await;
    }

    #[tokio::test]
    async fn provider_error_round_trips() {
        super::provider_error_round_trips(&reader()).await;
    }

    /// The static NDJSON format's head check and the preset reader's agree.
    #[test]
    fn the_static_format_and_the_preset_reader_agree() {
        use concord_codecs::jsonl::Ndjson;
        use concord_core::Incremental;
        use concord_core::reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderValue};
        use concord_core::reqwest::{StatusCode, Version};

        let preset = JsonLinesReader::new(JsonLinesOptions::ndjson());
        for value in ["application/x-ndjson", "application/jsonl", "text/plain"] {
            let mut headers = HeaderMap::new();
            headers.insert(CONTENT_TYPE, HeaderValue::from_static(value));
            let head = ResponseHead::new(StatusCode::OK, Version::HTTP_11, &headers);
            assert_eq!(
                preset.check_head(&head).map_err(|e| e.label()),
                <Ndjson as Incremental<Row>>::check_content_type(&head).map_err(|e| e.label()),
                "{value}"
            );
        }
    }

    /// A parser's error stays in the codec (by reference); only a source's error comes back by
    /// value.
    #[tokio::test]
    async fn a_parser_error_is_not_given_back_by_value() {
        let (_, error) = reader()
            .drain(
                Chunks::new([Bytes::from_static(b"{\"id\":\"x\"}\n")]),
                Some(OP),
            )
            .await;
        let error = error.expect("a decode error");
        assert_eq!(error.kind(), ReadErrorKind::Decode);
        assert!(error.source_unredacted().is_some(), "the serde error");
        assert!(error.into_source_unredacted().is_none());
    }

    struct Values(concord_codecs::jsonl::JsonValuesReader);

    fn values() -> Values {
        Values(concord_codecs::jsonl::JsonValuesReader::new())
    }

    impl Reader for Values {
        const PREFIX: &'static [u8] = b"{\"id\":1}\n{\"id\":";
        const MALFORMED: &'static [u8] = b"{\"id\":1} {\"id\":\"x\"}";
        const CONTENT_TYPE: &'static str = "application/json";
        const HEADS: &'static [(Option<&'static str>, bool)] = &[
            (Some("application/json"), true),
            (Some("application/vnd.watch+json; charset=utf-8"), true),
            (Some("application/x-ndjson"), false),
            (Some("application/json; charset=latin1"), false),
            (Some("text/plain"), false),
            (None, false),
        ];

        fn check_head(&self, head: &ResponseHead<'_>) -> Result<(), CodecError> {
            self.0.check_head(head)
        }

        fn open(&self, body: Incoming) -> Result<(), OpenError> {
            self.0.open::<Row>(body).map(drop)
        }

        async fn drain<S: ByteSource + 'static>(
            &self,
            source: S,
            operation: Option<&'static str>,
        ) -> (u64, Option<ReadError>) {
            match operation {
                Some(op) => drain!(self.0.read_for::<Row, _>(source, op)),
                None => drain!(self.0.read::<Row, _>(source)),
            }
        }
    }

    #[tokio::test]
    async fn values_check_head_matches_open() {
        super::check_head_matches_open(&values()).await;
    }

    #[tokio::test]
    async fn values_read_for_names_the_operation() {
        super::read_for_names_the_operation(&values()).await;
    }

    #[tokio::test]
    async fn values_incoming_keeps_concords_error() {
        super::incoming_keeps_concords_error(&values()).await;
    }

    #[tokio::test]
    async fn values_provider_error_round_trips() {
        super::provider_error_round_trips(&values()).await;
    }
}

#[cfg(feature = "sse")]
mod sse {
    use concord_codecs::sse::{SseOptions, SseReader};

    use super::*;

    struct Events(SseReader);

    fn reader() -> Events {
        Events(SseReader::new(SseOptions::new().max_event_bytes(4096)))
    }

    impl Reader for Events {
        const PREFIX: &'static [u8] = b"data: one\n\nevent: update\ndata: {\"partial\":";
        const MALFORMED: &'static [u8] = b"data: one\n\ndata: \xFF\n\n";
        const CONTENT_TYPE: &'static str = "text/event-stream";
        const HEADS: &'static [(Option<&'static str>, bool)] = &[
            (Some("text/event-stream"), true),
            (Some("Text/Event-Stream; charset=utf-8"), true),
            (Some("text/event-stream; charset=latin1"), false),
            (Some("text/plain"), false),
            (None, false),
        ];

        fn check_head(&self, head: &ResponseHead<'_>) -> Result<(), CodecError> {
            self.0.check_head(head)
        }

        fn open(&self, body: Incoming) -> Result<(), OpenError> {
            self.0.open(body).map(drop)
        }

        async fn drain<S: ByteSource + 'static>(
            &self,
            source: S,
            operation: Option<&'static str>,
        ) -> (u64, Option<ReadError>) {
            match operation {
                Some(op) => drain!(self.0.read_for(source, op)),
                None => drain!(self.0.read(source)),
            }
        }
    }

    #[tokio::test]
    async fn check_head_matches_open() {
        super::check_head_matches_open(&reader()).await;
    }

    #[tokio::test]
    async fn read_for_names_the_operation() {
        super::read_for_names_the_operation(&reader()).await;
    }

    #[tokio::test]
    async fn incoming_keeps_concords_error() {
        super::incoming_keeps_concords_error(&reader()).await;
    }

    #[tokio::test]
    async fn provider_error_round_trips() {
        super::provider_error_round_trips(&reader()).await;
    }

    /// The static `Sse` format's head check and the reader's agree (the reader has no policy).
    #[test]
    fn the_static_format_and_the_reader_agree() {
        use concord_codecs::sse::{Event, Sse};
        use concord_core::Incremental;
        use concord_core::reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderValue};
        use concord_core::reqwest::{StatusCode, Version};

        for value in [
            "text/event-stream",
            "text/plain",
            "text/event-stream; charset=a",
        ] {
            let mut headers = HeaderMap::new();
            headers.insert(CONTENT_TYPE, HeaderValue::from_static(value));
            let head = ResponseHead::new(StatusCode::OK, Version::HTTP_11, &headers);
            assert_eq!(
                reader().check_head(&head).map_err(|e| e.label()),
                <Sse as Incremental<Event>>::check_content_type(&head).map_err(|e| e.label()),
                "{value}"
            );
        }
    }
}

/// Without a reader: an `Incoming` read chunk by chunk (a caller's own loop, a filter of its
/// own) fails with a `SourceError` that gives Concord's error back by value.
#[tokio::test]
async fn an_incoming_source_error_gives_concords_error_back() {
    let peer = Peer::start(|mut s| async move {
        read_head(&mut s).await;
        write_head(&mut s, "200 OK", &["transfer-encoding: chunked"]).await;
        write_chunk(&mut s, b"partial").await;
    })
    .await;
    let client = client();
    let url = format!("{}{SECRET}/export", peer.url());
    let response = client.get(&url).send().await.unwrap();
    let mut body = Incoming::new(response, Some(Operation::label(OP))).unwrap();
    let mut read = Vec::new();
    let error = loop {
        match body.next_chunk().await {
            Ok(Some(chunk)) => read.extend_from_slice(&chunk),
            Ok(None) => panic!("the body breaks off"),
            Err(error) => break error,
        }
    };
    assert_eq!(read, b"partial");
    assert_eq!(error.label(), "reading the response body failed");
    let shown = format!("{error} {error:?}");
    assert!(!shown.contains(SECRET), "{shown}");
    assert!(shown.contains("<withheld>"), "{shown}");
    let concord = error
        .into_source_unredacted()
        .expect("Concord's error")
        .downcast::<concord_core::Error>()
        .expect("Concord's own type");
    assert_eq!(concord.kind(), ErrorKind::Transport);
    assert_eq!(concord.operation().map(|op| op.name()), Some(OP));
    assert!(!format!("{concord} {concord:?}").contains(SECRET));
    drop(body);
    drop(client);
    peer.join().await;
}

/// A source's provider error round-trips by value through `SourceError` alone.
#[tokio::test]
async fn a_source_error_gives_its_source_back_by_value() {
    let mut source = Provider::new(b"");
    assert_eq!(source.next_chunk().await.unwrap(), Some(Bytes::new()));
    let error = source.next_chunk().await.unwrap_err();
    assert_eq!(
        format!("{error} | {error:?}"),
        "the provider ended the body | SourceError { label: \"the provider ended the body\", source: Some(\"<withheld>\") }"
    );
    let provider = error
        .into_source_unredacted()
        .unwrap()
        .downcast::<ProviderError>()
        .unwrap();
    assert_eq!(provider.code, 241);
    assert!(
        SourceError::new("no source")
            .into_source_unredacted()
            .is_none()
    );
}

//! Opening and reading errors of the incremental readers.

use std::error::Error as StdError;
use std::fmt;

use concord_core::reqwest::{self, StatusCode};
use concord_core::{CodecError, ResponseHead};

use super::source::SourceError;

/// Why a reader refused a response before polling its body. The response is kept **unread**:
/// inspect it with [`response`](Self::response) or take it back with
/// [`into_response`](Self::into_response) (to read a provider's error document, say).
pub struct OpenError {
    inner: Box<OpenInner>,
}

struct OpenInner {
    operation: Option<&'static str>,
    codec: Option<CodecError>,
    response: reqwest::Response,
}

/// The stage of an [`OpenError`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum OpenErrorKind {
    /// The status is not 2xx.
    Status,
    /// The status is 2xx, but the format refused the head (`Content-Type`, a `header=` parameter
    /// that contradicts the configured header policy).
    Metadata,
}

impl OpenError {
    pub(crate) fn status(operation: Option<&'static str>, response: reqwest::Response) -> Self {
        Self {
            inner: Box::new(OpenInner {
                operation,
                codec: None,
                response,
            }),
        }
    }

    pub(crate) fn metadata(
        operation: Option<&'static str>,
        error: CodecError,
        response: reqwest::Response,
    ) -> Self {
        Self {
            inner: Box::new(OpenInner {
                operation,
                codec: Some(error),
                response,
            }),
        }
    }

    /// The stage.
    pub fn kind(&self) -> OpenErrorKind {
        if self.inner.codec.is_some() {
            OpenErrorKind::Metadata
        } else {
            OpenErrorKind::Status
        }
    }

    /// The operation name supplied when the response was opened, if any.
    pub fn operation(&self) -> Option<&'static str> {
        self.inner.operation
    }

    /// The response status.
    pub fn status_code(&self) -> StatusCode {
        self.inner.response.status()
    }

    /// The format's refusal, for [`OpenErrorKind::Metadata`].
    pub fn codec(&self) -> Option<&CodecError> {
        self.inner.codec.as_ref()
    }

    /// The unread response.
    pub fn response(&self) -> &reqwest::Response {
        &self.inner.response
    }

    /// The unread response, taken back.
    pub fn into_response(self) -> reqwest::Response {
        self.inner.response
    }
}

impl fmt::Display for OpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(op) = self.inner.operation {
            write!(f, "{op}: ")?;
        }
        match &self.inner.codec {
            None => write!(
                f,
                "the server answered status {} (response kept unread)",
                self.inner.response.status().as_u16()
            ),
            Some(codec) => write!(
                f,
                "the response head was refused: {codec} (response kept unread)"
            ),
        }
    }
}

impl fmt::Debug for OpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpenError")
            .field("kind", &self.kind())
            .field("operation", &self.inner.operation)
            .field("status", &self.inner.response.status().as_u16())
            .field("codec", &self.inner.codec)
            .finish()
    }
}

impl StdError for OpenError {}

/// Requires a 2xx status, then runs `check` on the borrowed head; both before the body is polled.
pub(crate) fn open_response(
    response: reqwest::Response,
    operation: Option<&'static str>,
    check: impl FnOnce(&ResponseHead<'_>) -> Result<(), CodecError>,
) -> Result<reqwest::Response, OpenError> {
    if !response.status().is_success() {
        return Err(OpenError::status(operation, response));
    }
    match check(&ResponseHead::of(&response)) {
        Ok(()) => Ok(response),
        Err(e) => Err(OpenError::metadata(operation, e, response)),
    }
}

/// Where an incremental read stood: records delivered to the caller and input bytes consumed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Position {
    /// Records delivered by `next()` (a CSV header is not a record).
    pub records: u64,
    /// Input bytes consumed by the parser (decoded bytes, after any HTTP decompression).
    pub bytes: u64,
}

/// The stage of a [`ReadError`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ReadErrorKind {
    /// The byte source failed (a transport error mid-body, an injected source failure).
    Transport,
    /// The bytes do not follow the format's grammar or table rules (malformed JSON, a CSV row
    /// of the wrong width, a duplicate CSV header name, invalid UTF-8, input ended inside a
    /// record).
    Format,
    /// A well-formed record does not deserialize into the requested type.
    Decode,
    /// A configured limit was reached (record, header or field size; field count; total
    /// records or bytes). Never reported as a clean end of input.
    Limit,
}

/// A failure after an incremental reader started polling the body. Earlier records stay
/// delivered. The reader is fused (every later `next()` returns `None`) unless
/// [`is_terminal`](Self::is_terminal) is `false`: only a JSON Lines reader in
/// `RecordErrors::Yield` mode reports a fully delimited bad record that way and goes on reading.
pub struct ReadError {
    inner: Box<ReadInner>,
}

struct ReadInner {
    kind: ReadErrorKind,
    operation: Option<&'static str>,
    position: Position,
    codec: Option<CodecError>,
    source: Option<Box<dyn StdError + Send + Sync>>,
    terminal: bool,
}

impl ReadError {
    pub(crate) fn new(kind: ReadErrorKind, codec: CodecError, position: Position) -> Self {
        Self {
            inner: Box::new(ReadInner {
                kind,
                operation: None,
                position,
                codec: Some(codec),
                source: None,
                terminal: true,
            }),
        }
    }

    /// A record-level failure after which the reader continues (JSON Lines `Yield` mode).
    #[cfg_attr(not(feature = "jsonl"), allow(dead_code))]
    pub(crate) fn recoverable(mut self) -> Self {
        self.inner.terminal = false;
        self
    }

    pub(crate) fn transport(error: SourceError, position: Position) -> Self {
        let label = error.label();
        Self {
            inner: Box::new(ReadInner {
                kind: ReadErrorKind::Transport,
                operation: None,
                position,
                codec: Some(CodecError::new(label)),
                source: error.into_source(),
                terminal: true,
            }),
        }
    }

    pub(crate) fn with_operation(mut self, operation: Option<&'static str>) -> Self {
        if self.inner.operation.is_none() {
            self.inner.operation = operation;
        }
        self
    }

    /// The stage.
    pub fn kind(&self) -> ReadErrorKind {
        self.inner.kind
    }

    /// Whether this error ended the read. `true` for every failure except a fully delimited bad
    /// record reported by a JSON Lines reader in `RecordErrors::Yield` mode, after which `next()`
    /// goes on with the following record. A caller that counts skipped records must check this:
    /// a terminal error (a refused byte-order mark, a limit, a transport failure, input ending
    /// inside a record) is never a skipped record, and the reader returns `None` afterwards.
    pub fn is_terminal(&self) -> bool {
        self.inner.terminal
    }

    /// The operation name supplied when the response was opened, if any.
    pub fn operation(&self) -> Option<&'static str> {
        self.inner.operation
    }

    /// Records delivered and bytes consumed before the failure.
    pub fn position(&self) -> Position {
        self.inner.position
    }

    /// The static description and numeric positions of the failure.
    pub fn codec(&self) -> Option<&CodecError> {
        self.inner.codec.as_ref()
    }

    /// The underlying error, **unredacted**: a transport error (its URL already removed), a
    /// serde error quoting the input. Read it deliberately; it is not chained by `source()`.
    pub fn source_unredacted(&self) -> Option<&(dyn StdError + Send + Sync + 'static)> {
        if let Some(source) = self.inner.source.as_deref() {
            return Some(source);
        }
        self.inner
            .codec
            .as_ref()
            .and_then(CodecError::source_unredacted)
    }
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(op) = self.inner.operation {
            write!(f, "{op}: ")?;
        }
        f.write_str(match self.inner.kind {
            ReadErrorKind::Transport => "reading failed",
            ReadErrorKind::Format => "malformed input",
            ReadErrorKind::Decode => "a record does not decode",
            ReadErrorKind::Limit => "a limit was reached",
        })?;
        if let Some(codec) = &self.inner.codec {
            write!(f, ": {codec}")?;
        }
        write!(
            f,
            " (after {} records, {} bytes)",
            self.inner.position.records, self.inner.position.bytes
        )
    }
}

impl fmt::Debug for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReadError")
            .field("kind", &self.inner.kind)
            .field("operation", &self.inner.operation)
            .field("position", &self.inner.position)
            .field("terminal", &self.inner.terminal)
            .field("codec", &self.inner.codec)
            .field("source", &self.inner.source.as_ref().map(|_| "<withheld>"))
            .finish()
    }
}

/// No `source()`: the underlying error may quote content.
impl StdError for ReadError {}

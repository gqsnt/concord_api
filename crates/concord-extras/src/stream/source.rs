//! Where an incremental reader gets its bytes.

use std::collections::VecDeque;
use std::error::Error as StdError;
use std::fmt;
use std::future::Future;

use concord_core::bytes::Bytes;
use concord_core::{Incoming, reqwest};

/// A one-shot sequence of byte chunks read by an incremental reader: a native response body, an
/// [`Incoming`] body, or another source (an in-memory [`Chunks`] list, a file adapter of yours, a
/// filter of yours wrapped around one of these).
///
/// `Ok(None)` is the true end of input; a chunk boundary is never an end. Chunks may be empty and
/// may split a record, a UTF-8 character or a CSV quote anywhere.
///
/// # Cancellation
///
/// A reader's `next()` is cancel-safe (a dropped pending call loses nothing) only if
/// `next_chunk` is: a `next_chunk` future dropped before it completes must not have taken input
/// from wherever the source gets it. A source that takes a chunk and then awaits something else
/// keeps that chunk in `self` until it returns it. Otherwise a dropped `next()` loses the chunk,
/// and a later call may even report a clean end of input. `reqwest::Response` (its `chunk()`
/// takes a body frame only once one is ready), [`Incoming`] (likewise: [`Incoming::chunk`]) and
/// [`Chunks`] (it completes on its first poll) are cancel-safe. Whatever the source, a reader's
/// `into_stream()` ([`RecordStream`](super::RecordStream)) keeps a pending read, the source's
/// future included, across a dropped `next()`.
pub trait ByteSource: Send {
    /// The next chunk, `Ok(None)` at the end of input.
    fn next_chunk(&mut self) -> impl Future<Output = Result<Option<Bytes>, SourceError>> + Send;
}

impl ByteSource for reqwest::Response {
    /// The next chunk of the (possibly decompressed) body; a transport failure is a
    /// [`SourceError`] whose underlying `reqwest::Error` has its URL removed.
    async fn next_chunk(&mut self) -> Result<Option<Bytes>, SourceError> {
        self.chunk().await.map_err(|e| {
            SourceError::new("reading the response body failed").with_source(e.without_url())
        })
    }
}

/// The unread body of an endpoint declared `-> stream` (or of [`Received::into_incoming`], or of
/// [`Incoming::new`]) as a byte source, for a reader's `read_for(body, operation)`: when the head
/// was already checked (an `Incremental` format's `check_content_type`, a reader's `check_head`),
/// or when a filter of yours sits between the body and the reader. Nothing is checked here; the
/// status is a 2xx already.
///
/// A transport failure is a [`SourceError`] labelled "reading the response body failed" whose
/// underlying error is Concord's own [`concord_core::Error`]: kind
/// [`Transport`](concord_core::ErrorKind::Transport), naming the operation the `Incoming` carries,
/// its URL removed. A reader's [`ReadError`](super::ReadError) of kind `Transport` gives it back by
/// value ([`ReadError::into_source_unredacted`](super::ReadError::into_source_unredacted), then
/// `downcast::<concord_core::Error>()`). A reader's `open(incoming)` reads the native response
/// instead, whose failure carries a `reqwest::Error`.
///
/// [`Received::into_incoming`]: concord_core::Received::into_incoming
impl ByteSource for Incoming {
    /// The next chunk of the body ([`Incoming::chunk`]); a failure keeps Concord's `Transport`
    /// error as the [`SourceError`]'s source.
    async fn next_chunk(&mut self) -> Result<Option<Bytes>, SourceError> {
        self.chunk()
            .await
            .map_err(|e| SourceError::new("reading the response body failed").with_source(e))
    }
}

/// An in-memory [`ByteSource`]: the given chunks in order, optionally followed by a failure.
/// Useful to test a parser on chosen chunk boundaries without a network.
///
/// Its `Debug` shows the number of chunks left, their total bytes and whether a failure follows,
/// never their content.
#[derive(Default)]
pub struct Chunks {
    chunks: VecDeque<Bytes>,
    fail_at_end: Option<&'static str>,
}

impl Chunks {
    /// The chunks, then the end of input.
    pub fn new<I, B>(chunks: I) -> Self
    where
        I: IntoIterator<Item = B>,
        B: Into<Bytes>,
    {
        Self {
            chunks: chunks.into_iter().map(Into::into).collect(),
            fail_at_end: None,
        }
    }

    /// The chunks, then a [`SourceError`] labelled `label` instead of the end of input.
    #[must_use]
    pub fn then_fail(mut self, label: &'static str) -> Self {
        self.fail_at_end = Some(label);
        self
    }
}

impl fmt::Debug for Chunks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Chunks")
            .field("chunks", &self.chunks.len())
            .field("bytes", &self.chunks.iter().map(Bytes::len).sum::<usize>())
            .field("fails_at_end", &self.fail_at_end.is_some())
            .finish()
    }
}

impl ByteSource for Chunks {
    async fn next_chunk(&mut self) -> Result<Option<Bytes>, SourceError> {
        match self.chunks.pop_front() {
            Some(chunk) => Ok(Some(chunk)),
            None => match self.fail_at_end.take() {
                Some(label) => Err(SourceError::new(label)),
                None => Ok(None),
            },
        }
    }
}

/// A [`ByteSource`] failed: a static label, and the underlying error kept for deliberate
/// inspection only ([`source_unredacted`](Self::source_unredacted), or by value with
/// [`into_source_unredacted`](Self::into_source_unredacted)).
pub struct SourceError {
    label: &'static str,
    source: Option<Box<dyn StdError + Send + Sync>>,
}

impl SourceError {
    /// An error described by a static, content-free label.
    pub fn new(label: &'static str) -> Self {
        Self {
            label,
            source: None,
        }
    }

    /// Attaches the underlying error (which may quote a URL or content).
    #[must_use]
    pub fn with_source(mut self, source: impl Into<Box<dyn StdError + Send + Sync>>) -> Self {
        self.source = Some(source.into());
        self
    }

    /// The static label.
    pub fn label(&self) -> &'static str {
        self.label
    }

    /// The underlying error, **unredacted**.
    pub fn source_unredacted(&self) -> Option<&(dyn StdError + Send + Sync + 'static)> {
        self.source.as_deref()
    }

    /// The underlying error, **unredacted**, taken back by value: `downcast` it to the type the
    /// source attached (Concord's `Error` for an [`Incoming`], a `reqwest::Error` for a native
    /// response, a type of yours) to keep it without a copy. The label is dropped.
    pub fn into_source_unredacted(self) -> Option<Box<dyn StdError + Send + Sync + 'static>> {
        self.source
    }
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label)
    }
}

impl fmt::Debug for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SourceError")
            .field("label", &self.label)
            .field("source", &self.source.as_ref().map(|_| "<withheld>"))
            .finish()
    }
}

/// No `source()`: the underlying error may quote a URL or content.
impl StdError for SourceError {}

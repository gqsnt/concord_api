//! Input bookkeeping shared by the incremental readers: the unconsumed part of the current chunk,
//! end of input, positions, the operation name and the fused state.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use concord_core::CodecError;
use concord_core::bytes::{Buf, Bytes};

use super::error::{Position, ReadError, ReadErrorKind};
use super::source::ByteSource;

/// Input bytes a reader may frame in one poll (skipped comments, blank lines and separators, or
/// part of a large record) before it yields to the executor ([`yield_now`]): a reader hands its
/// parser at most this much of the current chunk between two yields, so neither a long ignored
/// run nor one large ready chunk can make a poll unbounded.
pub(crate) const SKIP_BUDGET: usize = 64 * 1024;

/// Empty chunks the driver accepts from its source between two of its own yields: a source that
/// is always ready with empty chunks costs at most this many source polls per poll of a reader
/// (an endless run no longer holds one poll forever), and an empty chunk stays distinct from the
/// end of input. Each nonempty chunk carries at least one byte of the reader's [`SKIP_BUDGET`].
pub(crate) const EMPTY_CHUNK_BUDGET: usize = 64;

pub(crate) struct Driver<S> {
    source: S,
    chunk: Bytes,
    eof: bool,
    consumed: u64,
    delivered: u64,
    operation: Option<&'static str>,
    fused: bool,
    max_bytes: Option<u64>,
    max_records: Option<u64>,
    /// Empty chunks received since the last yield they caused.
    empty_chunks: usize,
}

impl<S: ByteSource> Driver<S> {
    pub(crate) fn new(source: S, operation: Option<&'static str>) -> Self {
        Self {
            source,
            chunk: Bytes::new(),
            eof: false,
            consumed: 0,
            delivered: 0,
            operation,
            fused: false,
            max_bytes: None,
            max_records: None,
            empty_chunks: 0,
        }
    }

    /// Optional totals: reaching them is a [`ReadErrorKind::Limit`] error, never a clean end.
    pub(crate) fn with_totals(mut self, max_bytes: Option<u64>, max_records: Option<u64>) -> Self {
        self.max_bytes = max_bytes;
        self.max_records = max_records;
        self
    }

    /// The unconsumed bytes of the current chunk.
    pub(crate) fn input(&self) -> &[u8] {
        &self.chunk
    }

    /// Marks `n` bytes of [`input`](Self::input) consumed.
    pub(crate) fn consume(&mut self, n: usize) {
        self.chunk.advance(n);
        self.consumed += n as u64;
    }

    /// Takes `n` bytes of [`input`](Self::input) as an owned (shared, not copied) buffer.
    // Not used by the CSV reader (it copies fields into owned records).
    #[allow(dead_code)]
    pub(crate) fn take(&mut self, n: usize) -> Bytes {
        self.consumed += n as u64;
        self.chunk.split_to(n)
    }

    /// The current chunk is used up and the source has ended.
    pub(crate) fn at_eof(&self) -> bool {
        self.eof && self.chunk.is_empty()
    }

    /// Reads the next nonempty chunk once the current one is used up. `Ok(false)`: end of input.
    ///
    /// Yields to the executor after every [`EMPTY_CHUNK_BUDGET`] empty chunks. Cancel-safe: a
    /// dropped call has consumed only empty chunks.
    pub(crate) async fn refill(&mut self) -> Result<bool, ReadError> {
        debug_assert!(self.chunk.is_empty());
        if self.eof {
            return Ok(false);
        }
        loop {
            match self.source.next_chunk().await {
                Ok(Some(chunk)) if chunk.is_empty() => {
                    self.empty_chunks += 1;
                    if self.empty_chunks >= EMPTY_CHUNK_BUDGET {
                        self.empty_chunks = 0;
                        yield_now().await;
                    }
                }
                Ok(Some(chunk)) => {
                    if let Some(max) = self.max_bytes
                        && self.consumed + chunk.len() as u64 > max
                    {
                        let position = self.position();
                        return Err(self.fail(ReadError::new(
                            ReadErrorKind::Limit,
                            CodecError::new("the input exceeds the configured total bytes")
                                .at("limit", max),
                            position,
                        )));
                    }
                    self.chunk = chunk;
                    return Ok(true);
                }
                Ok(None) => {
                    self.eof = true;
                    return Ok(false);
                }
                Err(e) => {
                    let position = self.position();
                    return Err(self.fail(ReadError::transport(e, position)));
                }
            }
        }
    }

    /// Records a delivered record; reaching the optional record total afterwards is checked by
    /// [`check_record_total`](Self::check_record_total) before the next one.
    pub(crate) fn delivered(&mut self) {
        self.delivered += 1;
    }

    /// A [`ReadErrorKind::Limit`] error if one more record would exceed the configured total.
    pub(crate) fn check_record_total(&mut self) -> Result<(), ReadError> {
        match self.max_records {
            Some(max) if self.delivered >= max => Err(self.fail(ReadError::new(
                ReadErrorKind::Limit,
                CodecError::new("the input exceeds the configured total records").at("limit", max),
                self.position(),
            ))),
            _ => Ok(()),
        }
    }

    pub(crate) fn position(&self) -> Position {
        Position {
            records: self.delivered,
            bytes: self.consumed,
        }
    }

    /// Fuses the reader and names the operation in `error`.
    pub(crate) fn fail(&mut self, error: ReadError) -> ReadError {
        self.fused = true;
        self.chunk = Bytes::new();
        error.with_operation(self.operation)
    }

    /// A terminal error of `kind` at the current position.
    pub(crate) fn error(&mut self, kind: ReadErrorKind, codec: CodecError) -> ReadError {
        let position = self.position();
        self.fail(ReadError::new(kind, codec, position))
    }

    pub(crate) fn is_fused(&self) -> bool {
        self.fused
    }

    /// Ends reading cleanly (later `next()` calls return `None`).
    pub(crate) fn finish(&mut self) {
        self.fused = true;
    }

    pub(crate) fn operation(&self) -> Option<&'static str> {
        self.operation
    }

    /// The source, for a caller that needs it after a failure (`into_source`).
    pub(crate) fn into_source(self) -> S {
        self.source
    }
}

/// Returns `Pending` once after waking itself: lets the executor run other work in the middle of
/// a long input run (or a long run of empty chunks), without depending on a particular runtime.
pub(crate) fn yield_now() -> YieldNow {
    YieldNow(false)
}

pub(crate) struct YieldNow(bool);

impl Future for YieldNow {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.0 {
            Poll::Ready(())
        } else {
            self.0 = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

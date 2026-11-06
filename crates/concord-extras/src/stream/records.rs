//! [`RecordStream`]: an incremental reader as an owning standard [`Stream`].

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::Stream;

use super::error::{Position, ReadError};

/// An incremental reader of records `T` (`CsvRecords`, `JsonLinesRecords`, `JsonValues`): what
/// [`RecordStream`] drives. Sealed; implemented by this crate's readers.
pub trait RecordReader: Send + 'static + sealed::Sealed {
    /// The record type.
    type Record: Send + 'static;

    #[doc(hidden)]
    fn next_record(
        &mut self,
    ) -> impl Future<Output = Option<Result<Self::Record, ReadError>>> + Send;

    #[doc(hidden)]
    fn record_position(&self) -> Position;

    #[doc(hidden)]
    fn record_operation(&self) -> Option<&'static str>;

    #[doc(hidden)]
    fn record_completion(&self) -> Completion;
}

pub(crate) mod sealed {
    pub trait Sealed {}
}

/// How far a [`RecordStream`] got.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Completion {
    /// More records may follow (also after a recoverable error, and while the stream is idle).
    Reading,
    /// The input ended cleanly after the last record: the only state that proves every record
    /// was read.
    EndOfInput,
    /// A terminal [`ReadError`] ended the reader (a limit, malformed input, a transport failure):
    /// not the end of input.
    Failed,
}

type Pending<R> = Pin<
    Box<dyn Future<Output = (R, Option<Result<<R as RecordReader>::Record, ReadError>>)> + Send>,
>;

enum State<R: RecordReader> {
    Idle(R),
    Reading(Pending<R>),
    Done(Option<R>),
}

/// An incremental reader as an owning [`futures_core::Stream`] of `Result<T, ReadError>`, for the
/// standard combinators (`map`, `try_filter`, `forward`) and for upload sources
/// (`StreamRows::new(rows.into_stream().map(..))`).
///
/// It keeps every property of the reader:
///
/// - **No read-ahead**: a record is read only when the stream is polled; no task is spawned and
///   nothing is buffered beyond the reader's own bounded state.
/// - **Cancellation-safe**: the read in progress belongs to the stream, so dropping a pending
///   `next()` future (from `StreamExt::next`, a `select!`) loses nothing; the next poll resumes it.
///   Dropping the stream drops the reader and its response.
/// - **Errors unchanged**: each [`ReadError`] is yielded as the reader returned it, with its
///   operation, position and [`is_terminal`](ReadError::is_terminal) flag. The stream does not
///   end on a recoverable error (JSON Lines `RecordErrors::Yield`); it ends after a terminal one.
///   A combinator that stops at the first `Err` (`try_collect`) has therefore not visited every
///   valid record: that is the caller's policy.
/// - **Completion is observable**: [`completion`](Self::completion) is
///   [`Completion::EndOfInput`] only after the reader reached the clean end of its input. A
///   stream that ended after a terminal error, was dropped early, or hit a limit did not.
///
/// Each read in progress is one boxed future (one allocation per record read); the reader itself
/// moves into it and back, so [`position`](Self::position) reports the position after the last
/// record yielded.
pub struct RecordStream<R: RecordReader> {
    state: State<R>,
    position: Position,
    operation: Option<&'static str>,
    completion: Completion,
}

impl<R: RecordReader> RecordStream<R> {
    pub(crate) fn new(reader: R) -> Self {
        Self {
            position: reader.record_position(),
            operation: reader.record_operation(),
            completion: reader.record_completion(),
            state: State::Idle(reader),
        }
    }

    /// Records delivered and input bytes consumed as of the last record (or error) yielded.
    pub fn position(&self) -> Position {
        self.position
    }

    /// The operation named by the reader's errors.
    pub fn operation(&self) -> Option<&'static str> {
        self.operation
    }

    /// Whether the input ended cleanly, failed, or may still yield records.
    pub fn completion(&self) -> Completion {
        self.completion
    }

    /// The reader back, when no read is in progress (`None` while a polled read is pending: drop
    /// the stream instead, or poll it to the next record first).
    pub fn into_reader(self) -> Option<R> {
        match self.state {
            State::Idle(reader) => Some(reader),
            State::Reading(_) => None,
            State::Done(reader) => reader,
        }
    }
}

// The reader is never pinned in place: it is moved into a boxed (pinned) future and back.
impl<R: RecordReader> Unpin for RecordStream<R> {}

impl<R: RecordReader> Stream for RecordStream<R> {
    type Item = Result<R::Record, ReadError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // `RecordStream` is `Unpin`: the reader is moved into a boxed future, never pinned here.
        let this = self.get_mut();
        loop {
            match std::mem::replace(&mut this.state, State::Done(None)) {
                State::Idle(mut reader) => {
                    this.state = State::Reading(Box::pin(async move {
                        let item = reader.next_record().await;
                        (reader, item)
                    }));
                }
                State::Reading(mut pending) => match pending.as_mut().poll(cx) {
                    Poll::Pending => {
                        this.state = State::Reading(pending);
                        return Poll::Pending;
                    }
                    Poll::Ready((reader, item)) => {
                        this.position = reader.record_position();
                        this.completion = reader.record_completion();
                        this.state = if item.is_none() || this.completion != Completion::Reading {
                            // The reader is fused: it yields nothing more.
                            State::Done(Some(reader))
                        } else {
                            State::Idle(reader)
                        };
                        return Poll::Ready(item);
                    }
                },
                State::Done(reader) => {
                    this.state = State::Done(reader);
                    return Poll::Ready(None);
                }
            }
        }
    }
}

impl<R: RecordReader> fmt::Debug for RecordStream<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RecordStream")
            .field("operation", &self.operation)
            .field("position", &self.position)
            .field("completion", &self.completion)
            .field("reading", &matches!(self.state, State::Reading(_)))
            .finish()
    }
}

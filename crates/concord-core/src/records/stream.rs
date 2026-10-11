use std::fmt;
#[cfg(test)]
use std::marker::PhantomData;
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::task::{Context, Poll, ready};

use bytes::Bytes;
use futures_core::Stream;

use crate::error::ApiError;
use crate::response::{
    ByteStream, ChunkEvent, ResponseBodyError, ResponseHead, StreamErrorKind, StreamSummary,
};
#[cfg(test)]
use crate::response::{ResponseDriver, ResponseRepresentation, Streaming};

use super::driver::{RecordDriver, RecordStep};
use super::error::response_error;
use super::{RecordDecoder, RecordError, RecordErrorKind};

/// A streaming accepted-response specification decoded by `D`.
///
/// Test-only equivalent of the first-party NDJSON/CSV streaming
/// representations: it selects Concord's sealed streaming mode and returns the
/// production [`ItemStream`] through [`ItemStream::decode_with`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct Records<D>(PhantomData<fn() -> D>);

/// Test alias naming the item stream produced by [`Records`].
#[cfg(test)]
pub(crate) type RecordStream<D> = ItemStream<<D as RecordDecoder>::Record>;

#[cfg(test)]
impl<D> ResponseRepresentation<ItemStream<D::Record>> for Records<D>
where
    D: RecordDecoder,
{
    type Driver = Streaming;

    const ACCEPT: Option<&'static str> = D::ACCEPT;

    fn validate(head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        D::validate(head).map_err(response_error)
    }

    fn from_body(
        body: <Self::Driver as ResponseDriver>::Body,
        head: &ResponseHead<'_>,
    ) -> Result<ItemStream<D::Record>, ResponseBodyError> {
        ItemStream::decode_with::<D>(body, head)
    }
}

/// Object-safe view of one started record driver, erasing the decoder type.
trait ErasedRecords<T>: Send {
    fn needs_input(&self) -> bool;
    fn supply_input(&mut self, chunk: Bytes) -> Result<(), RecordError>;
    fn finish_input(&mut self) -> Result<(), RecordError>;
    fn advance(&mut self) -> Result<RecordStep<T>, RecordError>;
    fn release(&mut self);
    #[cfg(test)]
    fn staging_is_empty(&self) -> bool;
}

impl<D> ErasedRecords<D::Record> for RecordDriver<D>
where
    D: RecordDecoder,
{
    fn needs_input(&self) -> bool {
        Self::needs_input(self)
    }

    fn supply_input(&mut self, chunk: Bytes) -> Result<(), RecordError> {
        Self::supply_input(self, chunk)
    }

    fn finish_input(&mut self) -> Result<(), RecordError> {
        Self::finish_input(self)
    }

    fn advance(&mut self) -> Result<RecordStep<D::Record>, RecordError> {
        Self::advance(self)
    }

    fn release(&mut self) {
        Self::release(self);
    }

    #[cfg(test)]
    fn staging_is_empty(&self) -> bool {
        Self::staging_is_empty(self)
    }
}

/// Maps one decoder failure to the structural stream error kind.
const fn stream_error_kind(kind: RecordErrorKind) -> StreamErrorKind {
    match kind {
        RecordErrorKind::InvalidFraming => StreamErrorKind::InvalidFraming,
        RecordErrorKind::Decode | RecordErrorKind::InvalidMediaType => StreamErrorKind::Decode,
        RecordErrorKind::IncompleteRecord => StreamErrorKind::IncompleteRecord,
        RecordErrorKind::LimitExceeded => StreamErrorKind::FrameLimit,
        RecordErrorKind::Body => StreamErrorKind::Body,
        RecordErrorKind::Internal => StreamErrorKind::Internal,
    }
}

/// A managed, owned lazy stream of semantic records decoded incrementally.
///
/// `ItemStream<T>` owns a managed [`ByteStream`] and one request-local record
/// decoder whose framing strategy is hidden. It shares every byte-level
/// contract of `ByteStream`: acquisition is not end of body, the finite total
/// lease and pending-read idle limit, consecutive empty-chunk guardrails, the
/// decoded byte limit, external cancellation, and no retry or replay after
/// handoff. It is not `Clone`.
///
/// # Records and framing
///
/// Each record's staged bytes are bounded by the smaller of the decoder's
/// `max_buffered_bytes` and the runtime frame limit (1 MiB by default,
/// `RuntimeConfigBuilder::stream_frame_limit`); the first excess byte is
/// rejected before it is retained and the oversized record is never yielded.
/// At most the runtime record limit (100,000 by default,
/// `stream_max_records`) is yielded; the next record is withheld with a
/// correlated error. Transport chunks are never record boundaries. The
/// decoder owns final-record policy at end of body (for example NDJSON parses
/// a final nonempty line without a newline exactly once; CSV rejects an
/// unterminated quote), and an incomplete tail is an error, never a silently
/// dropped record. A partial stream never becomes a complete success.
///
/// # Lifecycle and cancellation
///
/// [`next`](Self::next) is cancel-safe exactly like `ByteStream::next`: the
/// decoder buffer, poll state, counters and idle baseline live in this handle,
/// so dropping a pending `next` future does not terminate the stream and a
/// later call resumes it, unless the idle budget of that pending read ran out
/// in the meantime: record data or an end of body (which could otherwise
/// complete a staged final record) that becomes available only after the
/// idle limit yields one `IdleTimeout` error. Record data or an end of body
/// observed only at or after the end of the total lease likewise yields one
/// `LeaseExpired` error and never completes a staged record. A body read
/// failure, framing or
/// decoding failure, record/frame/byte limit, lease or idle expiry, or
/// external cancellation yields exactly one correlated [`ApiError`], then
/// `None`. The
/// [`summary`](Self::summary) counts full yielded records;
/// [`finish`](Self::finish) never drains and preserves `Eof` after the final
/// record. Dropping the stream before a terminal result records cancellation
/// and synchronously drops the decoder, staged bytes and native body.
///
/// # Sensitivity
///
/// Records are application data owned by the caller. Errors, `Debug` and the
/// summary never contain records, staged bytes, decoder state or parser text.
pub struct ItemStream<T> {
    body: ByteStream,
    records: Box<dyn ErasedRecords<T>>,
}

impl<T> ItemStream<T>
where
    T: Send + 'static,
{
    /// Builds a semantic item stream from one bounded core record decoder.
    ///
    /// This is the generated-code and representation integration point used
    /// by first-party record representations. The decoder sees only the
    /// managed byte stream and the sanitized semantic response head; it
    /// receives no native response or client authority. `D::start` runs once,
    /// before the first body poll.
    #[doc(hidden)]
    pub fn decode_with<D>(
        mut body: ByteStream,
        head: &ResponseHead<'_>,
    ) -> Result<Self, ResponseBodyError>
    where
        D: RecordDecoder<Record = T>,
    {
        let decoder = D::start(head).map_err(response_error)?;
        body.core.set_item_kind();
        let frame = D::max_buffered_bytes().min(body.core.limits().frame);
        Ok(Self {
            body,
            records: Box::new(RecordDriver::new(decoder, frame)),
        })
    }

    /// Groups items into non-empty batches of at most `size` items.
    #[must_use]
    pub fn batches(self, size: NonZeroUsize) -> ItemBatches<T> {
        ItemBatches {
            stream: self,
            size: size.get(),
            pending: Vec::new(),
            done: false,
        }
    }
}

impl<T> ItemStream<T> {
    /// Returns the next complete record, `None` after the final record or
    /// after the one terminal error.
    ///
    /// The returned future is cancel-safe: dropping it keeps the decoder
    /// buffer, counters and idle baseline in this handle.
    #[allow(
        clippy::should_implement_trait,
        reason = "STREAM-01 names the async inherent `next`; it is not an Iterator"
    )]
    pub fn next(&mut self) -> impl Future<Output = Option<Result<T, ApiError>>> {
        std::future::poll_fn(move |context| self.poll_item(context))
    }

    /// Returns an owned value-free snapshot of this stream's journal.
    #[must_use]
    pub fn summary(&self) -> StreamSummary {
        self.body.summary()
    }

    /// Stops consumption without draining and returns the final summary.
    ///
    /// After the final record was observed the `Eof` state is preserved, as
    /// is an earlier `Failed` or `Cancelled` state; otherwise the stream
    /// records `Application`.
    #[must_use]
    pub fn finish(mut self) -> StreamSummary {
        self.records.release();
        let Self { body, records } = self;
        drop(records);
        body.finish()
    }

    fn fail(&mut self, error: RecordError) -> Poll<Option<Result<T, ApiError>>> {
        let kind = stream_error_kind(error.kind());
        drop(error);
        self.records.release();
        Poll::Ready(Some(Err(self.body.core.terminate(kind))))
    }

    fn poll_item(&mut self, context: &mut Context<'_>) -> Poll<Option<Result<T, ApiError>>> {
        if !self.body.core.is_active() {
            return Poll::Ready(None);
        }
        self.body.core.begin_turn();
        // Lease and cancellation also bound delivery of already staged
        // records, not only reads. An outstanding pending read whose idle
        // budget ran out ends the stream here too, before the decoder is fed
        // data or the end of body that arrived after the deadline (a pending
        // read exists only while the decoder is waiting for input).
        if let Err(error) = self.body.core.precheck() {
            self.records.release();
            return Poll::Ready(Some(Err(error)));
        }
        loop {
            if self.records.needs_input() {
                match ready!(self.body.core.poll_chunk(context)) {
                    ChunkEvent::Data(chunk) => {
                        if let Err(error) = self.records.supply_input(chunk) {
                            return self.fail(error);
                        }
                    }
                    ChunkEvent::End => {
                        if let Err(error) = self.records.finish_input() {
                            return self.fail(error);
                        }
                    }
                    ChunkEvent::Error(error) => {
                        self.records.release();
                        return Poll::Ready(Some(Err(error)));
                    }
                    ChunkEvent::Closed => {
                        self.records.release();
                        return Poll::Ready(None);
                    }
                }
                continue;
            }

            match self.records.advance() {
                Ok(RecordStep::Record(record)) => {
                    let limit = self.body.core.limits().records.get();
                    if self.body.core.delivered_records() >= limit {
                        drop(record);
                        self.records.release();
                        return Poll::Ready(Some(Err(self
                            .body
                            .core
                            .terminate(StreamErrorKind::RecordLimit))));
                    }
                    self.body.core.record_item_delivered();
                    return Poll::Ready(Some(Ok(record)));
                }
                Ok(RecordStep::NeedInput) => {}
                Ok(RecordStep::Done) => {
                    self.body.core.complete_eof();
                    return Poll::Ready(None);
                }
                Err(error) => return self.fail(error),
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn staging_is_empty(&self) -> bool {
        self.records.staging_is_empty()
    }

    #[cfg(test)]
    pub(crate) fn body_is_released(&self) -> bool {
        self.body.core.source_is_released()
    }
}

impl<T> Unpin for ItemStream<T> {}

impl<T> Stream for ItemStream<T> {
    type Item = Result<T, ApiError>;

    fn poll_next(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.get_mut().poll_item(context)
    }
}

impl<T> fmt::Debug for ItemStream<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ItemStream")
            .field("body", &self.body)
            .field("records", &"<semantic stream omitted>")
            .finish()
    }
}

/// A consumer adapter yielding non-empty vectors from an [`ItemStream`].
///
/// It holds at most the requested positive batch size, yields a final
/// partial batch only after the item stream's actual end, and discards a
/// partial batch when the underlying stream fails; the one terminal error is
/// yielded once, then `None`. Dropping the adapter drops the item stream with
/// the same cancellation semantics. `Debug` never formats items.
pub struct ItemBatches<T> {
    stream: ItemStream<T>,
    size: usize,
    pending: Vec<T>,
    done: bool,
}

impl<T> Unpin for ItemBatches<T> {}

impl<T> Stream for ItemBatches<T>
where
    T: Send + 'static,
{
    type Item = Result<Vec<T>, ApiError>;

    fn poll_next(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.done {
            return Poll::Ready(None);
        }

        loop {
            match this.stream.poll_item(context) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Some(Ok(item))) => {
                    this.pending.push(item);
                    if this.pending.len() == this.size {
                        return Poll::Ready(Some(Ok(std::mem::take(&mut this.pending))));
                    }
                }
                Poll::Ready(Some(Err(error))) => {
                    this.done = true;
                    this.pending.clear();
                    return Poll::Ready(Some(Err(error)));
                }
                Poll::Ready(None) => {
                    this.done = true;
                    return if this.pending.is_empty() {
                        Poll::Ready(None)
                    } else {
                        Poll::Ready(Some(Ok(std::mem::take(&mut this.pending))))
                    };
                }
            }
        }
    }
}

impl<T> fmt::Debug for ItemBatches<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ItemBatches")
            .field("stream", &self.stream)
            .field("batch_size", &self.size)
            .field("pending_items", &self.pending.len())
            .field("done", &self.done)
            .finish()
    }
}

#[cfg(test)]
mod tests;

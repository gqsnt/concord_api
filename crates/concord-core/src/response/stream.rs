use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, ready};

use bytes::Bytes;
use futures_core::Stream;
use tokio::time::Instant;

use crate::controls::{AnyCancelled, CancellationToken};
use crate::error::{ApiError, DispatchKnowledge, ErrorCategory, FailureStage};
use crate::hooks::{
    OperationEvent, OperationObserver, StreamFinished, StreamKind, StreamOutcome, observe,
};
use crate::http::application::ApplicationByteStream;
use crate::limits::{MAX_EMPTY_RESPONSE_CHUNKS, MAX_EMPTY_RESPONSE_CHUNKS_PER_TURN};

use super::lease::{
    PendingVerdict, StreamContext, StreamCorrelation, StreamHandoff, StreamLease, StreamLimits,
};
use super::{BodyDisposition, StreamErrorKind, StreamState, StreamSummary};

/// One bounded step of core-owned byte acquisition.
pub(crate) enum ChunkEvent {
    /// One nonempty chunk passed every limit.
    Data(Bytes),
    /// The native body reached its actual end; the stream is still active so
    /// a framing layer can resolve its final record.
    End,
    /// The stream terminated with this one correlated error.
    Error(ApiError),
    /// The stream was already terminal.
    Closed,
}

struct StreamCancellation {
    tokens: Vec<CancellationToken>,
    waiter: Pin<Box<AnyCancelled>>,
}

/// Handle-owned acquisition state shared by [`ByteStream`] and `ItemStream`.
///
/// Poll state, acquired-byte accounting, the idle baseline, the owned timer
/// and the cancellation waiter all live here, never in a temporary `next`
/// future.
pub(crate) struct StreamCore {
    source: Option<ApplicationByteStream>,
    source_polled: bool,
    source_finished: bool,
    state: StreamState,
    error_kind: Option<StreamErrorKind>,
    correlation: StreamCorrelation,
    limits: StreamLimits,
    lease: StreamLease,
    cancellation: Option<StreamCancellation>,
    acquired_bytes: u64,
    delivered_records: usize,
    consecutive_empty: usize,
    /// Empty chunks handled during the current outer poll of the caller.
    turn_empty: usize,
    observer: Arc<dyn OperationObserver>,
    handoff: StreamHandoff,
    kind: StreamKind,
    units_yielded: u64,
    bytes_yielded: u64,
}

impl StreamCore {
    fn open(source: ApplicationByteStream, context: StreamContext) -> Self {
        let StreamContext {
            observer,
            correlation,
            limits,
            operation_cap,
            cancellation,
            handoff,
        } = context;
        let cancellation = (!cancellation.is_empty()).then(|| StreamCancellation {
            waiter: Box::pin(AnyCancelled::new(cancellation.clone())),
            tokens: cancellation,
        });
        Self {
            source: Some(source),
            source_polled: false,
            source_finished: false,
            state: StreamState::Active,
            error_kind: None,
            correlation,
            limits,
            lease: StreamLease::start(limits, operation_cap),
            cancellation,
            acquired_bytes: 0,
            delivered_records: 0,
            consecutive_empty: 0,
            turn_empty: 0,
            observer,
            handoff,
            kind: StreamKind::Bytes,
            units_yielded: 0,
            bytes_yielded: 0,
        }
    }

    pub(crate) fn is_active(&self) -> bool {
        self.state == StreamState::Active
    }

    #[cfg(feature = "records")]
    pub(crate) const fn limits(&self) -> StreamLimits {
        self.limits
    }

    #[cfg(feature = "records")]
    pub(crate) fn set_item_kind(&mut self) {
        self.kind = StreamKind::Items;
    }

    #[cfg(feature = "records")]
    pub(crate) const fn delivered_records(&self) -> usize {
        self.delivered_records
    }

    #[cfg(feature = "records")]
    pub(crate) fn record_item_delivered(&mut self) {
        self.delivered_records = self.delivered_records.saturating_add(1);
        self.units_yielded = self.units_yielded.saturating_add(1);
    }

    fn record_chunk_delivered(&mut self, length: usize) {
        self.units_yielded = self.units_yielded.saturating_add(1);
        self.bytes_yielded = self
            .bytes_yielded
            .saturating_add(u64::try_from(length).unwrap_or(u64::MAX));
    }

    /// Starts one outer poll of the caller. The per-poll empty-chunk budget
    /// spans every chunk read during this poll, including several reads made
    /// by a framing layer before it can yield a record.
    pub(crate) fn begin_turn(&mut self) {
        self.turn_empty = 0;
    }

    /// Terminates an active stream when an attached signal was cancelled, the
    /// total lease has elapsed, or an outstanding pending read has exhausted
    /// the idle budget (the lease takes precedence over the idle budget).
    /// Called at the start of every poll, before the source is polled again
    /// and before any data (including already staged records) is delivered;
    /// nothing is read, so nothing is acquired, once a limit has expired.
    ///
    /// An outstanding pending read keeps its idle baseline in the handle when
    /// the caller drops its `next` future; if that read's idle deadline
    /// passed in the meantime, data or an end of body that became available
    /// afterwards cannot complete it.
    pub(crate) fn precheck(&mut self) -> Result<(), ApiError> {
        if self.cancellation.as_ref().is_some_and(|cancellation| {
            cancellation
                .tokens
                .iter()
                .any(CancellationToken::is_cancelled)
        }) {
            return Err(self.terminate(StreamErrorKind::Cancelled));
        }
        if let Err(kind) = self.lease.check(Instant::now()) {
            return Err(self.terminate(kind));
        }
        Ok(())
    }

    /// Accepts an observed completion of an underlying read only within the
    /// stream's time limits, judged at `now`, the instant it was observed.
    ///
    /// [`StreamLease::check`] applies the order and boundaries of
    /// [`precheck`](Self::precheck) and of a still-pending read's owned
    /// timer: a completion at or after the total lease end is withheld as
    /// `LeaseExpired` (whether or not the read was ever pending, and also
    /// when the idle budget ran out as well); otherwise the completion of a
    /// pending read whose cumulative idle reached the limit is withheld as
    /// `IdleTimeout`. A withheld completion (data, empty chunk, end of body
    /// or read failure) terminates the stream; a withheld nonempty chunk was
    /// nevertheless read from the transport and counts as acquired. External
    /// cancellation is not judged here: a signal raised while the read was
    /// in progress ends the stream at the next poll.
    fn reject_overdue_completion(
        &mut self,
        completion: Option<&Result<Bytes, ApiError>>,
        now: Instant,
    ) -> Option<ApiError> {
        let kind = self.lease.check(now).err()?;
        if let Some(Ok(bytes)) = completion {
            let length = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
            self.acquired_bytes = self.acquired_bytes.saturating_add(length);
        }
        Some(self.terminate(kind))
    }

    /// Polls for the next nonempty chunk under every stream limit.
    pub(crate) fn poll_chunk(&mut self, context: &mut Context<'_>) -> Poll<ChunkEvent> {
        if !self.is_active() {
            return Poll::Ready(ChunkEvent::Closed);
        }
        if self.source_finished {
            return Poll::Ready(ChunkEvent::End);
        }
        if let Err(error) = self.precheck() {
            return Poll::Ready(ChunkEvent::Error(error));
        }

        loop {
            let Some(source) = self.source.as_mut() else {
                return Poll::Ready(ChunkEvent::Error(self.terminate(StreamErrorKind::Internal)));
            };
            self.source_polled = true;
            let Poll::Ready(completion) = source.as_mut().poll_next(context) else {
                if let PendingVerdict::Expired(kind) = self.lease.on_pending(context) {
                    return Poll::Ready(ChunkEvent::Error(self.terminate(kind)));
                }
                let cancelled = self.cancellation.as_mut().is_some_and(|cancellation| {
                    cancellation.waiter.as_mut().poll(context).is_ready()
                });
                if cancelled {
                    return Poll::Ready(ChunkEvent::Error(
                        self.terminate(StreamErrorKind::Cancelled),
                    ));
                }
                return Poll::Pending;
            };
            // Judged before any branch below accepts the completion or
            // clears the pending baseline (data, empty chunk, end of body or
            // read failure alike).
            let now = Instant::now();
            if let Some(error) = self.reject_overdue_completion(completion.as_ref(), now) {
                drop(completion);
                return Poll::Ready(ChunkEvent::Error(error));
            }
            match completion {
                None => {
                    self.source = None;
                    self.source_finished = true;
                    // The total lease keeps bounding delivery of records
                    // still staged by a framing layer; no read is pending.
                    self.lease.release();
                    return Poll::Ready(ChunkEvent::End);
                }
                Some(Err(error)) => {
                    let category = error.category();
                    drop(error);
                    return Poll::Ready(ChunkEvent::Error(
                        self.terminate_as(StreamErrorKind::Body, category),
                    ));
                }
                Some(Ok(bytes)) if bytes.is_empty() => {
                    self.lease.on_empty(now);
                    let Some(consecutive) = self.consecutive_empty.checked_add(1) else {
                        return Poll::Ready(ChunkEvent::Error(
                            self.terminate(StreamErrorKind::EmptyChunkLimit),
                        ));
                    };
                    if consecutive > MAX_EMPTY_RESPONSE_CHUNKS {
                        return Poll::Ready(ChunkEvent::Error(
                            self.terminate(StreamErrorKind::EmptyChunkLimit),
                        ));
                    }
                    self.consecutive_empty = consecutive;
                    self.turn_empty = self.turn_empty.saturating_add(1);
                    if self.turn_empty >= MAX_EMPTY_RESPONSE_CHUNKS_PER_TURN {
                        self.turn_empty = 0;
                        // Cooperative yield: no data was acquired, so the
                        // acquisition stays pending and the time until the
                        // next poll counts as pending idle (also when the
                        // caller drops `next` here); no timer is armed.
                        self.lease.on_yield(now);
                        context.waker().wake_by_ref();
                        return Poll::Pending;
                    }
                }
                Some(Ok(bytes)) => {
                    let length = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
                    // The chunk was read from the transport, so it is
                    // acquired even when the limit then withholds it.
                    self.acquired_bytes = self.acquired_bytes.saturating_add(length);
                    if self.acquired_bytes > self.limits.byte_limit() {
                        drop(bytes);
                        return Poll::Ready(ChunkEvent::Error(
                            self.terminate(StreamErrorKind::ByteLimit),
                        ));
                    }
                    self.consecutive_empty = 0;
                    self.lease.on_data();
                    return Poll::Ready(ChunkEvent::Data(bytes));
                }
            }
        }
    }

    /// Records the actual end of the body (or of the final record).
    pub(crate) fn complete_eof(&mut self) {
        if self.is_active() {
            self.state = StreamState::Eof;
            self.release();
            self.observe(StreamOutcome::Completed);
        }
    }

    /// Records an application stop before end of body without draining.
    fn finish_application(&mut self) {
        if self.is_active() {
            self.state = StreamState::Application;
            self.release();
            self.observe(StreamOutcome::Application);
        }
    }

    /// Records cancellation by drop before any terminal result.
    fn cancel_on_drop(&mut self) {
        if self.is_active() {
            self.state = StreamState::Cancelled;
            self.release();
            self.observe(StreamOutcome::Dropped);
        }
    }

    /// Terminates the stream with its one correlated error.
    pub(crate) fn terminate(&mut self, kind: StreamErrorKind) -> ApiError {
        self.terminate_as(kind, category_of(kind))
    }

    fn terminate_as(&mut self, kind: StreamErrorKind, category: ErrorCategory) -> ApiError {
        let cancelled = kind == StreamErrorKind::Cancelled;
        if self.is_active() {
            self.state = if cancelled {
                StreamState::Cancelled
            } else {
                StreamState::Failed
            };
            self.error_kind = Some(kind);
            self.release();
            self.observe(if cancelled {
                StreamOutcome::Cancelled
            } else {
                StreamOutcome::Failed
            });
        }
        let correlation = &self.correlation;
        ApiError::classified(
            correlation.endpoint.clone(),
            correlation.method.clone(),
            category,
        )
        .with_stage(
            FailureStage::StreamConsumption,
            DispatchKnowledge::ResponseObserved,
        )
        .with_journal(crate::error::ErrorJournal {
            operation_id: Some(correlation.operation_id),
            parent_operation_id: correlation.parent_operation_id,
            page_index: Some(correlation.page_index),
            stage: FailureStage::StreamConsumption,
            aggregate: DispatchKnowledge::ResponseObserved,
            last_attempt: Some(
                crate::error::AttemptFacts::new(correlation.attempt)
                    .settle(DispatchKnowledge::ResponseObserved)
                    .observed(correlation.status),
            ),
            last_status: Some(correlation.status),
            counts: None,
            provider: None,
        })
        .with_stream_summary(self.summary())
    }

    /// Drops the native body, the owned timer and cancellation waiters
    /// synchronously. No drain or detached cleanup is started.
    fn release(&mut self) {
        self.source = None;
        self.lease.release();
        self.cancellation = None;
    }

    fn observe(&self, outcome: StreamOutcome) {
        if !self.handoff.is_delivered() {
            // Dropped inside a failed operation before reaching the caller.
            return;
        }
        observe(
            self.observer.as_ref(),
            OperationEvent::StreamFinished(StreamFinished {
                operation_id: self.correlation.operation_id,
                parent_operation_id: self.correlation.parent_operation_id,
                endpoint: self.correlation.endpoint.as_str(),
                method: &self.correlation.method,
                kind: self.kind,
                outcome,
                units_yielded: self.units_yielded,
                bytes_yielded: (self.kind == StreamKind::Bytes).then_some(self.bytes_yielded),
            }),
        );
    }

    fn body_disposition(&self) -> BodyDisposition {
        match self.state {
            StreamState::Active => BodyDisposition::StreamOpened,
            StreamState::Eof => BodyDisposition::Complete,
            StreamState::Failed => BodyDisposition::Failed,
            StreamState::Application | StreamState::Cancelled if self.source_polled => {
                BodyDisposition::Partial
            }
            StreamState::Application | StreamState::Cancelled => BodyDisposition::DiscardedUnread,
        }
    }

    pub(crate) fn summary(&self) -> StreamSummary {
        StreamSummary {
            state: self.state,
            endpoint: self.correlation.endpoint.clone(),
            method: self.correlation.method.clone(),
            operation_id: self.correlation.operation_id,
            attempt: self.correlation.attempt,
            status: self.correlation.status,
            acquired_bytes: self.acquired_bytes,
            delivered_records: self.delivered_records,
            body_disposition: self.body_disposition(),
            error_kind: self.error_kind,
        }
    }

    #[cfg(all(test, feature = "records"))]
    pub(crate) fn source_is_released(&self) -> bool {
        self.source.is_none()
    }

    #[cfg(test)]
    pub(crate) fn lease_for_test(&self) -> &StreamLease {
        &self.lease
    }
}

const fn category_of(kind: StreamErrorKind) -> ErrorCategory {
    match kind {
        StreamErrorKind::Body
        | StreamErrorKind::ByteLimit
        | StreamErrorKind::EmptyChunkLimit
        | StreamErrorKind::RecordLimit
        | StreamErrorKind::FrameLimit => ErrorCategory::ResponseBody,
        StreamErrorKind::LeaseExpired | StreamErrorKind::IdleTimeout => ErrorCategory::Timeout,
        StreamErrorKind::InvalidFraming
        | StreamErrorKind::Decode
        | StreamErrorKind::IncompleteRecord => ErrorCategory::ResponseDecoding,
        StreamErrorKind::Cancelled => ErrorCategory::Cancelled,
        StreamErrorKind::Internal => ErrorCategory::Internal,
    }
}

/// A managed, owned, bounded accepted response body consumed lazily.
///
/// # Acquisition is not end of body
///
/// An endpoint await that returns a `ByteStream` has only validated the
/// accepted head and opened the stream: the operation's completion reports
/// `StreamOpened` and [`ApiResponse::body_disposition`](crate::contracts::ApiResponse::body_disposition)
/// reports [`BodyDisposition::StreamOpened`], never a completed download.
/// Later failures belong to the stream, not to the already fixed operation
/// result, so an operation is never reported as fully successful and then
/// failed for the same body.
///
/// # Ownership and lifecycle
///
/// The handle exclusively owns the native body; it is not `Clone`. Consume it
/// with [`next`](Self::next) (or the [`Stream`] implementation), inspect it
/// with [`summary`](Self::summary) and stop early with
/// [`finish`](Self::finish), which never drains the body. The terminal
/// [`StreamState`] is `Eof` only after the caller observed the actual end of
/// the body. A terminal failure yields exactly one error, then `None`; the
/// failure is retained in the summary. Dropping the handle before a terminal
/// result records cancellation and synchronously drops the native body.
///
/// # Cancellation of `next`
///
/// Dropping an individual pending `next` future does **not** terminate the
/// stream. Poll state, acquired-byte accounting and the idle baseline live in
/// the handle; a later `next` resumes the same acquisition and never resends
/// the request. This deliberately differs from pagination, where dropping a
/// started page fetch terminates the traversal. External cancellation signals
/// attached to the originating request (`cancel_with`) cancel the entire
/// stream: the next poll yields one `Cancelled` error and releases the body.
///
/// # Finite lease
///
/// After acquisition the stream holds a finite lease (5 minutes total and 30
/// seconds cumulative idle by default, configurable through
/// `RuntimeConfigBuilder::stream_lifetime` / `stream_idle_timeout` and
/// narrowed by `CallOptions::cap_stream_lifetime` / `cap_stream_idle`). The
/// total lease includes consumer pauses between polls; idle counts only time
/// while an underlying read is pending, nonempty data resets it and empty
/// chunks do not. A pending read keeps its idle baseline when its `next`
/// future is dropped, and once its idle budget is spent that read can no
/// longer complete: data or an end of body that becomes available only
/// afterwards yields one `IdleTimeout` error, not the data and not a clean
/// end. Likewise a read observed to complete only at or after the end of the
/// total lease yields one `LeaseExpired` error instead of its data or a clean
/// end; when both limits have run out, `LeaseExpired` takes precedence. Empty
/// chunks acquire no data, so the acquisition stays pending across the
/// cooperative yield described below: the time until the next poll counts as
/// idle, also when `next` is dropped at that yield. A whole-operation timeout
/// cap (`CallOptions::cap_timeout`)
/// started at the first request poll also bounds the lease; without it, the
/// request timeout ends at acquisition. Expiry is observed on the next poll
/// (or while a read is pending, through one handle-owned timer); a stream that
/// is never polled again releases its native body on drop. No detached task is
/// started and there is no implicit forever mode or reconnection.
///
/// # Limits and framing
///
/// Every yielded chunk is nonempty and passed the cumulative decoded
/// (post-decompression) byte limit; the first excess chunk is withheld.
/// Empty transport chunks are suppressed, at most
/// [`MAX_EMPTY_RESPONSE_CHUNKS`](crate::contracts::MAX_EMPTY_RESPONSE_CHUNKS)
/// consecutive empties are accepted, and at most 32 are handled per poll
/// before yielding to the scheduler. No policy retry, authentication recovery
/// or native replay happens after handoff: a reconnecting application must
/// start a new operation.
///
/// # Errors and sensitivity
///
/// Every stream error carries `FailureStage::StreamConsumption`,
/// `DispatchKnowledge::ResponseObserved`, the original endpoint, operation,
/// attempt and status, and a [`StreamSummary`]. Errors, `Debug` and the
/// summary never contain body bytes, headers or URLs.
///
/// # Runtime
///
/// The stream must be polled inside a Tokio runtime with time enabled, as is
/// every Concord operation.
pub struct ByteStream {
    pub(crate) core: StreamCore,
}

impl ByteStream {
    pub(crate) fn open(source: ApplicationByteStream, context: StreamContext) -> Self {
        Self {
            core: StreamCore::open(source, context),
        }
    }

    /// Returns the next nonempty chunk, `None` after the end of the body or
    /// after the one terminal error.
    ///
    /// The returned future is cancel-safe: dropping it leaves all acquisition
    /// state, counters and the idle baseline in this handle, and a later call
    /// resumes the same acquisition.
    #[allow(
        clippy::should_implement_trait,
        reason = "STREAM-01 names the async inherent `next`; it is not an Iterator"
    )]
    pub fn next(&mut self) -> impl Future<Output = Option<Result<Bytes, ApiError>>> {
        std::future::poll_fn(move |context| self.poll_bytes(context))
    }

    /// Returns an owned value-free snapshot of this stream's journal.
    #[must_use]
    pub fn summary(&self) -> StreamSummary {
        self.core.summary()
    }

    /// Stops consumption without draining and returns the final summary.
    ///
    /// After the actual end of the body the `Eof` state is preserved, as is
    /// an earlier `Failed` or `Cancelled` state; otherwise the stream records
    /// `Application` and drops the native body unread.
    #[must_use]
    pub fn finish(mut self) -> StreamSummary {
        self.core.finish_application();
        self.core.summary()
    }

    fn poll_bytes(&mut self, context: &mut Context<'_>) -> Poll<Option<Result<Bytes, ApiError>>> {
        self.core.begin_turn();
        match ready!(self.core.poll_chunk(context)) {
            ChunkEvent::Data(bytes) => {
                self.core.record_chunk_delivered(bytes.len());
                Poll::Ready(Some(Ok(bytes)))
            }
            ChunkEvent::End => {
                self.core.complete_eof();
                Poll::Ready(None)
            }
            ChunkEvent::Error(error) => Poll::Ready(Some(Err(error))),
            ChunkEvent::Closed => Poll::Ready(None),
        }
    }
}

impl Stream for ByteStream {
    type Item = Result<Bytes, ApiError>;

    fn poll_next(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.get_mut().poll_bytes(context)
    }
}

impl Drop for ByteStream {
    fn drop(&mut self) {
        self.core.cancel_on_drop();
    }
}

impl fmt::Debug for ByteStream {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ByteStream")
            .field("state", &self.core.state)
            .field("acquired_bytes", &self.core.acquired_bytes)
            .field("body", &"<response stream omitted>")
            .finish()
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::num::NonZeroUsize;
    use std::sync::Arc;
    use std::time::Duration;

    use super::*;
    use crate::endpoint::EndpointId;
    use crate::hooks::AttemptInfo;
    use crate::operation::OperationId;

    pub(crate) fn limits(bytes: usize) -> StreamLimits {
        StreamLimits {
            lifetime: Duration::from_secs(300),
            idle: Duration::from_secs(30),
            bytes: NonZeroUsize::new(bytes).expect("positive test byte limit"),
            records: NonZeroUsize::new(100_000).expect("positive test record limit"),
            frame: NonZeroUsize::new(1_048_576).expect("positive test frame limit"),
        }
    }

    pub(crate) fn context(
        endpoint: &'static str,
        limits: StreamLimits,
        observer: Arc<dyn OperationObserver>,
        cancellation: Vec<CancellationToken>,
        operation_cap: Option<tokio::time::Instant>,
    ) -> StreamContext {
        StreamContext {
            observer,
            correlation: StreamCorrelation {
                endpoint: EndpointId::from_static(endpoint),
                method: http::Method::GET,
                operation_id: OperationId::allocate().expect("test operation ID"),
                parent_operation_id: None,
                attempt: AttemptInfo::INITIAL,
                page_index: 0,
                status: http::StatusCode::OK,
            },
            limits,
            operation_cap,
            cancellation,
            handoff: StreamHandoff::delivered(),
        }
    }

    pub(crate) fn byte_stream(
        source: ApplicationByteStream,
        endpoint: &'static str,
        limits: StreamLimits,
    ) -> ByteStream {
        ByteStream::open(
            source,
            context(
                endpoint,
                limits,
                crate::hooks::default_operation_observer(),
                Vec::new(),
                None,
            ),
        )
    }

    /// Counts native polls and records the synchronous drop of a source.
    #[derive(Clone, Default)]
    pub(crate) struct SourceProbe {
        pub(crate) polls: Arc<std::sync::atomic::AtomicUsize>,
        pub(crate) dropped: Arc<std::sync::atomic::AtomicBool>,
    }

    impl SourceProbe {
        pub(crate) fn polls(&self) -> usize {
            self.polls.load(std::sync::atomic::Ordering::SeqCst)
        }

        pub(crate) fn dropped(&self) -> bool {
            self.dropped.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    /// An immediately ready scripted source.
    pub(crate) struct ReadySource {
        chunks: std::collections::VecDeque<Result<Bytes, ApiError>>,
        probe: SourceProbe,
    }

    impl Stream for ReadySource {
        type Item = Result<Bytes, ApiError>;

        fn poll_next(
            mut self: Pin<&mut Self>,
            _context: &mut Context<'_>,
        ) -> Poll<Option<Self::Item>> {
            self.probe
                .polls
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Poll::Ready(self.chunks.pop_front())
        }
    }

    impl Drop for ReadySource {
        fn drop(&mut self) {
            self.probe
                .dropped
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }

    pub(crate) fn ready_source(
        chunks: impl IntoIterator<Item = Result<Bytes, ApiError>>,
        probe: &SourceProbe,
    ) -> ApplicationByteStream {
        Box::pin(ReadySource {
            chunks: chunks.into_iter().collect(),
            probe: probe.clone(),
        })
    }

    /// A source fed by the test through a channel; pending while empty and
    /// ended when the sender is dropped.
    pub(crate) struct ChannelSource {
        receiver: tokio::sync::mpsc::UnboundedReceiver<Result<Bytes, ApiError>>,
        probe: SourceProbe,
    }

    impl Stream for ChannelSource {
        type Item = Result<Bytes, ApiError>;

        fn poll_next(
            mut self: Pin<&mut Self>,
            context: &mut Context<'_>,
        ) -> Poll<Option<Self::Item>> {
            self.probe
                .polls
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.receiver.poll_recv(context)
        }
    }

    impl Drop for ChannelSource {
        fn drop(&mut self) {
            self.probe
                .dropped
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }

    pub(crate) fn channel_source(
        probe: &SourceProbe,
    ) -> (
        tokio::sync::mpsc::UnboundedSender<Result<Bytes, ApiError>>,
        ApplicationByteStream,
    ) {
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        (
            sender,
            Box::pin(ChannelSource {
                receiver,
                probe: probe.clone(),
            }),
        )
    }

    /// One scripted native poll of a [`TimedSource`]: `during` of paused Tokio
    /// time passes inside the poll, then the poll returns `outcome`, so a
    /// ready outcome models a read that completes within that one poll.
    pub(crate) struct TimedPoll {
        during: Duration,
        outcome: Poll<Option<Result<Bytes, ApiError>>>,
    }

    impl TimedPoll {
        /// The read is pending (no waker is registered: only the stream's
        /// own timer or the test drives the next poll).
        pub(crate) const fn pending() -> Self {
            Self {
                during: Duration::ZERO,
                outcome: Poll::Pending,
            }
        }

        /// The read completes `during` into the poll with `completion`
        /// (`None` is the end of the body).
        pub(crate) const fn ready_after(
            during: Duration,
            completion: Option<Result<Bytes, ApiError>>,
        ) -> Self {
            Self {
                during,
                outcome: Poll::Ready(completion),
            }
        }
    }

    /// A scripted source on the paused Tokio clock whose reads take time
    /// inside a poll; pending once the script is exhausted.
    pub(crate) struct TimedSource {
        polls: std::collections::VecDeque<TimedPoll>,
        probe: SourceProbe,
    }

    impl Stream for TimedSource {
        type Item = Result<Bytes, ApiError>;

        fn poll_next(
            mut self: Pin<&mut Self>,
            _context: &mut Context<'_>,
        ) -> Poll<Option<Self::Item>> {
            self.probe
                .polls
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let Some(step) = self.polls.pop_front() else {
                return Poll::Pending;
            };
            if !step.during.is_zero() {
                let_paused_time_pass(step.during);
            }
            step.outcome
        }
    }

    impl Drop for TimedSource {
        fn drop(&mut self) {
            self.probe
                .dropped
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }

    pub(crate) fn timed_source(
        polls: impl IntoIterator<Item = TimedPoll>,
        probe: &SourceProbe,
    ) -> ApplicationByteStream {
        Box::pin(TimedSource {
            polls: polls.into_iter().collect(),
            probe: probe.clone(),
        })
    }

    /// Lets `duration` of paused Tokio time pass synchronously, from inside a
    /// native poll.
    ///
    /// `tokio::time::advance` moves the paused clock when it is first polled,
    /// before it yields once; polling it once with a no-op waker and dropping
    /// it therefore models time spent inside one poll. The clock is checked
    /// afterwards, so a different Tokio behavior fails loudly instead of
    /// silently weakening a test.
    pub(crate) fn let_paused_time_pass(duration: Duration) {
        let before = tokio::time::Instant::now();
        let mut advance = Box::pin(tokio::time::advance(duration));
        let mut context = Context::from_waker(std::task::Waker::noop());
        let _ = advance.as_mut().poll(&mut context);
        drop(advance);
        assert_eq!(
            tokio::time::Instant::now().saturating_duration_since(before),
            duration,
            "paused Tokio time passed inside the native poll"
        );
    }

    /// Records every stream lifecycle event.
    #[derive(Default)]
    pub(crate) struct StreamEvents {
        pub(crate) outcomes: std::sync::Mutex<Vec<(StreamKind, StreamOutcome, u64)>>,
    }

    impl StreamEvents {
        pub(crate) fn outcomes(&self) -> Vec<(StreamKind, StreamOutcome, u64)> {
            self.outcomes.lock().expect("stream events").clone()
        }
    }

    impl OperationObserver for StreamEvents {
        fn observe(&self, event: OperationEvent<'_>) {
            if let OperationEvent::StreamFinished(finished) = event {
                self.outcomes.lock().expect("stream events").push((
                    finished.kind,
                    finished.outcome,
                    finished.units_yielded,
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests;

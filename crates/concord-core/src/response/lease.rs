//! Finite post-acquisition stream leases, idle accounting and handoff context.
//!
//! Every value here lives inside the owned stream handle, never inside a
//! temporary `next` future, so abandoning a pending read cannot reset any
//! counter or baseline. No detached timer task is created: the only timer is
//! one [`tokio::time::Sleep`] registration owned by the handle, armed while an
//! underlying read is pending and dropped with the handle.

use std::future::Future;
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::Context;
use std::time::Duration;

use http::{Method, StatusCode};
use tokio::time::{Instant, Sleep};

use crate::controls::{CallOptions, CancellationToken};
use crate::endpoint::EndpointId;
use crate::hooks::{AttemptInfo, OperationObserver};
use crate::operation::{OperationDeadline, OperationId};

use super::StreamErrorKind;

/// Effective finite ceilings of one managed stream.
///
/// Built from the trusted runtime configuration and narrowed (never widened)
/// by the request's call caps.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct StreamLimits {
    /// Total elapsed lease after response-head acquisition.
    pub(crate) lifetime: Duration,
    /// Cumulative idle while an underlying read is pending.
    pub(crate) idle: Duration,
    /// Cumulative decoded nonempty body bytes.
    pub(crate) bytes: NonZeroUsize,
    /// Records yielded by an item stream.
    pub(crate) records: NonZeroUsize,
    /// Staged bytes of one framed record.
    pub(crate) frame: NonZeroUsize,
}

impl StreamLimits {
    /// Takes the componentwise minimum with the stream-relevant call caps.
    pub(crate) fn narrowed(self, caps: &CallOptions) -> Self {
        Self {
            lifetime: caps
                .stream_lifetime_cap()
                .map_or(self.lifetime, |cap| cap.min(self.lifetime)),
            idle: caps
                .stream_idle_cap()
                .map_or(self.idle, |cap| cap.min(self.idle)),
            bytes: caps
                .response_byte_cap()
                .map_or(self.bytes, |cap| cap.min(self.bytes)),
            records: self.records,
            frame: self.frame,
        }
    }

    pub(crate) fn byte_limit(self) -> u64 {
        u64::try_from(self.bytes.get()).unwrap_or(u64::MAX)
    }
}

/// Original request correlation carried by every later stream error.
#[derive(Clone, Debug)]
pub(crate) struct StreamCorrelation {
    pub(crate) endpoint: EndpointId,
    pub(crate) method: Method,
    pub(crate) operation_id: OperationId,
    pub(crate) parent_operation_id: Option<OperationId>,
    pub(crate) attempt: AttemptInfo,
    pub(crate) page_index: u32,
    pub(crate) status: StatusCode,
}

/// Whether a stream opened by an operation was actually delivered to the
/// caller.
///
/// A stream dropped inside the executor (for example because the
/// representation's decoder failed to start, or the conversion deadline
/// expired) belongs to a failed operation and emits no stream lifecycle
/// event; only a delivered stream (whose operation completed as
/// `StreamOpened`) reports exactly one `StreamFinished`.
#[derive(Clone, Debug)]
pub(crate) struct StreamHandoff(Arc<AtomicBool>);

impl StreamHandoff {
    pub(crate) fn pending() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    #[cfg(test)]
    pub(crate) fn delivered() -> Self {
        Self(Arc::new(AtomicBool::new(true)))
    }

    /// Marks the stream as handed to the caller. Called by the executor once
    /// the operation result containing the stream is fixed.
    pub(crate) fn mark_delivered(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub(crate) fn is_delivered(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Everything the executor hands to a managed stream at acquisition.
///
/// Ownership of continuation passes to the stream here; the executor performs
/// no policy retry, authentication recovery or native replay afterwards.
pub(crate) struct StreamContext {
    pub(crate) observer: Arc<dyn OperationObserver>,
    pub(crate) correlation: StreamCorrelation,
    pub(crate) limits: StreamLimits,
    /// Absolute whole-operation timeout-cap instant in the operation clock
    /// (first request poll plus `CallOptions::cap_timeout`), when present.
    pub(crate) operation_cap: Option<Instant>,
    /// External cancellation signals captured from the request controls.
    pub(crate) cancellation: Vec<CancellationToken>,
    /// Delivery flag shared with the executor.
    pub(crate) handoff: StreamHandoff,
}

impl StreamContext {
    /// Builds the handoff context for the accepted response of one attempt.
    #[allow(
        clippy::too_many_arguments,
        reason = "the handoff carries the exact original correlation facts"
    )]
    pub(crate) fn for_accepted_response(
        observer: Arc<dyn OperationObserver>,
        head: &crate::request::PreparedHead,
        runtime_limits: StreamLimits,
        deadline: OperationDeadline,
        operation_id: OperationId,
        parent_operation_id: Option<OperationId>,
        attempt: AttemptInfo,
        page_index: u32,
        status: StatusCode,
    ) -> Self {
        let caps = head.call_caps();
        Self {
            observer,
            correlation: StreamCorrelation {
                endpoint: head.endpoint_id().clone(),
                method: head.method().clone(),
                operation_id,
                parent_operation_id,
                attempt,
                page_index,
                status,
            },
            limits: runtime_limits.narrowed(caps),
            // An unrepresentable cap instant fails closed as an already
            // elapsed cap rather than becoming an unbounded lease.
            operation_cap: caps.timeout_cap().map(|cap| {
                deadline
                    .started()
                    .checked_add(cap)
                    .unwrap_or(deadline.started())
            }),
            cancellation: head.call_cancellation().to_vec(),
            handoff: StreamHandoff::pending(),
        }
    }
}

/// Outcome of evaluating the lease while an underlying read is pending.
pub(crate) enum PendingVerdict {
    /// Keep waiting; the timer and wakers are registered.
    Wait,
    /// The lease or idle budget is exhausted.
    Expired(StreamErrorKind),
}

/// Total-lease and pending-read idle accounting owned by the stream handle.
pub(crate) struct StreamLease {
    /// Absolute end of the total lease in the stream (Tokio) clock.
    expires: Instant,
    idle_limit: Duration,
    /// Idle accumulated since the last nonempty data, excluding the current
    /// pending span.
    idle_used: Duration,
    /// Start of the current pending underlying read.
    pending_since: Option<Instant>,
    timer: Option<Pin<Box<Sleep>>>,
    timer_deadline: Option<Instant>,
}

impl StreamLease {
    /// Starts the lease at acquisition.
    ///
    /// The total lease ends at `min(opened + lifetime, operation cap)`. The
    /// operation cap is converted from the operation clock into the stream
    /// clock by its remaining duration, measured now.
    pub(crate) fn start(limits: StreamLimits, operation_cap: Option<Instant>) -> Self {
        let opened = Instant::now();
        // An unrepresentable lease end fails closed at the first poll.
        let mut expires = opened.checked_add(limits.lifetime).unwrap_or(opened);
        if let Some(cap) = operation_cap {
            let remaining = cap.saturating_duration_since(OperationDeadline::now());
            let cap_end = opened.checked_add(remaining).unwrap_or(opened);
            expires = expires.min(cap_end);
        }
        Self {
            expires,
            idle_limit: limits.idle,
            idle_used: Duration::ZERO,
            pending_since: None,
            timer: None,
            timer_deadline: None,
        }
    }

    /// Checks the total lease; consumer pauses between polls count.
    fn check_total(&self, now: Instant) -> Result<(), StreamErrorKind> {
        if now >= self.expires {
            Err(StreamErrorKind::LeaseExpired)
        } else {
            Ok(())
        }
    }

    fn idle_total(&self, now: Instant) -> Duration {
        let current = self
            .pending_since
            .map_or(Duration::ZERO, |since| now.saturating_duration_since(since));
        self.idle_used.saturating_add(current)
    }

    /// Judges the stream's time limits at `now`: the total lease first, then
    /// the idle budget of an outstanding pending read.
    ///
    /// The stream evaluates this one predicate at the start of every poll
    /// (before the source is polled again) and at the instant a completion
    /// of an underlying read is observed, before
    /// [`on_data`](Self::on_data), [`on_empty`](Self::on_empty) or
    /// [`release`](Self::release) accepts that completion and clears its
    /// baseline. Order and boundaries are those of
    /// [`on_pending`](Self::on_pending) and of the owned timer: once the
    /// lease has elapsed (`now` at or after its end) the verdict is
    /// `LeaseExpired`, even when the idle budget ran out as well and
    /// whichever of the two ran out first; idle equal to the limit has
    /// expired.
    ///
    /// The total lease bounds the stream's lifetime, so a read observed to
    /// complete at or after the lease end cannot complete, whether or not it
    /// was ever pending. The idle deadline of a pending read belongs to the
    /// handle, not to the temporary `next` future: once it has passed, data,
    /// an empty chunk, the end of the body or a read failure that becomes
    /// observable only later cannot complete that read either, whether or not
    /// the caller kept polling.
    ///
    /// Without an outstanding pending read no idle is charged, so consumer
    /// time after a delivered read never counts.
    pub(crate) fn check(&self, now: Instant) -> Result<(), StreamErrorKind> {
        self.check_total(now)?;
        if self.pending_since.is_some() && self.idle_total(now) >= self.idle_limit {
            Err(StreamErrorKind::IdleTimeout)
        } else {
            Ok(())
        }
    }

    /// Records that the underlying read is pending and arms the one owned
    /// timer at the earlier of lease end and idle exhaustion.
    ///
    /// The pending baseline is set only once per outstanding read, so a
    /// dropped `next` future followed by another poll keeps charging the same
    /// baseline.
    pub(crate) fn on_pending(&mut self, context: &mut Context<'_>) -> PendingVerdict {
        let now = Instant::now();
        if self.check_total(now).is_err() {
            return PendingVerdict::Expired(StreamErrorKind::LeaseExpired);
        }
        let since = *self.pending_since.get_or_insert(now);
        if self.idle_total(now) >= self.idle_limit {
            return PendingVerdict::Expired(StreamErrorKind::IdleTimeout);
        }
        let idle_remaining = self.idle_limit.saturating_sub(self.idle_used);
        let idle_end = since.checked_add(idle_remaining).unwrap_or(since);
        let deadline = idle_end.min(self.expires);
        let timer = match &mut self.timer {
            Some(timer) => {
                if self.timer_deadline != Some(deadline) {
                    timer.as_mut().reset(deadline);
                }
                timer
            }
            None => self
                .timer
                .insert(Box::pin(tokio::time::sleep_until(deadline))),
        };
        self.timer_deadline = Some(deadline);
        if timer.as_mut().poll(context).is_pending() {
            return PendingVerdict::Wait;
        }
        // The owned timer fires no earlier than its deadline; classify the
        // expiry by the deadline it was armed for instead of re-reading a
        // clock that may round differently, so a fired timer never spins.
        if deadline >= self.expires {
            PendingVerdict::Expired(StreamErrorKind::LeaseExpired)
        } else {
            PendingVerdict::Expired(StreamErrorKind::IdleTimeout)
        }
    }

    /// An empty chunk completes the pending read without resetting idle: the
    /// span of that read up to `now` is added to the cumulative idle.
    ///
    /// Called only after [`check`](Self::check) accepted the completion at
    /// `now`.
    pub(crate) fn on_empty(&mut self, now: Instant) {
        if let Some(since) = self.pending_since.take() {
            self.idle_used = self
                .idle_used
                .saturating_add(now.saturating_duration_since(since));
        }
    }

    /// Keeps the data acquisition pending across the cooperative yield after
    /// a run of empty chunks.
    ///
    /// Empty chunks completed reads but acquired no data, so the acquisition
    /// is still pending when the stream yields to the scheduler: the pending
    /// baseline restarts at `now` (the spans of the empty chunks are already
    /// in the cumulative idle, which they never reset). Time until the next
    /// poll therefore counts as pending idle, also when the caller drops its
    /// `next` future at the yield, exactly as at a pending read. No timer is
    /// armed: the yield wakes the task at once and the next poll evaluates
    /// [`check`](Self::check) first.
    pub(crate) fn on_yield(&mut self, now: Instant) {
        self.pending_since.get_or_insert(now);
    }

    /// Nonempty data resets the idle budget and disarms the timer, so
    /// application processing after a delivered item is never charged and
    /// no stale timer wakes the caller.
    ///
    /// Called only after [`check`](Self::check) accepted the completion, so an
    /// expired read is never reset here.
    pub(crate) fn on_data(&mut self) {
        self.pending_since = None;
        self.idle_used = Duration::ZERO;
        self.timer = None;
        self.timer_deadline = None;
    }

    /// Releases the owned timer registration and pending baseline at a
    /// terminal state, or once the body has ended and no read can be pending
    /// (an end of body is accepted only after [`check`](Self::check)). The
    /// total lease end is kept for later delivery checks.
    pub(crate) fn release(&mut self) {
        self.timer = None;
        self.timer_deadline = None;
        self.pending_since = None;
    }

    #[cfg(test)]
    pub(crate) fn idle_charged_for_test(&self) -> Duration {
        self.idle_total(Instant::now())
    }

    #[cfg(test)]
    pub(crate) fn has_timer_for_test(&self) -> bool {
        self.timer.is_some()
    }
}

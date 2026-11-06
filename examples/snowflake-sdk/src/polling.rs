//! Tokio-clock polling with finite attempts, a deadline and an explicit local stop signal.
use crate::{
    Error, FirstResult, Query, Reply, RequestId, ServerError, ServerMessage, StatementState,
    Submission,
};
use std::{
    num::NonZeroUsize,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{
    sync::Notify,
    time::{Instant, sleep, timeout_at},
};

/// Local cancellation only. This never sends a remote cancel POST.
#[derive(Clone, Default, Debug)]
pub struct LocalStop(Arc<StopState>);
#[derive(Default, Debug)]
struct StopState {
    stopped: AtomicBool,
    notify: Notify,
}
impl LocalStop {
    /// Stops local polling; idempotent and shared by all clones.
    pub fn stop(&self) {
        self.0.stopped.store(true, Ordering::SeqCst);
        self.0.notify.notify_waiters();
    }
    /// Whether local work has been stopped.
    pub fn is_stopped(&self) -> bool {
        self.0.stopped.load(Ordering::SeqCst)
    }
    async fn stopped(&self) {
        loop {
            let notified = self.0.notify.notified();
            // Enable before checking the flag so a concurrent stop cannot be missed.
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.is_stopped() {
                return;
            }
            notified.await;
        }
    }
}

/// Finite polling policy; all timing uses Tokio time and supports a paused virtual clock.
#[derive(Clone, Copy, Debug)]
pub struct PollPolicy {
    /// Maximum status GETs after the initial submission.
    pub max_attempts: NonZeroUsize,
    /// Delay between GETs; the first status check is immediate.
    pub interval: Duration,
    /// Maximum elapsed polling time including waiting for HTTP headers/bodies.
    pub max_elapsed: Duration,
}
impl Default for PollPolicy {
    fn default() -> Self {
        Self {
            max_attempts: NonZeroUsize::new(60).expect("nonzero"),
            interval: Duration::from_secs(1),
            max_elapsed: Duration::from_secs(120),
        }
    }
}

/// A bounded local end does not determine the remote terminal state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PollEnd {
    /// All permitted status requests were used.
    Attempts,
    /// The elapsed deadline was reached, possibly with an HTTP request in flight.
    Deadline,
    /// The caller stopped local work; the server may still be executing.
    LocalStop,
}

/// Observed terminal result or a resumable local bound.
#[derive(Debug)]
pub enum PollOutcome {
    /// Query completed; this owns the first partition and its discovery data.
    Complete(Box<FirstResult>),
    /// A correlated HTTP 408/422 QueryStatus/QueryFailureStatus. Inspect is_cancelled(); other
    /// failures remain failures. A cancel acknowledgement alone does not create this outcome.
    Terminal(Box<ServerError>),
    /// Remote outcome is unresolved. The query remains usable for status or explicit cancel.
    Unresolved {
        /// Why local polling ended.
        reason: PollEnd,
        /// Number of status GETs started, including an interrupted in-flight request.
        attempts: usize,
        /// Most recent pending observation, if any.
        last_pending: Option<Box<Reply<ServerMessage>>>,
    },
}
impl Submission {
    /// Reuses immediate completion; otherwise polls the already-submitted query.
    pub async fn wait(self, policy: PollPolicy, stop: &LocalStop) -> Result<PollOutcome, Error> {
        match self.state {
            StatementState::Complete(first) => Ok(PollOutcome::Complete(Box::new(first))),
            StatementState::Pending(last) => self.query.poll_from(policy, stop, Some(last)).await,
        }
    }
}
impl Query {
    /// Observes status without resubmitting. To reuse an initial result use Submission::wait.
    pub async fn poll(&self, policy: PollPolicy, stop: &LocalStop) -> Result<PollOutcome, Error> {
        self.poll_from(policy, stop, None).await
    }
    async fn poll_from(
        &self,
        policy: PollPolicy,
        stop: &LocalStop,
        mut last: Option<Reply<ServerMessage>>,
    ) -> Result<PollOutcome, Error> {
        if policy.interval.is_zero() || policy.max_elapsed.is_zero() {
            return Err(Error::Config(
                "poll interval and elapsed limit must be positive",
            ));
        }
        let deadline = Instant::now()
            .checked_add(policy.max_elapsed)
            .ok_or(Error::Config("poll duration too large"))?;
        let mut attempts = 0;
        let reason = loop {
            if stop.is_stopped() {
                break PollEnd::LocalStop;
            }
            if Instant::now() >= deadline {
                break PollEnd::Deadline;
            }
            if attempts == policy.max_attempts.get() {
                break PollEnd::Attempts;
            }
            if attempts > 0 {
                tokio::select! {
                    biased;
                    _ = stop.stopped() => break PollEnd::LocalStop,
                    wait = timeout_at(deadline, sleep(policy.interval)) => if wait.is_err() { break PollEnd::Deadline; },
                }
                if Instant::now() >= deadline {
                    break PollEnd::Deadline;
                }
            }
            attempts += 1;
            let result = tokio::select! {
                biased;
                _ = stop.stopped() => break PollEnd::LocalStop,
                result = timeout_at(deadline, self.status()) => match result {
                    Ok(result) => result,
                    Err(_) => break PollEnd::Deadline,
                },
            };
            match result {
                Ok(StatementState::Complete(first)) => {
                    return Ok(PollOutcome::Complete(Box::new(first)));
                }
                Ok(StatementState::Pending(pending)) => last = Some(pending),
                Err(Error::Server(e))
                    if [408, 422].contains(&e.meta.status().as_u16())
                        && e.document
                            .as_ref()
                            .is_some_and(|d| d.statement_handle.as_ref() == Some(&self.handle)) =>
                {
                    return Ok(PollOutcome::Terminal(e));
                }
                Err(e) => return Err(e),
            }
        };
        Ok(PollOutcome::Unresolved {
            reason,
            attempts,
            last_pending: last.map(Box::new),
        })
    }
    /// Sends a distinct remote cancel POST, then polls even if cancellation was rejected.
    /// A completion race is reported as Complete; terminal cancellation requires observation.
    /// The local poll deadline starts after the bounded cancel HTTP request returns.
    pub async fn cancel_and_wait(
        &self,
        request_id: RequestId,
        policy: PollPolicy,
        stop: &LocalStop,
    ) -> CancelObservation {
        let acknowledgement = self.cancel(request_id).await;
        let outcome = self.poll(policy, stop).await;
        CancelObservation {
            acknowledgement,
            outcome,
        }
    }
}

/// Both sides of a cancellation race, including errors or unresolved observation.
#[derive(Debug)]
pub struct CancelObservation {
    /// What the server answered to the explicit cancellation POST.
    pub acknowledgement: Result<Reply<ServerMessage>, Error>,
    /// What subsequent polling actually observed (may be successful completion).
    pub outcome: Result<PollOutcome, Error>,
}

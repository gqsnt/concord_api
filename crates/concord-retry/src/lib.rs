//! Bounded repetition of application attempts, independent of a runtime or HTTP client.
//!
//! [`Retry`] separates semantic [`Replay`] permission from the SDK's [`Decision`]. A factory
//! promises to perform exactly one attempt and recreate its inputs; `Clone` alone grants no
//! permission. The optional `concord` adapter checks byte replayability independently.
//! Credentials, rate admission, provider schemas, body decoding and stream resumption belong
//! to the caller. Retry ends at the final response head, before any items are delivered.
//!
//! ```
//! use concord_retry::{Decision, Policy, Replay, Retry, StopReason};
//! use std::{num::NonZeroUsize, time::Duration};
//! # async fn demo() {
//! let policy = Policy::new(NonZeroUsize::new(3).unwrap());
//! let result = Retry::new(policy, Replay::Permitted).run(
//!     |attempt| async move { attempt },
//!     |n| if *n < 2 { Decision::RetryAfter(None) } else { Decision::Done },
//!     || Duration::ZERO, // inject a nondecreasing monotonic clock
//!     |_| async {},    // inject sleep (the zero backoff needs no wait)
//!     |_, ceiling| ceiling,
//! ).await;
//! assert_eq!(result.outcome, Some(2));
//! assert_eq!(result.report.stop, StopReason::Done);
//! # }
//! ```

use std::fmt;
use std::future::{Future, poll_fn};
use std::num::NonZeroUsize;
use std::pin::pin;
use std::task::Poll;
use std::time::Duration;

pub mod http;

#[cfg(feature = "concord")]
pub mod concord;

/// The caller's semantic permission, independent of recreating bytes or classifying outcomes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Replay {
    /// The caller/SDK permits repetition under its safe-method, idempotency-key or provider contract.
    Permitted,
    /// Send once; a retry decision stops with [`StopReason::NotReplayable`].
    Never,
}

/// The SDK's classification of an unread response head or transport outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Decision {
    /// The operation is complete.
    Done,
    /// Another attempt is warranted; the optional server delay is a minimum, never a cap.
    RetryAfter(Option<Duration>),
    /// The classifier explicitly refuses further attempts.
    Stop,
}

/// Attempt and time bounds, with capped exponential backoff. Zero backoff is the default.
#[derive(Clone, Copy, Debug)]
pub struct Policy {
    /// Counts the first attempt; cannot be zero.
    pub max_attempts: NonZeroUsize,
    /// One total monotonic budget, including attempts and waits; `None` means no deadline.
    pub timeout: Option<Duration>,
    /// Backoff ceiling after the first attempt, before injected jitter.
    pub initial_delay: Duration,
    /// Maximum local backoff ceiling. Server minimum delays may exceed it.
    pub max_delay: Duration,
}

impl Policy {
    /// A count bound with no deadline and zero backoff. Set fields to opt into time bounds.
    pub fn new(max_attempts: NonZeroUsize) -> Self {
        Self {
            max_attempts,
            timeout: None,
            initial_delay: Duration::ZERO,
            max_delay: Duration::ZERO,
        }
    }

    /// The ceiling after one-based `attempt`: `initial_delay * 2^(attempt - 1)`, capped.
    /// Zero and huge attempt numbers are safe; zero is treated as the first attempt.
    pub fn backoff(&self, attempt: usize) -> Duration {
        if self.initial_delay.is_zero() {
            return Duration::ZERO;
        }
        let exponent = attempt.saturating_sub(1);
        let nanos = if exponent >= 128 {
            u128::MAX
        } else {
            self.initial_delay
                .as_nanos()
                .saturating_mul(1u128 << exponent)
        }
        .min(self.max_delay.as_nanos());
        Duration::new(
            (nanos / 1_000_000_000) as u64,
            (nanos % 1_000_000_000) as u32,
        )
    }
}

/// Why repetition ended. No variant contains request or response data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StopReason {
    /// The classifier returned [`Decision::Done`].
    Done,
    /// The attempt limit was reached.
    Attempts,
    /// The total deadline expired, or there was no room for the next delay and attempt.
    Deadline,
    /// The classifier returned [`Decision::Stop`].
    ClassifierStop,
    /// Semantic permission or recreatable request bytes were unavailable.
    NotReplayable,
}

/// Attempt accounting; contains only counters, durations and reasons, never secrets.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Report {
    /// Factory invocations, including an attempt dropped at the deadline.
    pub attempts: usize,
    /// Requested backoff sleeps, in order (deadline timers are excluded).
    pub waits: Vec<Duration>,
    /// Monotonic time actually spent in backoff sleeps, including oversleep.
    pub total_waited: Duration,
    /// Why no further attempt was made.
    pub stop: StopReason,
    /// Both semantic permission and, for the adapter, recreatable template bytes were available.
    /// False even when a best-effort single send completed successfully.
    pub replay_available: bool,
}

impl Report {
    pub(crate) fn empty(replay_available: bool, stop: StopReason) -> Self {
        Self {
            attempts: 0,
            waits: Vec::new(),
            total_waited: Duration::ZERO,
            stop,
            replay_available,
        }
    }
}

/// The last completed outcome and accounting. `None` means no attempt completed (preflight or
/// deadline). If a later attempt times out, the previous completed outcome is retained.
/// `Debug` deliberately withholds the outcome, even if its type prints secrets.
pub struct Run<R> {
    /// Last completed application outcome, untouched and unconsumed.
    pub outcome: Option<R>,
    /// Attempt and wait accounting.
    pub report: Report,
}

impl<R> fmt::Debug for Run<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Run")
            .field("outcome", &self.outcome.as_ref().map(|_| "<withheld>"))
            .field("report", &self.report)
            .finish()
    }
}

/// A bounded repetition owner. It never infers replay permission or outcome classification.
#[derive(Clone, Copy, Debug)]
pub struct Retry {
    pub(crate) policy: Policy,
    pub(crate) replay: Replay,
}

impl Retry {
    /// Combines explicit semantic permission and bounds. It does not inspect an HTTP method.
    pub fn new(policy: Policy, replay: Replay) -> Self {
        Self { policy, replay }
    }

    /// Calls `factory(one_based_attempt)` exactly once per started application attempt.
    ///
    /// `now` returns a nondecreasing monotonic duration from any fixed epoch. `sleep` must return
    /// a lazy future that waits at least the requested duration and is cancellation safe; it is
    /// also used to cancel pending attempts at the total deadline. Futures run cooperatively:
    /// blocking synchronous code cannot be preempted. Dropping this future drops all owned work.
    ///
    /// `jitter(attempt, ceiling)` selects a local delay in `0..=ceiling`; out-of-range output is
    /// clamped. The actual delay is `max(local_delay, server_minimum)`. A delay equal to or greater
    /// than the remaining budget stops without sleeping or retrying early. The same deadline is
    /// checked after waits, factories and classification. At the deadline, `Deadline` takes
    /// precedence over classification; otherwise Done/Stop precede permission and count bounds.
    pub async fn run<R, F, Fut, C, N, S, Sleep, J>(
        self,
        mut factory: F,
        mut classify: C,
        mut now: N,
        mut sleep: S,
        mut jitter: J,
    ) -> Run<R>
    where
        F: FnMut(usize) -> Fut,
        Fut: Future<Output = R>,
        C: FnMut(&R) -> Decision,
        N: FnMut() -> Duration,
        S: FnMut(Duration) -> Sleep,
        Sleep: Future<Output = ()>,
        J: FnMut(usize, Duration) -> Duration,
    {
        let start = now();
        let mut report = Report::empty(self.replay == Replay::Permitted, StopReason::Deadline);
        let mut outcome = None;
        loop {
            let remaining = self
                .policy
                .timeout
                .map(|budget| budget.saturating_sub(now().saturating_sub(start)));
            if remaining == Some(Duration::ZERO) {
                break;
            }
            report.attempts += 1;
            let attempt = factory(report.attempts);
            // Defer constructing the timer until an attempt actually yields Pending. Test clocks
            // and sleep factories need not observe timers for immediately ready attempts.
            let remaining = self
                .policy
                .timeout
                .map(|budget| budget.saturating_sub(now().saturating_sub(start)));
            if remaining == Some(Duration::ZERO) {
                break;
            }
            let value = match self.policy.timeout {
                Some(budget) => {
                    until(attempt, async {
                        // The first poll can consume time before yielding: arm against the
                        // original budget, not a duration captured before polling the attempt.
                        let left = budget.saturating_sub(now().saturating_sub(start));
                        if !left.is_zero() {
                            sleep(left).await;
                        }
                    })
                    .await
                }
                None => Some(attempt.await),
            };
            let Some(value) = value else {
                break;
            };
            outcome = Some(value);
            let decision = classify(outcome.as_ref().expect("completed outcome"));
            if self.expired(start, now()) {
                break;
            }
            let minimum = match decision {
                Decision::Done => {
                    report.stop = StopReason::Done;
                    break;
                }
                Decision::Stop => {
                    report.stop = StopReason::ClassifierStop;
                    break;
                }
                Decision::RetryAfter(minimum) => minimum.unwrap_or_default(),
            };
            if !report.replay_available {
                report.stop = StopReason::NotReplayable;
                break;
            }
            if report.attempts == self.policy.max_attempts.get() {
                report.stop = StopReason::Attempts;
                break;
            }
            let ceiling = self.policy.backoff(report.attempts);
            let delay = jitter(report.attempts, ceiling).min(ceiling).max(minimum);
            let before = now();
            if self
                .policy
                .timeout
                .is_some_and(|budget| delay >= budget.saturating_sub(before.saturating_sub(start)))
            {
                break;
            }
            if !delay.is_zero() {
                report.waits.push(delay);
                sleep(delay).await;
                report.total_waited = report
                    .total_waited
                    .saturating_add(now().saturating_sub(before));
            }
        }
        Run { outcome, report }
    }

    fn expired(self, start: Duration, now: Duration) -> bool {
        self.policy
            .timeout
            .is_some_and(|budget| now.saturating_sub(start) >= budget)
    }

    /// Runs with `Instant` and Tokio sleep; jitter remains the caller's injected function.
    #[cfg(feature = "tokio")]
    pub async fn run_tokio<R, F, Fut, C, J>(self, factory: F, classify: C, jitter: J) -> Run<R>
    where
        F: FnMut(usize) -> Fut,
        Fut: Future<Output = R>,
        C: FnMut(&R) -> Decision,
        J: FnMut(usize, Duration) -> Duration,
    {
        let start = std::time::Instant::now();
        self.run(
            factory,
            classify,
            || start.elapsed(),
            tokio::time::sleep,
            jitter,
        )
        .await
    }
}

async fn until<F: Future, S: Future<Output = ()>>(future: F, timer: S) -> Option<F::Output> {
    let mut future = pin!(future);
    let mut timer = pin!(timer);
    poll_fn(|cx| {
        if let Poll::Ready(value) = future.as_mut().poll(cx) {
            return Poll::Ready(Some(value));
        }
        timer.as_mut().poll(cx).map(|()| None)
    })
    .await
}

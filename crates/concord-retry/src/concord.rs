//! Prepared-request adapter (feature `concord`). No decoding, provider classification or stream
//! resumption happens here. Each attempt returns an unread `Received<T, D>` with endpoint identity.
//!
//! A supplied `reqwest::Client` may retry or redirect internally: report counts are application
//! attempts. Strict wire accounting requires disabling those policies explicitly:
//!
//! ```
//! let client = reqwest::Client::builder()
//!     .retry(reqwest::retry::never())
//!     .redirect(reqwest::redirect::Policy::none())
//!     .build().unwrap();
//! ```
//!
//! Apply credentials/signatures in the attempt hook, **after** the wait. Put an idempotency key
//! in the prepared template, so cloning keeps it stable. The hook must preserve the operation's
//! replay contract and that key; it must not introduce side effects before sending.

use crate::{Decision, Replay, Report, Retry, Run, StopReason};
use concord_core::{Error, Prepared, Received};
use std::{fmt, future::Future, time::Duration};

/// How the adapter handles a template whose body cannot be recreated without buffering it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplayMode {
    /// When multiple attempts are requested and permitted, reject before the first I/O if the
    /// template cannot be cloned. A policy with one attempt, or `Replay::Never`, can send once.
    Required,
    /// Send once if bytes cannot be cloned, recording `replay_available = false`. If the
    /// classifier requests repetition, stop with `NotReplayable` without waiting or sending again.
    BestEffort,
}

impl Retry {
    /// Takes the prepared request apart once and calls `request.try_clone()` for every attempt.
    /// A one-shot body is never read or buffered to manufacture replayability.
    ///
    /// `hook(request, one_based_attempt)` runs immediately before each application attempt,
    /// after its wait, so the caller can refresh credentials/signatures. `classify` sees only an
    /// unread final response head or a Concord transport error; consuming the returned outcome
    /// (including stream items and provider error bodies) is the SDK's responsibility.
    ///
    /// Time and jitter have the same contract as [`Retry::run`]. A preflight refusal has no
    /// outcome and zero attempts. Reports and `Run` diagnostics contain no native request data.
    #[allow(
        clippy::too_many_arguments,
        reason = "explicit closures keep injected time and policy visible without an extra runtime abstraction"
    )]
    pub async fn receive<T, D, H, C, N, S, Sleep, J>(
        self,
        prepared: Prepared<T, D>,
        mode: ReplayMode,
        mut hook: H,
        classify: C,
        now: N,
        sleep: S,
        jitter: J,
    ) -> Run<Result<Received<T, D>, Error>>
    where
        H: FnMut(&mut reqwest::Request, usize),
        C: FnMut(&Result<Received<T, D>, Error>) -> Decision,
        N: FnMut() -> Duration,
        S: FnMut(Duration) -> Sleep,
        Sleep: Future<Output = ()>,
        J: FnMut(usize, Duration) -> Duration,
    {
        let (client, request, decoder) = prepared.into_parts();
        let cloneable = request.try_clone().is_some();
        if !cloneable
            && self.replay == Replay::Permitted
            && self.policy.max_attempts.get() > 1
            && mode == ReplayMode::Required
        {
            return Run {
                outcome: None,
                report: Report::empty(false, StopReason::NotReplayable),
            };
        }
        let replay = if cloneable {
            self.replay
        } else {
            Replay::Never
        };
        let mut template = Some(request);
        let client = &client;
        Retry::new(self.policy, replay)
            .run(
                |attempt| {
                    let mut request = template
                        .as_ref()
                        .and_then(reqwest::Request::try_clone)
                        .or_else(|| template.take())
                        .expect("one-shot template sent at most once");
                    hook(&mut request, attempt);
                    async move { decoder.execute(client, request).await }
                },
                classify,
                now,
                sleep,
                jitter,
            )
            .await
    }
}

/// Accounting for [`Retry::receive_with`], without request, response or hook values.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiveReport {
    /// Scheduling report. `attempts` counts dispatches to `Decoder::execute`, excluding hooks.
    /// These are application dispatches, not proof that bytes reached the wire.
    pub retry: Report,
    /// Dispatched attempts that returned a response head or a transport error.
    /// `retry.attempts - completed` counts dispatches cancelled at the deadline (zero or one).
    pub completed: usize,
    /// Hooks that returned an error (zero or one). A cancelled hook is not a refusal.
    pub refused: usize,
    /// Any dispatched attempt returned no response head, including cancellation at the deadline.
    /// Hook failure/cancellation and body decoding errors do not set this flag.
    pub no_response: bool,
}

/// An async adapter's last completed outcome, final attempt state and independent hook refusal.
/// A refusal never overwrites a previous outcome. `Debug` withholds all three caller values.
pub struct ReceiveRun<T, A, E, D = ()> {
    /// Last completed dispatch, unread and unconsumed; retained through later hook/dispatch expiry.
    pub outcome: Option<Result<Received<T, D>, Error>>,
    /// State returned by the hook for the final completed dispatch, retained through decoding.
    /// Dropped before another hook or backoff, even though `outcome` is retained.
    pub context: Option<A>,
    /// The SDK's own error if a hook refused dispatch; no conversion or error trait is required.
    pub refusal: Option<E>,
    /// Dispatch, refusal, completion and scheduling accounting.
    pub report: ReceiveReport,
}

impl<T, A, E, D> fmt::Debug for ReceiveRun<T, A, E, D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReceiveRun")
            .field("outcome", &self.outcome.as_ref().map(|_| "<withheld>"))
            .field("context", &self.context.as_ref().map(|_| "<withheld>"))
            .field("refusal", &self.refusal.as_ref().map(|_| "<withheld>"))
            .field("report", &self.report)
            .finish()
    }
}

impl Retry {
    /// Like [`Retry::receive`], with an async, fallible before-attempt hook and per-attempt state.
    ///
    /// `hook(request, one_based_attempt)` owns a fresh template clone after each wait.
    /// It may await credentials/admission and return `Ok((request, context))` or an SDK error.
    /// Refusal stops with [`StopReason::ClassifierStop`], skips dispatch and classification,
    /// and is returned separately from the last completed outcome. A hook must preserve the
    /// operation's replay contract, body semantics and idempotency key.
    ///
    /// `classify(outcome, context)` sees the unread head/transport error and hook state (for
    /// example a token generation and concurrency lease). The state stays alive through
    /// classification and final decoding, but is dropped before any retry wait or next hook.
    /// The previous outcome is retained, without its state, if that next hook refuses or expires.
    /// Drop the returned context after consuming the final body to release admission resources.
    ///
    /// Time, jitter and replay preflight follow [`Retry::receive`]. One total deadline includes
    /// hooks, dispatches, classification and backoff. Expiry cancels a pending hook without
    /// counting a dispatch or refusal; expiry after successful admission prevents dispatch.
    /// At exact expiry `Deadline` takes precedence, even if a hook just returned a refusal.
    /// Completed heads and refusals are still retained. Cancellation drops owned hook state;
    /// external/shared work started by a hook remains the caller's responsibility.
    ///
    /// [`ReceiveReport`] distinguishes dispatch from admission and remembers headless dispatches.
    /// No request, response, hook error or hook state appears in diagnostics.
    ///
    /// ```
    /// use concord_retry::{Decision, Policy, Replay, Retry, concord::ReplayMode};
    /// use std::{convert::Infallible, num::NonZeroUsize, time::Duration};
    /// # async fn example(prepared: concord_core::Prepared<()>) {
    /// let run = Retry::new(Policy::new(NonZeroUsize::new(2).unwrap()), Replay::Permitted)
    ///     .receive_with(
    ///         prepared,
    ///         ReplayMode::Required,
    ///         |mut request, attempt| async move {
    ///             // Await credential/admission acquisition here, after backoff.
    ///             let generation = std::future::ready(attempt).await;
    ///             request.headers_mut().insert("authorization", "Bearer example".parse().unwrap());
    ///             Ok::<_, Infallible>((request, generation))
    ///         },
    ///         |outcome, generation| {
    ///             // The exact credential generation is available for invalidation on 401.
    ///             let _ = (outcome, generation);
    ///             Decision::Done
    ///         },
    ///         || Duration::ZERO,
    ///         |_| async {},
    ///         |_, ceiling| ceiling,
    ///     ).await;
    /// assert_eq!(run.report.refused, 0);
    /// // Handle run.refusal separately, then consume run.outcome while holding run.context.
    /// # }
    /// ```
    #[allow(
        clippy::too_many_arguments,
        reason = "explicit closures keep injected time and policy visible without an extra runtime abstraction"
    )]
    pub async fn receive_with<T, D, A, E, H, Hook, C, N, S, Sleep, J>(
        self,
        prepared: Prepared<T, D>,
        mode: ReplayMode,
        mut hook: H,
        mut classify: C,
        mut now: N,
        mut sleep: S,
        mut jitter: J,
    ) -> ReceiveRun<T, A, E, D>
    where
        H: FnMut(reqwest::Request, usize) -> Hook,
        Hook: Future<Output = Result<(reqwest::Request, A), E>>,
        C: FnMut(&Result<Received<T, D>, Error>, &mut A) -> Decision,
        N: FnMut() -> Duration,
        S: FnMut(Duration) -> Sleep,
        Sleep: Future<Output = ()>,
        J: FnMut(usize, Duration) -> Duration,
    {
        let (client, request, decoder) = prepared.into_parts();
        let cloneable = request.try_clone().is_some();
        let mut result = ReceiveRun {
            outcome: None,
            context: None,
            refusal: None,
            report: ReceiveReport {
                retry: Report::empty(
                    cloneable && self.replay == Replay::Permitted,
                    StopReason::Deadline,
                ),
                completed: 0,
                refused: 0,
                no_response: false,
            },
        };
        if !cloneable
            && self.replay == Replay::Permitted
            && self.policy.max_attempts.get() > 1
            && mode == ReplayMode::Required
        {
            result.report.retry.stop = StopReason::NotReplayable;
            return result;
        }
        let start = now();
        let mut template = Some(request);
        loop {
            if self.expired(start, now()) {
                break;
            }
            let request = template
                .as_ref()
                .and_then(reqwest::Request::try_clone)
                .or_else(|| template.take())
                .expect("one-shot template sent at most once");
            let admitted = match self.policy.timeout {
                Some(budget) => {
                    crate::until(hook(request, result.report.retry.attempts + 1), async {
                        let left = budget.saturating_sub(now().saturating_sub(start));
                        if !left.is_zero() {
                            sleep(left).await;
                        }
                    })
                    .await
                }
                None => Some(hook(request, result.report.retry.attempts + 1).await),
            };
            let Some(admitted) = admitted else { break };
            let (request, context) = match admitted {
                Ok(admitted) => admitted,
                Err(error) => {
                    result.refusal = Some(error);
                    result.report.refused = 1;
                    if !self.expired(start, now()) {
                        result.report.retry.stop = StopReason::ClassifierStop;
                    }
                    break;
                }
            };
            if self.expired(start, now()) {
                break;
            }
            result.report.retry.attempts += 1;
            let outcome = match self.policy.timeout {
                Some(budget) => {
                    crate::until(decoder.execute(&client, request), async {
                        let left = budget.saturating_sub(now().saturating_sub(start));
                        if !left.is_zero() {
                            sleep(left).await;
                        }
                    })
                    .await
                }
                None => Some(decoder.execute(&client, request).await),
            };
            let Some(outcome) = outcome else {
                result.report.no_response = true;
                break;
            };
            result.report.completed += 1;
            result.report.no_response |= outcome.is_err();
            result.outcome = Some(outcome);
            result.context = Some(context);
            let decision = classify(
                result.outcome.as_ref().expect("completed outcome"),
                result.context.as_mut().expect("admitted context"),
            );
            if self.expired(start, now()) {
                break;
            }
            let minimum = match decision {
                Decision::Done => {
                    result.report.retry.stop = StopReason::Done;
                    break;
                }
                Decision::Stop => {
                    result.report.retry.stop = StopReason::ClassifierStop;
                    break;
                }
                Decision::RetryAfter(minimum) => minimum.unwrap_or_default(),
            };
            if !result.report.retry.replay_available {
                result.report.retry.stop = StopReason::NotReplayable;
                break;
            }
            if result.report.retry.attempts == self.policy.max_attempts.get() {
                result.report.retry.stop = StopReason::Attempts;
                break;
            }
            let attempt = result.report.retry.attempts;
            let ceiling = self.policy.backoff(attempt);
            let delay = jitter(attempt, ceiling).min(ceiling).max(minimum);
            let before = now();
            if self
                .policy
                .timeout
                .is_some_and(|budget| delay >= budget.saturating_sub(before.saturating_sub(start)))
            {
                break;
            }
            drop(result.context.take());
            if !delay.is_zero() {
                result.report.retry.waits.push(delay);
                sleep(delay).await;
                result.report.retry.total_waited = result
                    .report
                    .retry
                    .total_waited
                    .saturating_add(now().saturating_sub(before));
            }
        }
        result
    }
}

#[cfg(test)]
mod tests;

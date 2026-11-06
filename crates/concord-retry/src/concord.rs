//! Prepared-request adapter (feature `concord`). No decoding, provider classification or stream
//! resumption happens here. Each attempt returns an unread `Received<T>` with endpoint identity.
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
use std::{future::Future, time::Duration};

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
    pub async fn receive<T, H, C, N, S, Sleep, J>(
        self,
        prepared: Prepared<T>,
        mode: ReplayMode,
        mut hook: H,
        classify: C,
        now: N,
        sleep: S,
        jitter: J,
    ) -> Run<Result<Received<T>, Error>>
    where
        H: FnMut(&mut reqwest::Request, usize),
        C: FnMut(&Result<Received<T>, Error>) -> Decision,
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

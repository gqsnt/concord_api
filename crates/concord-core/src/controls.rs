//! Caller controls shared by generated requests and core terminal wrappers.
//!
//! A request carries three separate facts about time and work:
//!
//! 1. the effective **declared** timeout of its endpoint or traversal;
//! 2. the caller's **selected** timeout, a finite preference that can be
//!    replaced with [`timeout`](crate::__private::RequestControlState::set_timeout)
//!    and restored with `reset_timeout`;
//! 3. independent additional **caps** supplied through [`CallOptions`], which
//!    only ever narrow execution and survive every later timeout selection and
//!    every terminal transformation (`.response()`, `.accepted_bytes()`,
//!    `.store_as_*()`, `.pages()`, `.collect_prefix()`, `.collect_all()`).
//!
//! Cancellation is expressed with [`CancellationToken`] signals. A request can
//! observe up to [`MAX_CANCELLATION_SOURCES`] distinct signals, combined by OR.
//!
//! The [`RequestControls`] trait is deliberately **open**: applications may
//! implement it for their own wrappers. Implementing it grants no access to
//! native HTTP requests, private bindings, credentials or the executor, and core
//! never treats an implementation as evidence of authenticity. Execution
//! authority always comes from core-owned validated bindings.

use std::fmt;
use std::future::Future;
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll};
use std::time::Duration;

use tokio::sync::Notify;

/// Maximum number of distinct cancellation signals one request can observe.
pub const MAX_CANCELLATION_SOURCES: usize = 8;

/// A cheap, cloneable cancellation signal.
///
/// Clones refer to the same signal. A token has no reset: once cancelled it
/// stays cancelled. Cancelling is synchronous, never blocks and performs no
/// I/O. Observing a cancelled token before first dispatch prevents dispatch;
/// cancellation cannot retract a native request already handed to the
/// transport, undo a remote effect, refund committed quota or erase contact
/// evidence.
#[derive(Clone)]
pub struct CancellationToken {
    inner: Arc<CancellationInner>,
}

struct CancellationInner {
    cancelled: AtomicBool,
    notify: Notify,
}

impl CancellationToken {
    /// Creates a new, not yet cancelled signal.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(CancellationInner {
                cancelled: AtomicBool::new(false),
                notify: Notify::new(),
            }),
        }
    }

    /// Cancels this signal and wakes every current waiter.
    pub fn cancel(&self) {
        if !self.inner.cancelled.swap(true, Ordering::AcqRel) {
            self.inner.notify.notify_waiters();
        }
    }

    /// Returns whether this signal has been cancelled.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.inner.cancelled.load(Ordering::Acquire)
    }

    /// Waits until this signal is cancelled.
    ///
    /// The returned future is cancel-safe: dropping it has no effect on the
    /// signal.
    pub async fn cancelled(&self) {
        loop {
            let notified = self.inner.notify.notified();
            let mut notified = std::pin::pin!(notified);
            notified.as_mut().enable();
            if self.is_cancelled() {
                return;
            }
            notified.await;
            if self.is_cancelled() {
                return;
            }
        }
    }

    pub(crate) fn same_signal(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for CancellationToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CancellationToken")
            .field("cancelled", &self.is_cancelled())
            .finish()
    }
}

/// A request-control validation failure. Raised before any operation starts;
/// it therefore carries no operation identity or dispatch knowledge.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControlError {
    /// A duration was zero where a positive duration is required.
    ZeroDuration,
    /// A duration could not be represented as a finite deadline.
    UnrepresentableDuration,
    /// A ninth distinct cancellation signal was attached to one request.
    TooManyCancellationSources,
}

impl fmt::Display for ControlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ZeroDuration => "request control duration must be positive",
            Self::UnrepresentableDuration => "request control duration is not representable",
            Self::TooManyCancellationSources => "too many cancellation sources for one request",
        })
    }
}

impl std::error::Error for ControlError {}

/// Upper bound on a caller-selected duration. Any larger value cannot be
/// guaranteed to fit an absolute monotonic deadline on every platform.
pub(crate) const MAX_CONTROL_DURATION: Duration = Duration::from_secs(100 * 365 * 24 * 60 * 60);

pub(crate) fn validate_positive_duration(duration: Duration) -> Result<Duration, ControlError> {
    if duration.is_zero() {
        Err(ControlError::ZeroDuration)
    } else if duration > MAX_CONTROL_DURATION {
        Err(ControlError::UnrepresentableDuration)
    } else {
        Ok(duration)
    }
}

/// Additional non-widenable execution caps.
///
/// `CallOptions::default()` equals [`CallOptions::new`] and carries no
/// additional restriction. A missing cap means "no *additional* restriction",
/// never unbounded underlying execution: client configuration and hard limits
/// still apply. Repeated setters intersect (keep the minimum), and applying
/// options to a request takes the componentwise minimum with caps it already
/// carries. Caps never grant retries or other authority that the endpoint's
/// effect, body or credential rules disallow.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CallOptions {
    timeout: Option<Duration>,
    sends: Option<u32>,
    replays: Option<u32>,
    response_bytes: Option<NonZeroUsize>,
    stream_lifetime: Option<Duration>,
    stream_idle: Option<Duration>,
}

fn min_opt<T: Ord + Copy>(current: Option<T>, next: Option<T>) -> Option<T> {
    match (current, next) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (left, None) => left,
        (None, right) => right,
    }
}

impl CallOptions {
    /// Creates options with no additional caps.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            timeout: None,
            sends: None,
            replays: None,
            response_bytes: None,
            stream_lifetime: None,
            stream_idle: None,
        }
    }

    /// Caps the whole operation, starting at the first request poll. The cap
    /// also restricts any later stream consumption or traversal.
    pub fn cap_timeout(mut self, duration: Duration) -> Result<Self, ControlError> {
        let duration = validate_positive_duration(duration)?;
        self.timeout = min_opt(self.timeout, Some(duration));
        Ok(self)
    }

    /// Caps application attempt handoffs, including initial submissions of
    /// later pages. Zero prevents application dispatch.
    #[must_use]
    pub fn cap_sends(mut self, maximum: u32) -> Self {
        self.sends = min_opt(self.sends, Some(maximum));
        self
    }

    /// Caps automatic replays (policy retries plus authentication recovery),
    /// cumulatively across a traversal.
    #[must_use]
    pub fn cap_replays(mut self, maximum: u32) -> Self {
        self.replays = min_opt(self.replays, Some(maximum));
        self
    }

    /// Caps acquired response body bytes.
    #[must_use]
    pub fn cap_response_bytes(mut self, maximum: NonZeroUsize) -> Self {
        self.response_bytes = min_opt(self.response_bytes, Some(maximum));
        self
    }

    /// Caps the total post-acquisition lease of a managed stream.
    pub fn cap_stream_lifetime(mut self, duration: Duration) -> Result<Self, ControlError> {
        let duration = validate_positive_duration(duration)?;
        self.stream_lifetime = min_opt(self.stream_lifetime, Some(duration));
        Ok(self)
    }

    /// Caps cumulative pending-read idle time of a managed stream.
    pub fn cap_stream_idle(mut self, duration: Duration) -> Result<Self, ControlError> {
        let duration = validate_positive_duration(duration)?;
        self.stream_idle = min_opt(self.stream_idle, Some(duration));
        Ok(self)
    }

    /// Returns the componentwise minimum of two option sets.
    #[must_use]
    pub fn intersect(&self, other: &Self) -> Self {
        Self {
            timeout: min_opt(self.timeout, other.timeout),
            sends: min_opt(self.sends, other.sends),
            replays: min_opt(self.replays, other.replays),
            response_bytes: min_opt(self.response_bytes, other.response_bytes),
            stream_lifetime: min_opt(self.stream_lifetime, other.stream_lifetime),
            stream_idle: min_opt(self.stream_idle, other.stream_idle),
        }
    }

    /// Returns the whole-operation timeout cap, if any.
    #[must_use]
    pub const fn timeout_cap(&self) -> Option<Duration> {
        self.timeout
    }

    /// Returns the send cap, if any.
    #[must_use]
    pub const fn send_cap(&self) -> Option<u32> {
        self.sends
    }

    /// Returns the replay cap, if any.
    #[must_use]
    pub const fn replay_cap(&self) -> Option<u32> {
        self.replays
    }

    /// Returns the response-byte cap, if any.
    #[must_use]
    pub const fn response_byte_cap(&self) -> Option<NonZeroUsize> {
        self.response_bytes
    }

    /// Returns the stream lifetime cap, if any.
    #[must_use]
    pub const fn stream_lifetime_cap(&self) -> Option<Duration> {
        self.stream_lifetime
    }

    /// Returns the stream idle cap, if any.
    #[must_use]
    pub const fn stream_idle_cap(&self) -> Option<Duration> {
        self.stream_idle
    }
}

/// A value-free summary of a request's effective controls.
///
/// It reports declared, selected and effective acquisition (or aggregate
/// traversal) timeouts, the optional whole-operation cap, the effective
/// send/replay/response/stream caps and the number of cancellation sources.
/// It contains no token internals and no request values.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ControlSummary {
    declared_timeout: Option<Duration>,
    selected_timeout: Option<Duration>,
    caps: CallOptions,
    cancellation_sources: usize,
}

impl ControlSummary {
    pub(crate) fn new(
        declared_timeout: Option<Duration>,
        selected_timeout: Option<Duration>,
        caps: CallOptions,
        cancellation_sources: usize,
    ) -> Self {
        Self {
            declared_timeout,
            selected_timeout,
            caps,
            cancellation_sources,
        }
    }

    /// Returns the effective declared (or default) timeout.
    #[must_use]
    pub const fn declared_timeout(&self) -> Option<Duration> {
        self.declared_timeout
    }

    /// Returns the caller-selected timeout preference, or the declared value
    /// when no selection was made.
    #[must_use]
    pub fn selected_timeout(&self) -> Option<Duration> {
        self.selected_timeout.or(self.declared_timeout)
    }

    /// Returns whether the caller replaced the declared timeout.
    #[must_use]
    pub const fn timeout_was_selected(&self) -> bool {
        self.selected_timeout.is_some()
    }

    /// Returns the effective timeout: the selected timeout narrowed by the
    /// whole-operation cap.
    #[must_use]
    pub fn effective_timeout(&self) -> Option<Duration> {
        min_opt(self.selected_timeout(), self.caps.timeout)
    }

    /// Returns the whole-operation timeout cap, if any.
    #[must_use]
    pub const fn timeout_cap(&self) -> Option<Duration> {
        self.caps.timeout
    }

    /// Returns the effective send cap, if any.
    #[must_use]
    pub const fn send_cap(&self) -> Option<u32> {
        self.caps.sends
    }

    /// Returns the effective replay cap, if any.
    #[must_use]
    pub const fn replay_cap(&self) -> Option<u32> {
        self.caps.replays
    }

    /// Returns the effective response-byte cap, if any.
    #[must_use]
    pub const fn response_byte_cap(&self) -> Option<NonZeroUsize> {
        self.caps.response_bytes
    }

    /// Returns the effective stream lifetime cap, if any.
    #[must_use]
    pub const fn stream_lifetime_cap(&self) -> Option<Duration> {
        self.caps.stream_lifetime
    }

    /// Returns the effective stream idle cap, if any.
    #[must_use]
    pub const fn stream_idle_cap(&self) -> Option<Duration> {
        self.caps.stream_idle
    }

    /// Returns the number of distinct cancellation signals attached.
    #[must_use]
    pub const fn cancellation_sources(&self) -> usize {
        self.cancellation_sources
    }
}

/// Open caller-composition interface shared by generated requests and core
/// terminal wrappers.
///
/// Library terminal wrappers and every macro-generated named request
/// implement this trait, so generic application helpers can apply common
/// controls without naming a concrete endpoint type:
///
/// ```
/// use std::future::IntoFuture;
/// use concord_core::contracts::{ApiError, CallOptions, RequestControls};
///
/// pub async fn execute<R, T>(request: R, options: &CallOptions) -> Result<T, ApiError>
/// where
///     R: RequestControls + IntoFuture<Output = Result<T, ApiError>>,
/// {
///     request.with_options(options).await
/// }
/// ```
///
/// # Extension contract
///
/// * **Ownership.** `with_options` and `cancel_with` consume the inert request
///   and return it with narrowed controls; `control_summary` borrows it. No
///   control call contacts the server.
/// * **Concurrency.** Control calls operate on one owned value; a
///   [`CancellationToken`] may be shared and triggered from any thread.
/// * **Cancellation.** Attaching a token only records it; once triggered,
///   core observes it at the operation's next await point and stops later
///   work without rolling back effects that already happened. At most
///   [`MAX_CANCELLATION_SOURCES`] distinct signals are kept.
/// * **Input sensitivity.** Controls and their summary are value-free: no
///   request value, credential or URL is read or reported.
/// * **Errors.** `cancel_with` fails with
///   [`ControlError::TooManyCancellationSources`] instead of discarding an
///   existing restriction; `with_options` cannot fail because caps only
///   narrow.
/// * **Resource obligations.** Control state is bounded by the cancellation
///   source limit and the fixed cap set; an implementation outside Concord
///   must keep its own state equally finite.
/// * **Negative capabilities.** The trait is intentionally **not sealed**.
///   Any type may implement it; doing so grants no ability to obtain a native
///   request, forge a private binding, access credentials or bypass the
///   managed executor. Core never uses an implementation of this trait as
///   authenticity evidence.
pub trait RequestControls: Sized {
    /// Applies additional caps, keeping the componentwise minimum with caps
    /// already carried. Caps can only narrow execution.
    #[must_use]
    fn with_options(self, options: &CallOptions) -> Self;

    /// Returns the value-free effective control summary.
    fn control_summary(&self) -> ControlSummary;

    /// Attaches one more cancellation signal (deduplicated by identity).
    ///
    /// Fails with [`ControlError::TooManyCancellationSources`] when a ninth
    /// distinct signal would be attached; existing restrictions are never
    /// discarded silently.
    fn cancel_with(self, token: CancellationToken) -> Result<Self, ControlError>;
}

/// Core-owned per-request control state carried by requests and terminals.
#[doc(hidden)]
#[derive(Clone, Debug, Default)]
pub struct RequestControlState {
    selected_timeout: Option<Duration>,
    caps: CallOptions,
    cancellation: Vec<CancellationToken>,
}

impl RequestControlState {
    /// Creates control state with no selection, caps or cancellation.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            selected_timeout: None,
            caps: CallOptions::new(),
            cancellation: Vec::new(),
        }
    }

    /// Replaces the selected finite timeout preference. Caps are retained.
    pub fn set_timeout(&mut self, duration: Duration) -> Result<(), ControlError> {
        self.selected_timeout = Some(validate_positive_duration(duration)?);
        Ok(())
    }

    /// Restores the declared timeout. Caps are retained.
    pub fn reset_timeout(&mut self) {
        self.selected_timeout = None;
    }

    /// Intersects additional caps.
    pub fn apply_options(&mut self, options: &CallOptions) {
        self.caps = self.caps.intersect(options);
    }

    /// Attaches a cancellation signal, deduplicating by identity.
    pub fn add_cancellation(&mut self, token: CancellationToken) -> Result<(), ControlError> {
        if self
            .cancellation
            .iter()
            .any(|existing| existing.same_signal(&token))
        {
            return Ok(());
        }
        if self.cancellation.len() >= MAX_CANCELLATION_SOURCES {
            return Err(ControlError::TooManyCancellationSources);
        }
        self.cancellation.push(token);
        Ok(())
    }

    /// Returns the caller-selected timeout, if any.
    #[must_use]
    pub const fn selected_timeout(&self) -> Option<Duration> {
        self.selected_timeout
    }

    /// Returns the accumulated caps.
    #[must_use]
    pub const fn caps(&self) -> &CallOptions {
        &self.caps
    }

    /// Returns the attached cancellation signals.
    #[must_use]
    pub fn cancellation(&self) -> &[CancellationToken] {
        &self.cancellation
    }

    /// Returns whether any attached signal is already cancelled.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancellation
            .iter()
            .any(CancellationToken::is_cancelled)
    }

    /// Resolves the timeout to enforce from the first poll: the selected (or
    /// declared) timeout narrowed by the whole-operation cap.
    #[must_use]
    pub fn effective_timeout(&self, declared: Option<Duration>) -> Option<Duration> {
        min_opt(self.selected_timeout.or(declared), self.caps.timeout)
    }

    /// Builds the value-free summary for a declared timeout.
    #[must_use]
    pub fn summary(&self, declared: Option<Duration>) -> ControlSummary {
        ControlSummary::new(
            declared,
            self.selected_timeout,
            self.caps.clone(),
            self.cancellation.len(),
        )
    }

    /// Waits until any attached signal is cancelled; never resolves when no
    /// signal is attached.
    pub fn cancelled(&self) -> AnyCancelled {
        AnyCancelled::new(self.cancellation.clone())
    }
}

/// A future resolving when any of a bounded set of signals is cancelled.
#[doc(hidden)]
pub struct AnyCancelled {
    waiters: Vec<Pin<Box<dyn Future<Output = ()> + Send>>>,
}

impl AnyCancelled {
    /// Builds a waiter over an already bounded, deduplicated signal set
    /// (for example the signals captured by a managed stream at handoff).
    pub(crate) fn new(tokens: Vec<CancellationToken>) -> Self {
        let waiters = tokens
            .into_iter()
            .map(|token| {
                Box::pin(async move { token.cancelled().await })
                    as Pin<Box<dyn Future<Output = ()> + Send>>
            })
            .collect();
        Self { waiters }
    }
}

impl Future for AnyCancelled {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<()> {
        for waiter in &mut self.waiters {
            if waiter.as_mut().poll(context).is_ready() {
                return Poll::Ready(());
            }
        }
        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn call_options_default_is_new_and_caps_intersect() {
        assert_eq!(CallOptions::default(), CallOptions::new());
        let options = CallOptions::new()
            .cap_sends(5)
            .cap_sends(9)
            .cap_replays(3)
            .cap_timeout(Duration::from_secs(10))
            .and_then(|options| options.cap_timeout(Duration::from_secs(4)))
            .expect("valid caps");
        assert_eq!(options.send_cap(), Some(5));
        assert_eq!(options.timeout_cap(), Some(Duration::from_secs(4)));
        let other = CallOptions::new().cap_sends(2).cap_replays(7);
        let combined = options.intersect(&other);
        assert_eq!(combined.send_cap(), Some(2));
        assert_eq!(combined.replay_cap(), Some(3));
        assert_eq!(combined.timeout_cap(), Some(Duration::from_secs(4)));
    }

    #[test]
    fn zero_and_unrepresentable_durations_are_control_errors() {
        assert_eq!(
            CallOptions::new().cap_timeout(Duration::ZERO),
            Err(ControlError::ZeroDuration)
        );
        assert_eq!(
            CallOptions::new().cap_stream_idle(Duration::MAX),
            Err(ControlError::UnrepresentableDuration)
        );
        let mut state = RequestControlState::new();
        assert_eq!(
            state.set_timeout(Duration::ZERO),
            Err(ControlError::ZeroDuration)
        );
    }

    #[test]
    fn selected_timeout_never_removes_cap() {
        let mut state = RequestControlState::new();
        state.apply_options(
            &CallOptions::new()
                .cap_timeout(Duration::from_secs(5))
                .expect("cap"),
        );
        state.set_timeout(Duration::from_secs(60)).expect("select");
        assert_eq!(
            state.effective_timeout(Some(Duration::from_secs(30))),
            Some(Duration::from_secs(5))
        );
        state.reset_timeout();
        assert_eq!(
            state.effective_timeout(Some(Duration::from_secs(30))),
            Some(Duration::from_secs(5))
        );
        let summary = state.summary(Some(Duration::from_secs(30)));
        assert_eq!(summary.declared_timeout(), Some(Duration::from_secs(30)));
        assert_eq!(summary.selected_timeout(), Some(Duration::from_secs(30)));
        assert_eq!(summary.effective_timeout(), Some(Duration::from_secs(5)));
        assert_eq!(summary.timeout_cap(), Some(Duration::from_secs(5)));
    }

    #[test]
    fn cancellation_sources_deduplicate_and_are_bounded() {
        let mut state = RequestControlState::new();
        let shared = CancellationToken::new();
        state.add_cancellation(shared.clone()).expect("first");
        state.add_cancellation(shared.clone()).expect("duplicate");
        assert_eq!(state.cancellation().len(), 1);
        for _ in 1..MAX_CANCELLATION_SOURCES {
            state
                .add_cancellation(CancellationToken::new())
                .expect("within bound");
        }
        assert_eq!(
            state.add_cancellation(CancellationToken::new()),
            Err(ControlError::TooManyCancellationSources)
        );
        assert_eq!(state.cancellation().len(), MAX_CANCELLATION_SOURCES);
        assert!(!state.is_cancelled());
        shared.cancel();
        assert!(state.is_cancelled());
    }

    #[tokio::test]
    async fn cancelled_future_wakes_on_any_signal() {
        let first = CancellationToken::new();
        let second = CancellationToken::new();
        let mut state = RequestControlState::new();
        state.add_cancellation(first.clone()).expect("first");
        state.add_cancellation(second.clone()).expect("second");
        let waiter = tokio::spawn(state.cancelled());
        tokio::task::yield_now().await;
        second.cancel();
        waiter.await.expect("waiter completes");
        assert!(second.is_cancelled());
        assert!(!first.is_cancelled());
        // A token cancelled before waiting resolves immediately.
        first.cancel();
        first.cancelled().await;
    }

    #[test]
    fn token_debug_reveals_no_internals() {
        let token = CancellationToken::new();
        assert_eq!(
            format!("{token:?}"),
            "CancellationToken { cancelled: false }"
        );
    }
}

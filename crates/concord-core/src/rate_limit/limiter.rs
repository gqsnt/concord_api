use std::sync::Arc;
use std::time::Duration;

use http::{Method, StatusCode};

use crate::error::RateLimitErrorKind;
use crate::hooks::BoxFuture;
use crate::redaction::SanitizedHeaders;

use super::{RateLimitError, RateLimitPermit, RateLimitPlan};

/// Default maximum aggregate byte length of one resolved rate-limit key.
pub use crate::runtime::defaults::DEFAULT_RATE_LIMIT_KEY_BYTES;

/// Safe request metadata supplied to a selected rate limiter.
pub struct RateLimitRequest<'a> {
    /// Stable generated endpoint identity.
    pub endpoint: &'a str,
    /// Application HTTP method.
    pub method: &'a Method,
    /// Credential-sanitized logical URL. Public values may still contain
    /// operational identifiers or personally identifiable information.
    pub safe_url: &'a str,
    /// Final logical host when one exists.
    pub host: Option<&'a str>,
    /// Zero-based page index; direct endpoint execution uses zero.
    pub page_index: u32,
    /// Canonical immutable effective plan.
    pub plan: &'a RateLimitPlan,
}

/// Sanitized response-head metadata supplied to a selected rate limiter.
pub struct RateLimitResponse<'a> {
    /// The matching body-free request context.
    pub request: RateLimitRequest<'a>,
    /// Final response status for this Concord attempt.
    pub status: StatusCode,
    /// Sanitized response headers.
    pub headers: SanitizedHeaders<'a>,
    /// Centrally configured maximum duration for server-directed cooldown state.
    pub cooldown_cap: Duration,
}

/// Application-supplied or Concord-provided per-attempt limiter authority.
///
/// Calls may overlap. Implementations own the boundedness of CPU, memory,
/// retries, maps, queues, spawned tasks, metric cardinality, and external I/O,
/// and must treat resolved plan keys and logical hosts as potentially sensitive.
/// Custom implementations are responsible for protecting those values in their
/// own logs and storage. Acquisition and response observation are
/// correctness-relevant runtime authority and run under the remaining operation
/// deadline. Observation cannot replace the received response, but its safe
/// error or timeout determines the operation before the user response hook,
/// challenge handling, status handling, or body acquisition. With the operation
/// timeout disabled, a custom observation future is trusted and may remain
/// pending. Futures are dropped on cancellation, but completed reservations and
/// external effects are not rolled back. Concord does not automatically bound
/// custom limiter state, synchronous work before a future is returned, or
/// application-created tasks.
pub trait RateLimiter: Send + Sync + 'static {
    /// Returns the maximum aggregate byte length accepted while preparing one
    /// resolved rate-limit key.
    ///
    /// The default is [`DEFAULT_RATE_LIMIT_KEY_BYTES`]. Returning zero permits
    /// only global keys. Values above
    /// [`crate::rate_limit::MAX_RATE_LIMIT_KEY_BYTES`] are invalid and fail
    /// client construction. Core snapshots this value with the selected
    /// limiter and validates generated and runtime-defined plans before
    /// credentials, limiter acquisition, body materialization, or transport
    /// work.
    fn preparation_key_limit(&self) -> usize {
        DEFAULT_RATE_LIMIT_KEY_BYTES
    }

    /// Acquires admission for exactly one visible Concord attempt.
    ///
    /// The returned [`RateLimitPermit`] is a proof of admission, not a
    /// concurrency lease retained until request completion.
    fn acquire<'a>(
        &'a self,
        request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>>;

    /// Observes one sanitized response head before the user response hook.
    fn observe<'a>(
        &'a self,
        _response: RateLimitResponse<'a>,
    ) -> BoxFuture<'a, Result<(), RateLimitError>> {
        Box::pin(async { Ok(()) })
    }
}

/// The future-acquisition scope of an observed cooldown.
pub enum CooldownScope {
    /// Every exact rule/key pair in the current plan, or the host for an empty plan.
    CurrentBuckets,
    /// Every later request to the current logical host through this limiter.
    Host,
    /// Every later request through this limiter object.
    Client,
}

/// One observer-proposed future-acquisition cooldown.
pub struct Cooldown {
    /// Proposed duration before runtime clamping.
    pub duration: Duration,
    /// Affected future-acquisition scope.
    pub scope: CooldownScope,
}

/// Sanitized response-head context supplied to a rate-limit observer.
pub struct RateLimitResponseHead<'a> {
    /// Stable generated endpoint identity.
    pub endpoint: &'a str,
    /// Application HTTP method.
    pub method: &'a Method,
    /// Credential-sanitized logical URL. Public values may still contain
    /// operational identifiers or personally identifiable information.
    pub safe_url: &'a str,
    /// Final logical host when one exists.
    pub host: Option<&'a str>,
    /// Zero-based page index; direct endpoint execution uses zero.
    pub page_index: u32,
    /// Canonical immutable effective plan.
    pub plan: &'a RateLimitPlan,
    /// Final response status for this Concord attempt.
    pub status: StatusCode,
    /// Sanitized response headers.
    pub headers: SanitizedHeaders<'a>,
}

/// Synchronous policy for translating a response head into a future cooldown.
///
/// Calls for independent responses may overlap and receive resolved rate keys,
/// logical hosts, and sanitized response metadata. Implementations own logging
/// and storage and must bound synchronous CPU, memory, retries, maps, queues,
/// spawned tasks, metric cardinality, and external I/O. Concord cannot preempt
/// this callback. Observation is awaited before body processing but cannot
/// replace the current result, so the implementation must self-bound its work;
/// Concord checks the operation deadline after return. Cancellation prevents
/// later work only, and custom observer state is not automatically bounded.
pub trait RateLimitObserver: Send + Sync + 'static {
    /// Returns a future-acquisition cooldown, or `None` for no state change.
    fn observe(&self, response: &RateLimitResponseHead<'_>) -> Option<Cooldown>;
}

/// An explicit limiter that authorizes every plan and ignores observation.
pub struct NoopRateLimiter;

#[allow(
    clippy::new_without_default,
    reason = "the normative contract specifies an explicit constructor without a Default contract"
)]
impl NoopRateLimiter {
    /// Creates an explicit no-op limiter.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl RateLimiter for NoopRateLimiter {
    fn acquire<'a>(
        &'a self,
        _request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        Box::pin(async { Ok(RateLimitPermit::granted()) })
    }
}

pub(crate) struct FallbackRateLimiter;

impl RateLimiter for FallbackRateLimiter {
    fn acquire<'a>(
        &'a self,
        request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        Box::pin(async move {
            if request.plan.is_empty() {
                Ok(RateLimitPermit::granted())
            } else {
                Err(RateLimitError::new(
                    RateLimitErrorKind::InvalidConfiguration,
                ))
            }
        })
    }
}

pub(crate) fn fallback_limiter() -> Arc<dyn RateLimiter> {
    Arc::new(FallbackRateLimiter)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limiter_metadata_default_is_the_core_key_bound() {
        assert_eq!(
            NoopRateLimiter::new().preparation_key_limit(),
            DEFAULT_RATE_LIMIT_KEY_BYTES
        );
    }

    #[tokio::test]
    async fn noop_and_fallback_are_explicitly_distinct() {
        let method = Method::GET;
        let empty = RateLimitPlan::empty();
        NoopRateLimiter::new()
            .acquire(RateLimitRequest {
                endpoint: "RateApi::get",
                method: &method,
                safe_url: "https://example.test/get",
                host: Some("example.test"),
                page_index: 0,
                plan: &empty,
            })
            .await
            .expect("noop grants empty plan");
        FallbackRateLimiter
            .acquire(RateLimitRequest {
                endpoint: "RateApi::get",
                method: &method,
                safe_url: "https://example.test/get",
                host: Some("example.test"),
                page_index: 0,
                plan: &empty,
            })
            .await
            .expect("fallback grants empty plan");
    }
}

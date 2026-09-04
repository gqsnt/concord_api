use std::sync::Arc;
use std::time::Duration;

use http::{Method, StatusCode};

use crate::error::RateLimitErrorKind;
use crate::hooks::{AttemptInfo, BoxFuture};

use super::{RateLimitError, RateLimitPermit, RateLimitPlan};

/// Default maximum aggregate byte length of one resolved rate-limit key.
pub use crate::runtime::defaults::DEFAULT_RATE_LIMIT_KEY_BYTES;

/// Safe request metadata supplied to a selected rate limiter.
pub struct RateLimitRequest<'a> {
    /// Stable generated endpoint identity.
    pub endpoint: &'a str,
    /// Application HTTP method.
    pub method: &'a Method,
    /// Normalized logical origin only when the effective plan explicitly uses
    /// the built-in `origin` dimension.
    ///
    /// The value can contain operational identifiers and must be protected in
    /// custom limiter storage and logs. It is absent for every other plan.
    pub logical_origin: Option<&'a str>,
    /// Zero-based page index; direct endpoint execution uses zero.
    pub page_index: u32,
    /// Structural identity of this visible managed attempt.
    pub attempt: AttemptInfo,
    /// Canonical immutable effective plan.
    pub plan: &'a RateLimitPlan,
}

/// Typed response feedback supplied to a selected rate limiter.
pub struct RateLimitResponse<'a> {
    /// The matching body-free request context.
    pub request: RateLimitRequest<'a>,
    /// Final response status for this Concord attempt.
    pub status: StatusCode,
    /// Standard server delay parsed once by core from the response head.
    pub server_delay: Option<Duration>,
    /// Centrally configured maximum duration for server-directed cooldown state.
    pub cooldown_cap: Duration,
}

/// Application-supplied or Concord-provided per-attempt limiter authority.
///
/// Calls may overlap. Implementations own the boundedness of CPU, memory,
/// retries, maps, queues, spawned tasks, metric cardinality, and external I/O,
/// and must treat resolved plan keys and an explicitly requested logical origin
/// as potentially sensitive.
/// Custom implementations are responsible for protecting those values in their
/// own logs and storage. Acquisition is current-attempt authority and runs under
/// the remaining operation deadline. Response observation is synchronous,
/// prompt, and non-fallible for the current result; an implementation may record
/// degradation that a later `acquire` reports. Concord cannot preempt arbitrary
/// blocking synchronous Rust or bound application-created work.
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
    /// The returned [`RateLimitPermit`] is proof that the implementation
    /// admitted the complete conjunctive [`RateLimitPlan`], not a concurrency
    /// lease retained until request completion. The bundled process-local
    /// limiter evaluates and reserves all policies atomically. External or
    /// distributed implementations must document whether a rejected
    /// multi-policy evaluation is atomic or conservatively retains an earlier
    /// reservation; core cannot promise rollback for authority it does not
    /// own.
    fn acquire<'a>(
        &'a self,
        request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>>;

    /// Observes typed response feedback for future admission only.
    ///
    /// Arbitrary response-header names and values are deliberately absent.
    /// Implementations cannot replace the current attempt result.
    fn observe(&self, _response: RateLimitResponse<'_>) {}
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
                logical_origin: None,
                page_index: 0,
                attempt: AttemptInfo::INITIAL,
                plan: &empty,
            })
            .await
            .expect("noop grants empty plan");
        FallbackRateLimiter
            .acquire(RateLimitRequest {
                endpoint: "RateApi::get",
                method: &method,
                logical_origin: None,
                page_index: 0,
                attempt: AttemptInfo::INITIAL,
                plan: &empty,
            })
            .await
            .expect("fallback grants empty plan");
    }
}

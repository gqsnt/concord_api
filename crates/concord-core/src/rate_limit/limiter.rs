use std::sync::Arc;
use std::time::Duration;

use http::{Method, StatusCode};

use crate::error::RateLimitErrorKind;
use crate::hooks::{AttemptInfo, BoxFuture};

use super::{Enforcement, LimiterCapability, RateLimitError, RateLimitPermit, RateLimitPlan};

/// Default maximum aggregate byte length of one resolved rate-limit key.
pub use crate::runtime::defaults::DEFAULT_RATE_LIMIT_KEY_BYTES;

/// Safe request metadata supplied to a selected rate limiter.
///
/// The request carries the complete effective conjunctive plan with protected
/// canonical keys, the value-free schema of every policy
/// ([`crate::rate_limit::RateLimitPolicy::schema`]), and the limiter
/// capability that core snapshotted at client build. Together, each policy
/// schema and [`Self::capability`] form the complete schema descriptor of
/// RATE-05: policy ID, ordered labels and encoding IDs, framing version,
/// canonical window schema, and limiter namespace/algorithm identity. It
/// never carries request bodies, credentials or raw headers.
pub struct RateLimitRequest<'a> {
    /// Operational endpoint ID (an explicit `id` pin or navigation-derived).
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
    /// The selected limiter's capability as snapshotted by core at client
    /// build: enforcement kind, adapter namespace and algorithm version.
    pub capability: LimiterCapability,
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
/// # Atomic conjunctive admission (RATE-04)
///
/// Each successful `acquire` admits the complete conjunctive plan (every
/// window of every policy) atomically, or returns an error / keeps waiting
/// without any new quota debit. Waiting or failed admission must not
/// partially reserve a prefix of the plan. This strengthens the Concord 2.1
/// contract, which permitted conservative partial reservations; custom
/// limiters must be migrated.
///
/// # Shared persistent adapters
///
/// An adapter reporting [`Enforcement::SharedPersistent`] must, inside
/// `acquire` and before any state use, atomically register or compare every
/// policy schema under its namespace and algorithm version, fail closed on an
/// incompatible active schema under the same ID (never treating it as a fresh
/// bucket), and keep registrations in finite storage that never evicts an
/// active contract.
///
/// # Extension contract
///
/// * **Ownership.** The limiter owns all of its quota, cooldown and schema
///   state. Clients share state only when explicitly given the same limiter
///   object; separate objects are independent even for identical policy IDs.
/// * **Concurrency.** Calls may overlap from many tasks. No core lock is held
///   while `acquire` is pending.
/// * **Cancellation.** `acquire` runs under the remaining operation deadline
///   and its future may be dropped at any await point; a dropped, uncommitted
///   acquisition must leave no debit. A committed admission is
///   non-refundable: later cancellation, a credential-freshness restart or a
///   failure before dispatch may consume quota without a send. A permit is an
///   admission result, not a held concurrency lease.
/// * **Input sensitivity.** Resolved key values and the optional logical
///   origin are potentially sensitive; implementations must protect them in
///   their own logs, metrics and storage. Policy IDs, labels, encoding IDs
///   and windows are value-free schema facts.
/// * **Errors.** Acquisition failures become `RateLimit` API errors before
///   dispatch. Observation is synchronous, prompt, and non-fallible for the
///   current result; an implementation may record degradation that a later
///   `acquire` reports.
/// * **Resource obligations.** Implementations own the boundedness of CPU,
///   memory, maps, queues, spawned tasks, metric cardinality and external
///   I/O. Concord cannot preempt arbitrary blocking synchronous Rust.
/// * **Negative capabilities.** A limiter never sees bodies, credentials or
///   raw headers, cannot change the current endpoint result from feedback,
///   and cannot add endpoint behavior.
pub trait RateLimiter: Send + Sync + 'static {
    /// Returns the structural enforcement capability of this limiter.
    ///
    /// This method is required: every limiter must state truthfully whether
    /// it enforces locally, is an explicit no-op, or is a shared persistent
    /// adapter, so that plans can disclose the actual enforcement choice.
    /// Core calls it once, when the limiter is selected, and snapshots the
    /// result; it must be prompt and side-effect free.
    fn capability(&self) -> LimiterCapability;

    /// Returns the maximum aggregate byte length accepted while preparing one
    /// resolved rate-limit key.
    ///
    /// The default is [`DEFAULT_RATE_LIMIT_KEY_BYTES`]. Every v3 key has at
    /// least one dimension, so zero admits no policy: a generated client that
    /// declares policies refuses to build with such a limiter. Values above
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
    /// atomically admitted the complete conjunctive [`RateLimitPlan`]; see the
    /// trait documentation for the all-or-nothing and non-refundable rules.
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
///
/// Its capability is [`Enforcement::Unenforced`]; bound plans label declared
/// policies as "declared policies; enforcement explicitly disabled by
/// application selection". Endpoint syntax never installs it implicitly.
pub struct NoopRateLimiter;

#[allow(
    clippy::new_without_default,
    reason = "the normative contract specifies an explicit constructor without a Default contract"
)]
impl NoopRateLimiter {
    /// Structural capability of the explicit no-op limiter.
    pub const CAPABILITY: LimiterCapability =
        LimiterCapability::new(Enforcement::Unenforced, "concord.noop", 1);

    /// Creates an explicit no-op limiter.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl RateLimiter for NoopRateLimiter {
    fn capability(&self) -> LimiterCapability {
        Self::CAPABILITY
    }

    fn acquire<'a>(
        &'a self,
        _request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        Box::pin(async { Ok(RateLimitPermit::granted()) })
    }
}

pub(crate) struct FallbackRateLimiter;

impl FallbackRateLimiter {
    pub(crate) const CAPABILITY: LimiterCapability =
        LimiterCapability::new(Enforcement::Unenforced, "concord.unselected", 1);
}

impl RateLimiter for FallbackRateLimiter {
    fn capability(&self) -> LimiterCapability {
        Self::CAPABILITY
    }

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

    #[test]
    fn noop_and_fallback_capabilities_are_explicitly_unenforced_and_distinct() {
        let noop = NoopRateLimiter::new().capability();
        assert_eq!(noop.enforcement(), Enforcement::Unenforced);
        assert_eq!(noop.namespace(), "concord.noop");
        assert_eq!(noop.algorithm_version(), 1);
        noop.validate().expect("noop capability is valid");
        let fallback = FallbackRateLimiter.capability();
        assert_ne!(noop, fallback);
        fallback.validate().expect("fallback capability is valid");
        assert_eq!(
            Enforcement::Unenforced.plan_label(),
            "declared policies; enforcement explicitly disabled by application selection"
        );
    }

    #[tokio::test]
    async fn noop_and_fallback_are_explicitly_distinct() {
        let method = Method::GET;
        let empty = RateLimitPlan::empty();
        NoopRateLimiter::new()
            .acquire(RateLimitRequest {
                endpoint: "rate.api.get",
                method: &method,
                logical_origin: None,
                page_index: 0,
                attempt: AttemptInfo::INITIAL,
                plan: &empty,
                capability: NoopRateLimiter::CAPABILITY,
            })
            .await
            .expect("noop grants empty plan");
        FallbackRateLimiter
            .acquire(RateLimitRequest {
                endpoint: "rate.api.get",
                method: &method,
                logical_origin: None,
                page_index: 0,
                attempt: AttemptInfo::INITIAL,
                plan: &empty,
                capability: FallbackRateLimiter::CAPABILITY,
            })
            .await
            .expect("fallback grants empty plan");
    }
}

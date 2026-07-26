//! A response observer that delegates bounded `Retry-After` interpretation.
//!
//! Required feature: `rate-limit`. Mode: construction-only.
//! Documentation owners: `docs/features/rate-limiting.md` and
//! `docs/extending/rate-limiting.md`.
//! Proving test: `tests/construction.rs`.

// ANCHOR: custom_observer_extension
use concord::rate_limit::{Cooldown, RateLimitObserver, RateLimitResponseHead, RetryAfterObserver};

/// Applies Concord's first-party bounded `Retry-After` policy.
#[derive(Default)]
pub struct DelegatingRetryAfterObserver;

impl RateLimitObserver for DelegatingRetryAfterObserver {
    fn observe(&self, response: &RateLimitResponseHead<'_>) -> Option<Cooldown> {
        RetryAfterObserver.observe(response)
    }
}
// ANCHOR_END: custom_observer_extension

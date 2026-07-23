//! A response observer that delegates bounded `Retry-After` interpretation.
//!
//! Required feature: `rate-limit-governor`. Mode: construction-only.
//! Documentation owner: `docs/guide/rate-limiting.md`.
//! Proving test: `tests/construction.rs`.

use concord::rate_limit::{
    Cooldown, DefaultRateLimitObserver, RateLimitObserver, RateLimitResponseHead,
};

/// Applies Concord's first-party bounded `Retry-After` policy.
#[derive(Default)]
pub struct RetryAfterObserver;

impl RateLimitObserver for RetryAfterObserver {
    fn observe(&self, response: &RateLimitResponseHead<'_>) -> Option<Cooldown> {
        DefaultRateLimitObserver.observe(response)
    }
}

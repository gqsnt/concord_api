//! A response observer that delegates bounded `Retry-After` interpretation.
//!
//! Required feature: `rate-limit`. Mode: construction-only.
//! Documentation owner: `docs/guide/06-pagination-and-rate-limits.md`.
//! Proving test: `tests/construction.rs`.

use concord::rate_limit::{Cooldown, RateLimitObserver, RateLimitResponseHead, RetryAfterObserver};

/// Applies Concord's first-party bounded `Retry-After` policy.
#[derive(Default)]
pub struct DelegatingRetryAfterObserver;

impl RateLimitObserver for DelegatingRetryAfterObserver {
    fn observe(&self, response: &RateLimitResponseHead<'_>) -> Option<Cooldown> {
        RetryAfterObserver.observe(response)
    }
}

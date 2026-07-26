//! A minimal downstream limiter proving the public semantic contract is enough.
//!
//! Required feature: `rate-limit`. Mode: deterministic local
//! execution. Documentation owner:
//! `docs/guide/06-pagination-and-rate-limits.md`.
//! Proving test: `tests/local_runtime.rs`.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use concord::rate_limit::{
    BoxFuture, RateLimitError, RateLimitPermit, RateLimitRequest, RateLimiter,
};

/// Deterministically grants attempts while exposing only a bounded counter.
#[derive(Clone, Default)]
pub struct CountingLimiter {
    acquisitions: Arc<AtomicUsize>,
}

impl CountingLimiter {
    /// Creates an empty counter.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of completed acquisition calls.
    #[must_use]
    pub fn acquisitions(&self) -> usize {
        self.acquisitions.load(Ordering::SeqCst)
    }
}

impl RateLimiter for CountingLimiter {
    fn acquire<'a>(
        &'a self,
        _request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        self.acquisitions.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(RateLimitPermit::granted()) })
    }
}

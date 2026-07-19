//! Stable advanced rate-limit contracts and private execution support.

mod error;
#[cfg(feature = "rate-limit-governor")]
mod governor;
mod limiter;
mod types;

pub use error::RateLimitError;
#[cfg(feature = "rate-limit-governor")]
pub use governor::GovernorRateLimiter;
pub use limiter::{
    Cooldown, CooldownScope, DefaultRateLimitObserver, NoopRateLimiter, RateLimitObserver,
    RateLimitRequest, RateLimitResponse, RateLimitResponseHead, RateLimiter,
};
pub use types::{
    RateLimitId, RateLimitKey, RateLimitKeyPart, RateLimitPermit, RateLimitPlan, RateLimitRule,
    RateLimitWindow,
};

#[cfg(not(feature = "rate-limit-governor"))]
pub(crate) use limiter::fallback_limiter;
pub(crate) use limiter::preparation_key_limit;

use std::sync::Arc;
use std::time::Duration;

tokio::task_local! {
    static OBSERVATION_COOLDOWN_CAP: Duration;
}

pub(crate) async fn observe_with_cooldown_cap<'a>(
    limiter: &'a Arc<dyn RateLimiter>,
    response: RateLimitResponse<'a>,
    cap: Duration,
) {
    OBSERVATION_COOLDOWN_CAP
        .scope(cap, async move { limiter.observe(response).await })
        .await;
}

#[cfg(feature = "rate-limit-governor")]
pub(super) fn observation_cooldown_cap() -> Duration {
    OBSERVATION_COOLDOWN_CAP
        .try_with(|cap| *cap)
        .unwrap_or(Duration::from_secs(60))
}

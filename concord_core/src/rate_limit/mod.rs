//! Stable advanced rate-limit contracts and private execution support.

mod error;
mod limiter;
mod types;

pub use error::RateLimitError;
pub use limiter::{
    Cooldown, CooldownScope, DEFAULT_RATE_LIMIT_KEY_BYTES, NoopRateLimiter, RateLimitObserver,
    RateLimitRequest, RateLimitResponse, RateLimitResponseHead, RateLimiter,
};
pub use types::{
    RateLimitId, RateLimitKey, RateLimitKeyPart, RateLimitPermit, RateLimitPlan, RateLimitRule,
    RateLimitWindow,
};

pub(crate) use limiter::fallback_limiter;

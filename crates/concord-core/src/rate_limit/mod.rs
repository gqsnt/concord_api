//! Stable advanced rate-limit contracts and private execution support.

mod error;
mod limiter;
mod types;

pub use crate::limits::{
    MAX_RATE_LIMIT_ID_BYTES, MAX_RATE_LIMIT_KEY_BYTES, MAX_RATE_LIMIT_KEY_PART_NAME_BYTES,
    MAX_RATE_LIMIT_KEY_PARTS, MAX_RATE_LIMIT_RULES_PER_PLAN, MAX_RATE_LIMIT_WINDOWS_PER_RULE,
};
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

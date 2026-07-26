//! Stable rate-limit contracts and private execution support.
//!
//! One bucket is identified by a selected limiter object, rule ID,
//! ordered resolved key, and window. Resolved key values may be operationally
//! sensitive: plans and keys intentionally have no `Debug` or `Display`, and
//! core does not copy their values into lifecycle errors, user hook events, or
//! completion events.

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

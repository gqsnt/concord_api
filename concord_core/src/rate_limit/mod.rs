mod context;
mod limiter;
mod plan;
mod response;

#[cfg(feature = "rate-limit-governor")]
mod governor_runtime;

pub use context::{RateLimitContext, RateLimitPermit, RateLimitResponseContext};
#[cfg(feature = "rate-limit-governor")]
pub use governor_runtime::{DefaultRateLimiter, GovernorRateLimiter};
pub use limiter::{NoopRateLimiter, RateLimiter};
pub use plan::{
    RateLimitBucketId, RateLimitBucketUse, RateLimitKey, RateLimitKeyPart, RateLimitKeyValue,
    RateLimitPlan, RateLimitSetting, RateLimitWindow,
};
pub use response::{
    DefaultRateLimitResponsePolicy, RateLimitObservation, RateLimitObserver,
    RateLimitResponseAction, RateLimitResponsePolicy, RateLimitScopeHint, RateLimitTarget,
    parse_retry_after,
};

#[cfg(not(feature = "rate-limit-governor"))]
pub type DefaultRateLimiter = NoopRateLimiter;
#[cfg(not(feature = "rate-limit-governor"))]
pub type GovernorRateLimiter = NoopRateLimiter;

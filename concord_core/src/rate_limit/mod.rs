mod context;
mod error;
mod limiter;
mod plan;
mod response;

#[cfg(feature = "rate-limit-governor")]
mod governor_runtime;

#[cfg(not(feature = "rate-limit-governor"))]
mod no_governor_default;

pub use context::{RateLimitContext, RateLimitPermit, RateLimitResponseContext};
pub use error::{RateLimitError, RateLimitErrorKind};
#[cfg(feature = "rate-limit-governor")]
pub use governor_runtime::{DefaultRateLimiter, GovernorRateLimiter};
pub use limiter::{NoopRateLimiter, RateLimitFuture, RateLimiter};
#[cfg(not(feature = "rate-limit-governor"))]
pub use no_governor_default::{DefaultRateLimiter, GovernorRateLimiter};
pub use plan::{
    RateLimitBucketId, RateLimitBucketUse, RateLimitKey, RateLimitKeyPart, RateLimitKeyValue,
    RateLimitPlan, RateLimitSetting, RateLimitWindow,
};
#[allow(unused_imports)]
pub use response::RateLimitTarget;
pub use response::{
    DefaultRateLimitResponsePolicy, RateLimitObservation, RateLimitObserver,
    RateLimitResponseAction, RateLimitResponsePolicy, RateLimitScopeHint, parse_retry_after,
};

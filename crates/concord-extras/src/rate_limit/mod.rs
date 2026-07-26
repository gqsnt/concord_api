//! Bounded first-party rate-limit policies built on core semantic contracts.

mod local;
mod observer;

pub use local::LocalRateLimiter;
pub use observer::RetryAfterObserver;

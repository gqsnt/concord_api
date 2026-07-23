//! Bounded first-party rate-limit policies built on core semantic contracts.

mod governor;
mod observer;

pub use governor::GovernorRateLimiter;
pub use observer::DefaultRateLimitObserver;

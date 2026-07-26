//! Bounded first-party rate-limit policies built on core semantic contracts.
//!
//! [`LocalRateLimiter`] is a continuously refilling token-bucket policy whose
//! quota and cooldown state belongs to one object in one process. It preserves
//! active history under capacity pressure and returns recoverable
//! `RateLimitErrorKind::StateCapacity` for a new key that does not fit.
//! [`RetryAfterObserver`] is its built-in final-429 observer.

mod local;
mod observer;

pub use local::LocalRateLimiter;
pub use observer::RetryAfterObserver;

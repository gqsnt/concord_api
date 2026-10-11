//! Bounded first-party rate-limit policies built on core semantic contracts.
//!
//! [`LocalRateLimiter`] is a continuously refilling token-bucket policy whose
//! quota and cooldown state belongs to one object in one process. It preserves
//! active history under capacity pressure and returns recoverable
//! `RateLimitErrorKind::StateCapacity` for a new key that does not fit.
//! Typed final-429 server delays are applied to future admission without
//! exposing response headers to limiter implementations.
//!
//! With the opt-in `rate-limit-conformance` feature, `conformance` provides a
//! deterministic in-memory shared-persistent adapter that demonstrates and
//! tests the schema-registration contract (RATE-05). It is a test fixture,
//! not a distributed limiter.

#[cfg(feature = "rate-limit-conformance")]
pub mod conformance;
mod local;

pub use local::LocalRateLimiter;

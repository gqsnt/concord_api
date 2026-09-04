//! Lexical rate plans plus downstream limiter implementations.
//!
//! Required feature: `rate-limit`. Mode: construction and
//! deterministic local execution. Documentation owner: `docs/rate_limits.md`.
//! Proving tests: `tests/construction.rs` and `tests/local_runtime.rs`.

pub mod custom_limiter;
pub mod lexical;
pub mod partitioning;

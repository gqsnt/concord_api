//! Lexical rate plans plus downstream limiter and observer implementations.
//!
//! Required feature: `rate-limit-governor`. Mode: construction and
//! deterministic local execution. Documentation owner:
//! `docs/guide/rate-limiting.md`. Proving tests:
//! `tests/construction.rs` and `tests/local_runtime.rs`.

pub mod custom_limiter;
pub mod lexical;
pub mod observer;

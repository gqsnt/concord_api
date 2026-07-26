//! Lexical rate plans plus downstream limiter and observer implementations.
//!
//! Required feature: `rate-limit`. Mode: construction and
//! deterministic local execution. Documentation owner:
//! `docs/guide/06-pagination-and-rate-limits.md`. Proving tests:
//! `tests/construction.rs` and `tests/local_runtime.rs`.
//!
//! Generated declaration rule IDs are deterministic within each example
//! client contract; examples share limiter objects only when sharing state is
//! intentional.

pub mod custom_limiter;
pub mod lexical;
pub mod observer;
pub mod partitioning;

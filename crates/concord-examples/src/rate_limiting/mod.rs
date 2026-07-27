//! Lexical rate plans plus downstream limiter and observer implementations.
//!
//! Required feature: `rate-limit`. Mode: construction and
//! deterministic local execution. Documentation owner:
//! `docs/features/rate-limiting.md`. Proving tests:
//! `tests/construction.rs` and `tests/local_runtime.rs`.
//!
//! Derived declaration rule IDs are deterministic within each example client
//! contract. Explicit IDs remain exact; examples share limiter objects only
//! when sharing state is intentional and schemas are compatible.

pub mod custom_limiter;
pub mod lexical;
pub mod observer;
pub mod partitioning;

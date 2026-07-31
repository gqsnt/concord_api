//! First-contact examples for a minimal declaration, generated API, and real
//! managed request execution.
//!
//! Required features: none. Mode: inert construction plus deterministic
//! loopback execution. Documentation owner: `docs/quick_start.md`.
//! Proving tests: `tests/construction.rs` and
//! `tests/local_runtime.rs`.

pub mod minimal;

#[cfg(feature = "json")]
pub mod domain_tree;

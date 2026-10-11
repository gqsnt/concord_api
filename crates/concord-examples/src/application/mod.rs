//! Application integration worked examples (§23) over the canonical Catalog.
//!
//! Required features: `json` and `rate-limit`; building the fixed-HTTPS
//! Catalog also needs `default-tls`. Mode: deterministic TLS loopback
//! execution through an explicit CONNECT proxy. Documentation owners:
//! `docs/overview.md`, `docs/pagination.md`, `docs/auth.md` and
//! `docs/generated_client.md`.
//! Proving test: `tests/application.rs`.
//!
//! Each module is one ordinary caller job:
//!
//! - [`construction`]: build the client and request explicit session
//!   installation, examining the [`concord::StoreOutcome`] (§23.1).
//! - [`execute`]: one generic helper applying common call options to any
//!   generated request or terminal wrapper (§23.2).
//! - [`saved_state`]: exact saved absence versus "no new opinion" CLI input
//!   (§23.3).
//! - [`search`]: application-controlled traversal that never reports a page
//!   bound as "not found" (§23.4).
//! - [`export`]: a bounded stream prefix that is not EOF (§23.5).
//! - [`reconciliation`]: a write helper that keeps the durable command
//!   identifier for reconciliation and formats errors without leaking it
//!   (§23.6).

pub mod construction;
pub mod execute;
pub mod export;
pub mod reconciliation;
pub mod saved_state;
pub mod search;

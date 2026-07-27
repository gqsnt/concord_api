#![forbid(unsafe_code)]
//! Importable, compile-checked examples for Concord's public product surface.
//!
//! Each capability module owns its declaration, semantic extension examples,
//! construction or local-runtime usage, documentation pointer, and acceptance
//! test pointer. The targets under `examples/` only call these modules.
//!
//! Generated examples keep constructor/navigation names and use the public
//! `set_*`, absence-capable `clear_*`, and declaration-default `reset_*`
//! value-control model. The capabilities are independent: the full DSL
//! example exercises all three controls on defaulted `Option` values and the
//! ordinary-value `= default` shorthand.
//!
//! Transport compression features are intentionally not separate examples:
//! gzip, Brotli, and deflate do not alter a declaration or generated request
//! surface, so their canonical proofs remain transport-owned. Internal
//! performance tooling exercises only public behavior and is likewise omitted
//! from the product narrative.

pub mod contracts;
pub mod getting_started;
pub mod recipes;
pub mod request_response;
pub mod runtime_endpoints;
pub mod runtime_policy;

#[cfg(feature = "oauth2-client")]
pub mod authentication;

#[cfg(feature = "json")]
pub mod pagination;

#[cfg(feature = "rate-limit")]
pub mod rate_limiting;

#[cfg(all(feature = "json", feature = "rate-limit"))]
pub mod riot;

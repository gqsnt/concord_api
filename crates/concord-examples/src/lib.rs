#![forbid(unsafe_code)]
//! Importable, compile-checked examples for Concord's public product surface.
//!
//! Each capability module owns its declaration, semantic extension examples,
//! construction or local-runtime usage, documentation pointer, and acceptance
//! test pointer. The targets under `examples/` only call these modules.
//!
//! Generated examples keep constructor/navigation names and use the v3
//! value-control matrix: `set_*` sets a payload, `replace_*` takes an exact
//! `Option` (so `None` means absence), `set_*_if_some` expresses "no new
//! opinion" for `None`, `omit_*` removes an optional value and `reset_*`
//! restores a retained declaration initializer. Required uninitialized scope
//! and endpoint inputs are positional constructor arguments.
//!
//! [`contracts::catalog_v3`] is the canonical extended Catalog of the v3
//! specification (§22) and [`application`] holds the §23 application
//! integration examples built on it.
//!
//! Transport compression features are intentionally not separate examples:
//! gzip, Brotli, and deflate do not alter a declaration or generated request
//! surface, so their canonical proofs remain transport-owned. Internal
//! performance tooling exercises only public behavior and is likewise omitted
//! from the product narrative.

pub mod contracts;
#[cfg(all(feature = "json", feature = "rate-limit", feature = "oauth2-client"))]
pub mod docs_spec;
#[cfg(all(feature = "json", feature = "rate-limit", feature = "oauth2-client"))]
pub use docs_spec::DEFAULT_LABEL;
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
pub mod application;

#[cfg(all(feature = "json", feature = "rate-limit"))]
pub mod riot;

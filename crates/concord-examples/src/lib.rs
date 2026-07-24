#![forbid(unsafe_code)]
//! Importable, compile-checked examples for Concord's public product surface.
//!
//! Each capability module owns its declaration, semantic extension examples,
//! construction or local-runtime usage, documentation pointer, and acceptance
//! test pointer. The binaries under `src/bin` only call these modules.
//!
//! Transport compression features are intentionally not separate examples:
//! gzip, Brotli, and deflate do not alter a declaration or generated request
//! surface, so their canonical proofs remain transport-owned. Internal
//! benchmark instrumentation is not a public application capability and is
//! likewise omitted from the product narrative.

pub mod contracts;
pub mod getting_started;
pub mod request_response;
pub mod runtime_endpoints;

#[cfg(feature = "oauth2-client")]
pub mod authentication;

#[cfg(any(feature = "dangerous-custom-retry", feature = "dangerous-raw-response"))]
pub mod dangerous;

#[cfg(feature = "json")]
pub mod pagination;

#[cfg(feature = "rate-limit-governor")]
pub mod rate_limiting;

#[cfg(all(feature = "json", feature = "rate-limit-governor"))]
pub mod riot;

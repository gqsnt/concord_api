//! Contract-language examples from the full grammar to regional scope composition.
//!
//! Required features vary by child module. Modes are construction-only or
//! deterministic local execution. Documentation owners and proving tests are
//! listed on each child module. The examples distinguish domain-named
//! construction/navigation from explicit `set_*` value mutation.
//!
//! [`catalog_v3`] is the canonical extended V3 vertical fixture (§22): root
//! defaults, direct placement, repetition, inherited fallback, exact body
//! input, auth replacement, explicit rate schemas, head-only absence, byte
//! exposure, custom envelope traversal and streaming.

pub mod explicit_endpoints;
pub mod full_dsl;

#[cfg(all(feature = "json", feature = "rate-limit"))]
pub mod catalog_v3;

#[cfg(feature = "json")]
pub mod regional_scopes;

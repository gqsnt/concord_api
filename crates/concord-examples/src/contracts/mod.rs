//! Contract-language examples from the full grammar to regional scope composition.
//!
//! Required features vary by child module. Modes are construction-only or
//! deterministic local execution. Documentation owners and proving tests are
//! listed on each child module. The examples distinguish domain-named
//! construction/navigation from explicit `set_*` value mutation.

pub mod explicit_endpoints;
pub mod full_dsl;

#[cfg(feature = "json")]
pub mod regional_scopes;

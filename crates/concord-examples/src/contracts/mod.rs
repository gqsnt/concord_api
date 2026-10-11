//! Contract-language examples from the full grammar to regional scope composition.
//!
//! Required features vary by child module. Modes are construction-only or
//! deterministic local execution. Documentation owners and proving tests are
//! listed on each child module. The examples distinguish domain-named
//! construction/navigation from explicit `set_*` value mutation.
//!
//! Two Catalog declarations live here deliberately:
//!
//! - [`catalog_v3`] is the canonical extended v3 vertical fixture (§22): root
//!   defaults, direct placement, repetition, an inherited fallback, exact body
//!   input, auth replacement, explicit rate schemas, head-only absence, byte
//!   exposure, custom envelope traversal and streaming.
//! - [`catalog_contract`] is the meaning-preserving v3 migration of the
//!   baseline v2.1 Catalog used for state-equivalence testing (§24); it keeps
//!   the original `X-Client-Version: 2`, explicit query blocks and the legacy
//!   `"client.tenant"` rate label.

pub mod explicit_endpoints;
pub mod full_dsl;

#[cfg(all(feature = "json", feature = "rate-limit", feature = "default-tls"))]
pub mod catalog_contract;

#[cfg(all(feature = "json", feature = "rate-limit"))]
pub mod catalog_v3;

#[cfg(feature = "json")]
pub mod regional_scopes;

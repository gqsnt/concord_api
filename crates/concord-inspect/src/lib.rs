//! Versioned `concord.contract/3` structural export, snapshot check and
//! semantic diff for Concord contract descriptions (§18.3).
//!
//! This companion crate keeps JSON out of the core runtime dependency graph:
//! `concord-core` owns the value-free
//! [`ContractDescription`] and
//! this crate serializes, compares and explains it.
//!
//! ```text
//! let json = concord_inspect::export_string(TheirClient::contract());
//! let report = concord_inspect::check(TheirClient::contract(), &snapshot)?;
//! if !report.is_structurally_equal() { /* fail the build */ }
//! ```
//!
//! # Format guarantees
//!
//! * Top-level keys: `format`, `client`, `owners`, `endpoints`, `policies`,
//!   `window_sets`, `dependencies`, `diagnostics`.
//! * Every record carries a stable structural key (`path`, `id`, `name`, or
//!   `kind`+`reference`); semantic arrays keep declaration order and object
//!   keys are sorted, so exports are byte-identical across runs.
//! * Only integers and strings: durations are `{ "secs": u64, "nanos": u32 }`.
//! * No timestamps, absolute paths, runtime values, bodies, material or raw
//!   keys. Sensitive literals appear only as
//!   `{ "redacted": true, "classification": "sensitive", "location": ... }`.
//!
//! # Negative capabilities
//!
//! Export performs no network work and runs no user code: it reads a built
//! description. Two equal snapshots establish only equality of represented
//! structural facts; opaque Rust dependencies still require behavior checks,
//! and redacted literals are reported as unresolved, never as equal.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod canonical;
mod diff;
mod error;
mod explain;
mod export;
#[cfg(any(test, feature = "fixtures"))]
pub mod fixtures;

#[cfg(test)]
mod tests;

pub use canonical::to_canonical_string;
pub use diff::{
    AffectedEndpoint, Axis, Change, DIFF_FORMAT, DiffReport, Unresolved, Verdict, diff_documents,
};
pub use error::{InspectError, InspectErrorKind};
pub use explain::{explain_endpoint, explain_policy};
pub use export::{export_string, export_value, parse_document, validate_document};

/// The structural format name, re-exported from core.
pub use concord_core::inspection::CONTRACT_FORMAT;

use concord_core::inspection::ContractDescription;

/// Compares the current description against a stored `concord.contract/3`
/// snapshot (the snapshot is "before", the current description "after").
///
/// # Errors
///
/// Returns [`InspectError`] when the snapshot is not valid JSON or not a
/// `concord.contract/3` document.
pub fn check(current: &ContractDescription, snapshot: &str) -> Result<DiffReport, InspectError> {
    let before = parse_document(snapshot)?;
    let after = export_value(current);
    diff_documents(&before, &after)
}

//! The single production boundary for sanitized metadata and diagnostics.

mod headers;
mod names;
mod url;

pub use headers::{SanitizedHeaderMap, SanitizedHeaderValue, SanitizedHeaders};

#[cfg(test)]
#[path = "../../tests/security/redaction_matrix.rs"]
mod redaction_matrix;

#[cfg(test)]
#[path = "../../tests/property/safe_url.rs"]
mod safe_url;

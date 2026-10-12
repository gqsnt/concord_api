//! Value-free inspection tooling errors.

use std::fmt;

/// The category of an inspection tooling failure.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum InspectErrorKind {
    /// The input is not syntactically valid JSON.
    InvalidJson,
    /// The document's `format` is missing or not `concord.contract/3`.
    UnsupportedFormat,
    /// A required top-level key is missing or has the wrong JSON type.
    MissingKey,
    /// The document contains a floating-point number (the format is
    /// integer/string only).
    NonIntegerNumber,
    /// Two indexed records have the same effective structural key.
    DuplicateKey,
    /// An indexed record or its supplied key has an invalid JSON type.
    InvalidRecordKey,
}

/// A value-free inspection tooling error.
///
/// The detail is structural (a top-level key name or format label); document
/// contents are never echoed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InspectError {
    kind: InspectErrorKind,
    detail: &'static str,
}

impl InspectError {
    pub(crate) const fn new(kind: InspectErrorKind, detail: &'static str) -> Self {
        Self { kind, detail }
    }

    /// Returns the failure category.
    #[must_use]
    pub const fn kind(&self) -> InspectErrorKind {
        self.kind
    }
}

impl fmt::Display for InspectError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self.kind {
            InspectErrorKind::InvalidJson => "input is not valid JSON",
            InspectErrorKind::UnsupportedFormat => "document is not concord.contract/3",
            InspectErrorKind::MissingKey => "document is missing a required key",
            InspectErrorKind::NonIntegerNumber => "document contains a non-integer number",
            InspectErrorKind::DuplicateKey => "document contains duplicate indexed keys",
            InspectErrorKind::InvalidRecordKey => "document contains an invalid indexed key",
        };
        write!(formatter, "{label} ({})", self.detail)
    }
}

impl std::error::Error for InspectError {}

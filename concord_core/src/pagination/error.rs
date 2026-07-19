use std::error::Error;
use std::fmt;

use crate::error::PaginationErrorKind;

/// A safe structured failure produced by pagination control logic.
///
/// The error retains only its stable kind and an optional caller-supplied safe
/// message. Request identities, continuation values, controller state, item
/// values, credentials, bodies, and native transport diagnostics are never
/// stored.
pub struct PaginationError {
    kind: PaginationErrorKind,
    safe_message: Option<String>,
}

impl PaginationError {
    /// Creates a pagination error with no diagnostic detail.
    #[must_use]
    pub const fn new(kind: PaginationErrorKind) -> Self {
        Self {
            kind,
            safe_message: None,
        }
    }

    /// Creates a pagination error with caller-reviewed safe diagnostic text.
    #[must_use]
    pub fn with_safe_message(kind: PaginationErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            safe_message: Some(message.into()),
        }
    }

    /// Returns the stable pagination classification.
    #[must_use]
    pub const fn kind(&self) -> PaginationErrorKind {
        self.kind
    }

    pub(crate) fn safe_message(&self) -> Option<&str> {
        self.safe_message.as_deref()
    }
}

impl fmt::Display for PaginationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("pagination failed")?;
        if let Some(message) = &self.safe_message {
            write!(formatter, ": {message}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for PaginationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PaginationError")
            .field("kind", &self.kind)
            .field("safe_message", &self.safe_message)
            .finish()
    }
}

impl Error for PaginationError {}

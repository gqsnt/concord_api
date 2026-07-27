use std::error::Error;
use std::fmt;

/// A safe failure reported by an accepted-response specification.
///
/// Concord converts this value to a structural [`crate::contracts::ApiError`]
/// category and discards application text before automatic diagnostics.
pub struct ResponseBodyError {
    kind: ResponseBodyErrorKind,
    safe_message: Option<String>,
}

impl ResponseBodyError {
    /// Creates a response-body failure with structural formatting only.
    #[must_use]
    pub const fn new(kind: ResponseBodyErrorKind) -> Self {
        Self {
            kind,
            safe_message: None,
        }
    }

    /// Creates a response-body failure carrying application-approved safe text.
    ///
    /// The caller is responsible for excluding application values, response
    /// bytes, headers, parser text, credentials, and other protected data.
    /// Concord formatting remains structural and does not reproduce this text.
    #[must_use]
    pub fn with_safe_message(kind: ResponseBodyErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            safe_message: Some(message.into()),
        }
    }

    /// Returns the stable structural failure kind.
    #[must_use]
    pub const fn kind(&self) -> ResponseBodyErrorKind {
        self.kind
    }
}

impl fmt::Display for ResponseBodyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("response body operation failed")
    }
}

impl fmt::Debug for ResponseBodyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResponseBodyError")
            .field("kind", &self.kind)
            .field("has_safe_message", &self.safe_message.is_some())
            .finish()
    }
}

impl Error for ResponseBodyError {}

/// Stable structural kinds of accepted-response specification failure.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResponseBodyErrorKind {
    /// Accepted response media metadata violated the declared contract.
    InvalidMediaType,
    /// Complete buffered bytes could not be decoded safely.
    Decode,
    /// The specification could not construct a valid output.
    InvalidOutput,
    /// Concord could not classify an internal response failure more narrowly.
    Internal,
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::{ResponseBodyError, ResponseBodyErrorKind};

    #[test]
    fn formatting_and_sources_exclude_application_text() {
        let error = ResponseBodyError::with_safe_message(
            ResponseBodyErrorKind::Decode,
            "response-extension-message-sentinel-4c833dad70",
        );
        let rendered = format!("{error} {error:?}");
        assert!(!rendered.contains("response-extension-message-sentinel-4c833dad70"));
        assert!(rendered.contains("has_safe_message: true"));
        assert!(error.source().is_none());
    }
}

use std::error::Error;
use std::fmt;

use crate::error::RateLimitErrorKind;

/// A bounded, key-safe limiter or rate-limit configuration failure.
///
/// Text supplied to [`RateLimitError::with_safe_message`] is an explicit
/// application trust boundary. It must not contain resolved key values,
/// credentials, body data, native URLs, or other protected values.
pub struct RateLimitError {
    kind: RateLimitErrorKind,
    safe_message: Option<Box<str>>,
}

impl RateLimitError {
    /// Creates a structural rate-limit failure without application text.
    #[must_use]
    pub const fn new(kind: RateLimitErrorKind) -> Self {
        Self {
            kind,
            safe_message: None,
        }
    }

    /// Creates a rate-limit failure with application-author-declared safe text.
    #[must_use]
    pub fn with_safe_message(kind: RateLimitErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            safe_message: Some(message.into().into_boxed_str()),
        }
    }

    /// Returns the stable structural rate-limit failure kind.
    #[must_use]
    pub const fn kind(&self) -> RateLimitErrorKind {
        self.kind
    }

    pub(crate) fn safe_message(&self) -> Option<&str> {
        self.safe_message.as_deref()
    }
}

impl fmt::Display for RateLimitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "rate-limit operation failed ({:?})", self.kind)?;
        if let Some(message) = &self.safe_message {
            write!(formatter, ": {message}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for RateLimitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RateLimitError")
            .field("kind", &self.kind)
            .field("safe_message", &self.safe_message)
            .finish()
    }
}

impl Error for RateLimitError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatting_is_structural_and_has_no_recursive_source() {
        const KEY_SENTINEL: &str = "rate-key-private-8b7d67d6";
        let structural = RateLimitError::new(RateLimitErrorKind::Acquire);
        assert_eq!(structural.kind(), RateLimitErrorKind::Acquire);
        assert!(structural.source().is_none());
        assert!(!structural.to_string().contains(KEY_SENTINEL));
        assert!(!format!("{structural:?}").contains(KEY_SENTINEL));

        let safe = RateLimitError::with_safe_message(
            RateLimitErrorKind::Internal,
            "operator-approved limiter failure",
        );
        assert!(
            safe.to_string()
                .contains("operator-approved limiter failure")
        );
        assert!(safe.source().is_none());
    }
}

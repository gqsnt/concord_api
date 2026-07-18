use std::error::Error;
use std::fmt;

/// A safe failure while inspecting, constructing, or producing a request body.
///
/// The optional message accepted by [`BodyError::with_safe_message`] is a
/// trusted downstream extension boundary. Implementors must provide only text
/// that is safe for ordinary application diagnostics. Concord never copies a
/// producer error message or request-body data into this value.
pub struct BodyError {
    kind: BodyErrorKind,
    safe_message: Option<String>,
}

impl BodyError {
    /// Creates a body failure with structural formatting only.
    #[must_use]
    pub const fn new(kind: BodyErrorKind) -> Self {
        Self {
            kind,
            safe_message: None,
        }
    }

    /// Creates a body failure with application-authored diagnostic text.
    ///
    /// The caller is responsible for ensuring that `message` contains no body
    /// data, producer text, filename, credential, or other protected value.
    #[must_use]
    pub fn with_safe_message(kind: BodyErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            safe_message: Some(message.into()),
        }
    }

    /// Returns the stable structural body failure kind.
    #[must_use]
    pub const fn kind(&self) -> BodyErrorKind {
        self.kind
    }
}

impl fmt::Display for BodyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("request body operation failed")?;
        if let Some(message) = &self.safe_message {
            write!(formatter, ": {message}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for BodyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BodyError")
            .field("kind", &self.kind)
            .field("safe_message", &self.safe_message)
            .finish()
    }
}

impl Error for BodyError {}

/// Stable structural kinds of request-body failure.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BodyErrorKind {
    /// A declared media type is not a valid concrete HTTP media type.
    InvalidContentType,
    /// Exact-length metadata conflicts with the produced byte sequence.
    InvalidLength,
    /// A downstream stream or asynchronous reader failed.
    Producer,
    /// The configured aggregate request-body limit was exceeded.
    LimitExceeded,
    /// A reconstructible body factory failed to create a fresh body.
    Factory,
    /// Multipart validation or native recipe construction failed.
    Multipart,
    /// Concord could not safely classify an internal body failure more narrowly.
    Internal,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structural_and_trusted_message_errors_have_no_source() {
        let structural = BodyError::new(BodyErrorKind::Producer);
        assert_eq!(structural.kind(), BodyErrorKind::Producer);
        assert_eq!(structural.to_string(), "request body operation failed");
        assert!(structural.source().is_none());
        assert!(!format!("{structural:?}").contains("producer-authored-secret"));

        let safe = BodyError::with_safe_message(
            BodyErrorKind::Factory,
            "configured body recipe is unavailable",
        );
        assert_eq!(safe.kind(), BodyErrorKind::Factory);
        assert!(
            safe.to_string()
                .contains("configured body recipe is unavailable")
        );
        assert!(safe.source().is_none());
    }
}

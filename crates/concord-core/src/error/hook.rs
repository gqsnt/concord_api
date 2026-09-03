use std::error::Error;
use std::fmt;

/// An application-owned safe rejection returned by a decision gate.
///
/// The supplied message is treated as safe application text. It must not contain
/// credentials, body bytes, native URLs, or other protected values.
pub struct GateError {
    safe_message: String,
}

impl GateError {
    /// Creates a gate rejection from application-owned safe text.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            safe_message: message.into(),
        }
    }
}

impl fmt::Display for GateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.safe_message)
    }
}

impl fmt::Debug for GateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GateError")
            .field("safe_message", &self.safe_message)
            .finish()
    }
}

impl Error for GateError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_message_has_no_hidden_source_or_unrelated_context() {
        let error = GateError::new("application-approved gate veto");
        assert_eq!(error.to_string(), "application-approved gate veto");
        assert_eq!(
            format!("{error:?}"),
            "GateError { safe_message: \"application-approved gate veto\" }"
        );
        assert!(error.source().is_none());
    }
}

use std::error::Error;
use std::fmt;

/// An application-owned safe rejection returned by a request lifecycle hook.
///
/// The supplied message is treated as safe application text. It must not contain
/// credentials, body bytes, native URLs, or other protected values.
pub struct HookError {
    safe_message: String,
}

impl HookError {
    /// Creates a hook rejection from application-owned safe text.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            safe_message: message.into(),
        }
    }
}

impl fmt::Display for HookError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.safe_message)
    }
}

impl fmt::Debug for HookError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HookError")
            .field("safe_message", &self.safe_message)
            .finish()
    }
}

impl Error for HookError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_message_has_no_hidden_source_or_unrelated_context() {
        let error = HookError::new("application-approved hook veto");
        assert_eq!(error.to_string(), "application-approved hook veto");
        assert_eq!(
            format!("{error:?}"),
            "HookError { safe_message: \"application-approved hook veto\" }"
        );
        assert!(error.source().is_none());
    }
}

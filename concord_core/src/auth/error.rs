use std::error::Error;
use std::fmt;
use std::time::Duration;

use crate::error::AuthErrorKind;

/// A bounded, secret-safe credential-provider or credential-state failure.
pub struct AuthError {
    kind: AuthErrorKind,
    safe_message: Option<Box<str>>,
    retry_after: Option<Duration>,
}

impl AuthError {
    /// Creates an authentication error without provider text.
    #[must_use]
    pub fn new(kind: AuthErrorKind) -> Self {
        Self {
            kind,
            safe_message: None,
            retry_after: None,
        }
    }

    /// Creates an authentication error with provider-author-declared safe text.
    #[must_use]
    pub fn with_safe_message(kind: AuthErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            safe_message: Some(message.into().into_boxed_str()),
            retry_after: None,
        }
    }

    /// Attaches a provider-directed retry delay.
    #[must_use]
    pub fn with_retry_after(mut self, duration: Duration) -> Self {
        self.retry_after = Some(duration);
        self
    }

    /// Returns the stable authentication failure kind.
    #[must_use]
    pub const fn kind(&self) -> AuthErrorKind {
        self.kind
    }

    /// Returns the optional provider-directed retry delay.
    #[must_use]
    pub const fn retry_after(&self) -> Option<Duration> {
        self.retry_after
    }

    pub(crate) fn clone_internal(&self) -> Self {
        Self {
            kind: self.kind,
            safe_message: self.safe_message.clone(),
            retry_after: self.retry_after,
        }
    }

    pub(crate) fn safe_message(&self) -> Option<&str> {
        self.safe_message.as_deref()
    }
}

impl fmt::Display for AuthError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "authentication failed ({:?})", self.kind)?;
        if let Some(message) = &self.safe_message {
            write!(formatter, ": {message}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for AuthError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthError")
            .field("kind", &self.kind)
            .field("safe_message", &self.safe_message)
            .field("retry_after", &self.retry_after)
            .finish()
    }
}

impl Error for AuthError {}

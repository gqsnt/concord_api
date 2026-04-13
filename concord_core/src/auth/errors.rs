use std::time::Duration;
use thiserror::Error;

#[derive(Clone, Debug, Error)]
#[error("{kind:?}: {message}")]
pub struct AuthError {
    pub kind: AuthErrorKind,
    pub message: String,
    pub retry_after: Option<Duration>,
}

impl AuthError {
    #[inline]
    pub fn new(kind: AuthErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            retry_after: None,
        }
    }

    #[inline]
    pub fn with_retry_after(mut self, retry_after: Duration) -> Self {
        self.retry_after = Some(retry_after);
        self
    }

    #[inline]
    pub fn retry_after(&self) -> Option<Duration> {
        self.retry_after
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthErrorKind {
    MissingCredential,
    AcquireFailed,
    RefreshFailed,
    RejectedCredential,
    UnsupportedScheme,
    RecursionDetected,
    ProviderRejected,
    StateUnavailable,
    CertificateUnavailable,
    InvalidConfiguration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialRefreshReason {
    Missing,
    Expired,
    ExpiringSoon,
    Rejected,
    Manual,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvalidateReason {
    Unauthorized,
    Forbidden,
    Manual,
    ProviderRejected,
}

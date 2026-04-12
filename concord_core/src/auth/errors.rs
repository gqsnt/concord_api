use thiserror::Error;

#[derive(Clone, Debug, Error)]
#[error("{kind:?}: {message}")]
pub struct AuthError {
    pub kind: AuthErrorKind,
    pub message: String,
}

impl AuthError {
    #[inline]
    pub fn new(kind: AuthErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
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

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

    #[inline]
    pub fn state_unavailable(message: impl Into<String>) -> Self {
        Self::new(AuthErrorKind::StateUnavailable, message)
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
    ResponseTooLarge,
    ResponseBody,
    StateUnavailable,
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

#[cfg(test)]
fn read_auth_lock<'a, T>(
    lock: &'a std::sync::RwLock<T>,
    message: &'static str,
) -> Result<std::sync::RwLockReadGuard<'a, T>, AuthError> {
    lock.read()
        .map_err(|_| AuthError::state_unavailable(message))
}

pub fn write_auth_lock<'a, T>(
    lock: &'a std::sync::RwLock<T>,
    message: &'static str,
) -> Result<std::sync::RwLockWriteGuard<'a, T>, AuthError> {
    lock.write()
        .map_err(|_| AuthError::state_unavailable(message))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::panic::AssertUnwindSafe;

    fn poison<T>(lock: &std::sync::RwLock<T>) {
        let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let _guard = lock.write().expect("test lock should be available");
            panic!("poison test lock");
        }));
    }

    #[test]
    fn poisoned_generated_auth_vars_lock_returns_typed_error() {
        let lock = std::sync::RwLock::new(1_u8);
        poison(&lock);

        let err = read_auth_lock(&lock, "auth vars lock poisoned")
            .expect_err("poisoned auth vars lock should return typed error");
        assert_eq!(err.kind, AuthErrorKind::StateUnavailable);
        assert!(err.to_string().contains("auth vars lock poisoned"));

        let err = write_auth_lock(&lock, "auth vars lock poisoned")
            .expect_err("poisoned auth vars lock should return typed error");
        assert_eq!(err.kind, AuthErrorKind::StateUnavailable);
        assert!(err.to_string().contains("auth vars lock poisoned"));
    }
}

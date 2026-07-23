use std::fmt;
use std::time::Instant;

/// A string whose ordinary formatting never reveals its contents.
#[derive(Clone)]
pub struct SecretString {
    value: String,
}

impl SecretString {
    /// Wraps one protected string.
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
        }
    }

    /// Explicitly exposes the protected value.
    #[must_use]
    pub fn expose_secret(&self) -> &str {
        &self.value
    }
}

impl From<String> for SecretString {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for SecretString {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl fmt::Display for SecretString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<redacted>")
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretString(<redacted>)")
    }
}

/// Credential material retained by a Concord credential slot.
///
/// Values may be cloned and read by overlapping credential operations. They are
/// sensitive application-owned state: implementations are responsible for safe
/// `Debug`/`Display`, logging, storage, retention, and destruction behavior.
/// Synchronous methods cannot be preempted; implementations must bound CPU,
/// memory, maps, queues, tasks, metric cardinality, retries, and external I/O and
/// must not assume Concord supplies automatic limits for custom state. Concord
/// checks operation deadlines around the credential lifecycle, while cancellation
/// prevents later Concord work without rolling back completed extension effects.
pub trait CredentialMaterial: Clone + Send + Sync + 'static {
    /// Returns the provider-declared expiration instant, when present.
    fn expires_at(&self) -> Option<Instant> {
        None
    }
}

/// Single-string credential material suitable for bearer, header, or query placement.
///
/// `expose_secret` is invoked only at the native authentication boundary, but the
/// implementation remains responsible for protecting the returned value in its
/// own memory, logging, storage, metrics, and synchronous work. The concurrency,
/// cancellation, deadline, and boundedness obligations of [`CredentialMaterial`]
/// continue to apply.
pub trait SecretCredential: CredentialMaterial {
    /// Explicitly exposes the value at the native authentication boundary.
    fn expose_secret(&self) -> &str;
}

/// Built-in bearer access-token material.
#[derive(Clone)]
pub struct AccessToken {
    token: SecretString,
    expires_at: Option<Instant>,
}

impl AccessToken {
    /// Creates a token without provider-declared expiration.
    pub fn new(token: impl Into<SecretString>) -> Self {
        Self {
            token: token.into(),
            expires_at: None,
        }
    }

    /// Creates a token with one monotonic expiration instant.
    pub fn with_expiration(token: impl Into<SecretString>, expires_at: Instant) -> Self {
        Self {
            token: token.into(),
            expires_at: Some(expires_at),
        }
    }
}

impl CredentialMaterial for AccessToken {
    fn expires_at(&self) -> Option<Instant> {
        self.expires_at
    }
}

impl SecretCredential for AccessToken {
    fn expose_secret(&self) -> &str {
        self.token.expose_secret()
    }
}

/// Built-in API-key material.
#[derive(Clone)]
pub struct ApiKey {
    value: SecretString,
}

impl ApiKey {
    /// Creates one API key.
    pub fn new(value: impl Into<SecretString>) -> Self {
        Self {
            value: value.into(),
        }
    }
}

impl CredentialMaterial for ApiKey {}

impl SecretCredential for ApiKey {
    fn expose_secret(&self) -> &str {
        self.value.expose_secret()
    }
}

/// Built-in atomic Basic username/password material.
#[derive(Clone)]
pub struct BasicCredential {
    username: SecretString,
    password: SecretString,
}

impl BasicCredential {
    /// Creates one atomic Basic credential pair.
    pub fn new(username: impl Into<SecretString>, password: impl Into<SecretString>) -> Self {
        Self {
            username: username.into(),
            password: password.into(),
        }
    }

    pub(crate) fn username(&self) -> &str {
        self.username.expose_secret()
    }

    pub(crate) fn password(&self) -> &str {
        self.password.expose_secret()
    }
}

impl CredentialMaterial for BasicCredential {}

macro_rules! redacted_material_format {
    ($type:ty, $name:literal) => {
        impl fmt::Display for $type {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(concat!($name, "(<redacted>)"))
            }
        }

        impl fmt::Debug for $type {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(concat!($name, "(<redacted>)"))
            }
        }
    };
}

redacted_material_format!(AccessToken, "AccessToken");
redacted_material_format!(ApiKey, "ApiKey");
redacted_material_format!(BasicCredential, "BasicCredential");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_material_formatting_is_redacted() {
        const SENTINEL: &str = "credential-material-sentinel-5874c229f7b04e59";
        let values: [&dyn fmt::Debug; 4] = [
            &SecretString::new(SENTINEL),
            &AccessToken::new(SENTINEL),
            &ApiKey::new(SENTINEL),
            &BasicCredential::new(SENTINEL, SENTINEL),
        ];
        for value in values {
            assert!(!format!("{value:?}").contains(SENTINEL));
        }
    }
}

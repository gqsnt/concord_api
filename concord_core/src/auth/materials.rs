use super::credentials::{CredentialMaterial, SecretCredential};
use crate::secret::SecretString;
use serde::Deserialize;
use std::time::Instant;

#[derive(Clone, Debug)]
pub struct AccessToken {
    pub token: SecretString,
    pub expires_at: Option<Instant>,
    pub refresh_token: Option<SecretString>,
    pub scope: Vec<String>,
    pub audience: Option<String>,
}

impl AccessToken {
    #[inline]
    pub fn new(token: impl Into<SecretString>) -> Self {
        Self {
            token: token.into(),
            expires_at: None,
            refresh_token: None,
            scope: Vec::new(),
            audience: None,
        }
    }

    #[inline]
    pub fn expires_at(mut self, expires_at: Instant) -> Self {
        self.expires_at = Some(expires_at);
        self
    }
}

impl CredentialMaterial for AccessToken {
    fn expires_at(&self) -> Option<Instant> {
        self.expires_at
    }
}

impl SecretCredential for AccessToken {
    fn secret_value(&self) -> &str {
        self.token.expose_secret()
    }
}

impl<'de> Deserialize<'de> for AccessToken {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct AccessTokenPayload {
            #[serde(rename = "access_token")]
            token: SecretString,
            #[serde(default)]
            refresh_token: Option<SecretString>,
            #[serde(default)]
            scope: Vec<String>,
            #[serde(default)]
            audience: Option<String>,
        }

        let payload = AccessTokenPayload::deserialize(deserializer)?;
        Ok(AccessToken {
            token: payload.token,
            expires_at: None,
            refresh_token: payload.refresh_token,
            scope: payload.scope,
            audience: payload.audience,
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct ApiKey {
    pub value: SecretString,
}

impl ApiKey {
    #[inline]
    pub fn new(value: impl Into<SecretString>) -> Self {
        Self {
            value: value.into(),
        }
    }
}

impl CredentialMaterial for ApiKey {}

impl SecretCredential for ApiKey {
    fn secret_value(&self) -> &str {
        self.value.expose_secret()
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct BasicCredential {
    pub username: SecretString,
    pub password: SecretString,
}

impl BasicCredential {
    #[inline]
    pub fn new(username: impl Into<SecretString>, password: impl Into<SecretString>) -> Self {
        Self {
            username: username.into(),
            password: password.into(),
        }
    }
}

impl CredentialMaterial for BasicCredential {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_debug_redacts_username_and_password() {
        let credential = BasicCredential::new(
            "BASIC_USERNAME_SENTINEL_DO_NOT_DEBUG",
            "BASIC_PASSWORD_SENTINEL_DO_NOT_DEBUG",
        );
        let rendered = format!("{credential:?}");

        assert!(!rendered.contains("BASIC_USERNAME_SENTINEL_DO_NOT_DEBUG"));
        assert!(!rendered.contains("BASIC_PASSWORD_SENTINEL_DO_NOT_DEBUG"));
    }
}

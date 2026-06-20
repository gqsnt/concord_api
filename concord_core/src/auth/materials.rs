use super::credentials::{CredentialMaterial, SecretCredential};
use super::ids::AuthIdentity;
use super::util::hash_secret;
use crate::secret::SecretString;
use std::time::Instant;

#[derive(Clone, Debug)]
pub struct AccessToken {
    pub token: SecretString,
    pub expires_at: Option<Instant>,
    pub refresh_token: Option<SecretString>,
    pub scope: Vec<String>,
    pub audience: Option<String>,
    pub identity_hint: Option<String>,
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
            identity_hint: None,
        }
    }

    #[inline]
    pub fn expires_at(mut self, expires_at: Instant) -> Self {
        self.expires_at = Some(expires_at);
        self
    }

    #[inline]
    pub fn identity_hint(mut self, hint: impl Into<String>) -> Self {
        self.identity_hint = Some(hint.into());
        self
    }
}

impl CredentialMaterial for AccessToken {
    fn expires_at(&self) -> Option<Instant> {
        self.expires_at
    }

    fn safe_identity(&self) -> AuthIdentity {
        if let Some(hint) = &self.identity_hint {
            return AuthIdentity::User(hint.clone());
        }
        if !self.scope.is_empty() || self.audience.is_some() {
            return AuthIdentity::ScopeAudience {
                scope: self.scope.clone(),
                audience: self.audience.clone(),
            };
        }
        AuthIdentity::OpaqueHash(hash_secret(self.token.expose()))
    }
}

impl SecretCredential for AccessToken {
    fn secret_value(&self) -> &str {
        self.token.expose()
    }
}

#[derive(Clone, Debug)]
pub struct ApiKey {
    pub value: SecretString,
    pub identity_hint: Option<String>,
}

impl ApiKey {
    #[inline]
    pub fn new(value: impl Into<SecretString>) -> Self {
        Self {
            value: value.into(),
            identity_hint: None,
        }
    }

    #[inline]
    pub fn identity_hint(mut self, hint: impl Into<String>) -> Self {
        self.identity_hint = Some(hint.into());
        self
    }
}

impl CredentialMaterial for ApiKey {
    fn safe_identity(&self) -> AuthIdentity {
        if let Some(hint) = &self.identity_hint {
            AuthIdentity::Tenant(hint.clone())
        } else {
            AuthIdentity::OpaqueHash(hash_secret(self.value.expose()))
        }
    }
}

impl SecretCredential for ApiKey {
    fn secret_value(&self) -> &str {
        self.value.expose()
    }
}

#[derive(Clone, Debug)]
pub struct BasicCredential {
    pub username: SecretString,
    pub password: SecretString,
    pub identity_hint: Option<String>,
}

impl BasicCredential {
    #[inline]
    pub fn new(username: impl Into<SecretString>, password: impl Into<SecretString>) -> Self {
        Self {
            username: username.into(),
            password: password.into(),
            identity_hint: None,
        }
    }

    #[inline]
    pub fn identity_hint(mut self, hint: impl Into<String>) -> Self {
        self.identity_hint = Some(hint.into());
        self
    }
}

impl CredentialMaterial for BasicCredential {
    fn safe_identity(&self) -> AuthIdentity {
        if let Some(hint) = &self.identity_hint {
            AuthIdentity::User(hint.clone())
        } else {
            AuthIdentity::OpaqueHash(hash_secret(self.username.expose()))
        }
    }
}

#[derive(Clone, Debug)]
pub struct ClientCertificate {
    pub identity_id: String,
}

impl ClientCertificate {
    #[inline]
    pub fn new(identity_id: impl Into<String>) -> Self {
        Self {
            identity_id: identity_id.into(),
        }
    }
}

impl CredentialMaterial for ClientCertificate {
    fn safe_identity(&self) -> AuthIdentity {
        AuthIdentity::OpaqueHash(hash_secret(&self.identity_id))
    }
}

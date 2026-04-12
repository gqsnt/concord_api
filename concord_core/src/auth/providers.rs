use super::credentials::{CredentialContext, CredentialProvider};
use super::errors::AuthError;
use super::future::AuthFuture;
use super::ids::CredentialId;
use super::materials::{AccessToken, ApiKey, BasicCredential};
use crate::client::ClientContext;

#[cfg(feature = "json")]
use super::errors::AuthErrorKind;
#[cfg(feature = "json")]
use super::http::{AuthHttpRequest, AuthInternalPolicy, AuthMode};
#[cfg(feature = "json")]
use crate::secret::SecretString;
#[cfg(feature = "json")]
use base64::Engine;
#[cfg(feature = "json")]
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
#[cfg(feature = "json")]
use bytes::Bytes;
#[cfg(feature = "json")]
use http::HeaderMap;
#[cfg(feature = "json")]
use http::header::{AUTHORIZATION, CONTENT_TYPE, HeaderValue};
#[cfg(feature = "json")]
use serde::Deserialize;
#[cfg(feature = "json")]
use std::time::{Duration, Instant};
#[cfg(feature = "json")]
use url::Url;

#[derive(Clone)]
pub struct StaticBearerProvider {
    id: CredentialId,
    token: AccessToken,
}

impl StaticBearerProvider {
    #[inline]
    pub fn new(id: CredentialId, token: AccessToken) -> Self {
        Self { id, token }
    }
}

impl<Cx: ClientContext> CredentialProvider<Cx> for StaticBearerProvider {
    type Credential = AccessToken;

    fn id(&self) -> CredentialId {
        self.id.clone()
    }

    fn acquire<'a>(
        &'a self,
        _ctx: CredentialContext<'a, Cx>,
    ) -> AuthFuture<'a, Result<Self::Credential, AuthError>> {
        Box::pin(async move { Ok(self.token.clone()) })
    }
}

#[derive(Clone)]
pub struct StaticApiKeyProvider {
    id: CredentialId,
    key: ApiKey,
}

impl StaticApiKeyProvider {
    #[inline]
    pub fn new(id: CredentialId, key: ApiKey) -> Self {
        Self { id, key }
    }
}

impl<Cx: ClientContext> CredentialProvider<Cx> for StaticApiKeyProvider {
    type Credential = ApiKey;

    fn id(&self) -> CredentialId {
        self.id.clone()
    }

    fn acquire<'a>(
        &'a self,
        _ctx: CredentialContext<'a, Cx>,
    ) -> AuthFuture<'a, Result<Self::Credential, AuthError>> {
        Box::pin(async move { Ok(self.key.clone()) })
    }
}

#[derive(Clone)]
pub struct StaticBasicProvider {
    id: CredentialId,
    credential: BasicCredential,
}

impl StaticBasicProvider {
    #[inline]
    pub fn new(id: CredentialId, credential: BasicCredential) -> Self {
        Self { id, credential }
    }
}

impl<Cx: ClientContext> CredentialProvider<Cx> for StaticBasicProvider {
    type Credential = BasicCredential;

    fn id(&self) -> CredentialId {
        self.id.clone()
    }

    fn acquire<'a>(
        &'a self,
        _ctx: CredentialContext<'a, Cx>,
    ) -> AuthFuture<'a, Result<Self::Credential, AuthError>> {
        Box::pin(async move { Ok(self.credential.clone()) })
    }
}

#[cfg(feature = "json")]
#[derive(Clone, Debug)]
pub struct OAuth2ClientCredentialsProvider {
    id: CredentialId,
    token_url: Url,
    client_id: SecretString,
    client_secret: SecretString,
    scope: Option<String>,
}

#[cfg(feature = "json")]
impl OAuth2ClientCredentialsProvider {
    #[inline]
    pub fn new(
        id: CredentialId,
        token_url: Url,
        client_id: impl Into<SecretString>,
        client_secret: impl Into<SecretString>,
    ) -> Self {
        Self {
            id,
            token_url,
            client_id: client_id.into(),
            client_secret: client_secret.into(),
            scope: None,
        }
    }

    #[inline]
    pub fn scope(mut self, scope: impl Into<String>) -> Self {
        self.scope = Some(scope.into());
        self
    }
}

#[cfg(feature = "json")]
impl<Cx: ClientContext> CredentialProvider<Cx> for OAuth2ClientCredentialsProvider {
    type Credential = AccessToken;

    fn id(&self) -> CredentialId {
        self.id.clone()
    }

    fn acquire<'a>(
        &'a self,
        ctx: CredentialContext<'a, Cx>,
    ) -> AuthFuture<'a, Result<Self::Credential, AuthError>> {
        Box::pin(async move {
            let mut headers = HeaderMap::new();
            let raw = format!("{}:{}", self.client_id.expose(), self.client_secret.expose());
            let basic = format!("Basic {}", BASE64_STANDARD.encode(raw));
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&basic).map_err(|_| {
                    AuthError::new(AuthErrorKind::InvalidConfiguration, "invalid client secret")
                })?,
            );
            headers.insert(
                CONTENT_TYPE,
                HeaderValue::from_static("application/x-www-form-urlencoded"),
            );

            let body = {
                let mut form = url::form_urlencoded::Serializer::new(String::new());
                form.append_pair("grant_type", "client_credentials");
                if let Some(scope) = &self.scope {
                    form.append_pair("scope", scope);
                }
                form.finish()
            };

            let resp = ctx
                .executor
                .send(AuthHttpRequest {
                    method: http::Method::POST,
                    url: self.token_url.clone(),
                    headers,
                    body: Some(Bytes::from(body.into_bytes())),
                    mode: AuthMode::SkipAuth,
                    policy: AuthInternalPolicy::default(),
                })
                .await?;

            if !resp.status.is_success() {
                return Err(AuthError::new(
                    AuthErrorKind::AcquireFailed,
                    format!("oauth2 token endpoint returned {}", resp.status),
                ));
            }

            let token: OAuth2TokenResponse = serde_json::from_slice(&resp.body).map_err(|e| {
                AuthError::new(
                    AuthErrorKind::AcquireFailed,
                    format!("oauth2 token response decode failed: {e}"),
                )
            })?;

            if let Some(token_type) = &token.token_type
                && !token_type.eq_ignore_ascii_case("bearer")
            {
                return Err(AuthError::new(
                    AuthErrorKind::UnsupportedScheme,
                    format!("unsupported oauth2 token_type {token_type}"),
                ));
            }

            let mut out = AccessToken::new(token.access_token);
            out.expires_at = token
                .expires_in
                .map(|seconds| Instant::now() + Duration::from_secs(seconds));
            out.refresh_token = token.refresh_token.map(SecretString::new);
            out.scope = token
                .scope
                .unwrap_or_default()
                .split_whitespace()
                .map(ToOwned::to_owned)
                .collect();
            Ok(out)
        })
    }
}

#[cfg(feature = "json")]
#[derive(Deserialize)]
struct OAuth2TokenResponse {
    access_token: String,
    token_type: Option<String>,
    expires_in: Option<u64>,
    refresh_token: Option<String>,
    scope: Option<String>,
}

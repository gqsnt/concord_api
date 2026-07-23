use std::error::Error;
use std::fmt;
use std::time::{Duration, Instant};

use bytes::Bytes;
use concord_core::advanced::{
    AuthError, AuthErrorKind, BoxFuture, CredentialProvider, CredentialRequest, HeaderValue,
    Method, ProviderBody, ProviderRequest, RecoveryMode, StatusCode, Url,
};
use concord_core::prelude::{AccessToken, SecretString};
use http::header::{AUTHORIZATION, CONTENT_TYPE};

/// A client-credentials provider that acquires bearer access tokens through
/// Concord's isolated, bounded provider-HTTP capability.
///
/// The provider stores the token URL and protected client credentials in one
/// immutable configuration. Provider requests remain outside application
/// hooks, application rate limiting, and application transport pools. Concord
/// bounds request and response bodies and the enclosing credential operation;
/// callers remain responsible for bounding the strings supplied to the
/// builder.
pub struct OAuth2ClientCredentials {
    token_url: Url,
    client_id: SecretString,
    client_secret: SecretString,
    scope: Option<Box<str>>,
}

impl OAuth2ClientCredentials {
    /// Starts a secret-safe named builder for one token endpoint.
    ///
    /// Invalid token URL text is discarded immediately and is never retained
    /// in the builder or its eventual error.
    #[must_use]
    pub fn builder(token_url: impl AsRef<str>) -> OAuth2ClientCredentialsBuilder {
        OAuth2ClientCredentialsBuilder::new(token_url.as_ref())
    }

    async fn exchange(&self, context: CredentialRequest<'_>) -> Result<AccessToken, AuthError> {
        let parts = self.exchange_parts()?;
        let request = ProviderRequest::new(Method::POST, self.token_url.clone())
            .header(AUTHORIZATION, parts.authorization)
            .header(
                CONTENT_TYPE,
                HeaderValue::from_static("application/x-www-form-urlencoded"),
            )
            .body(ProviderBody::Bytes(parts.body));
        let response = context.http.send(request).await?;
        decode_token_response(response.status(), response.body())
    }

    fn exchange_parts(&self) -> Result<ExchangeParts, AuthError> {
        let username = form_component(self.client_id.expose_secret());
        let password = form_component(self.client_secret.expose_secret());
        let authorization = format!(
            "Basic {}",
            encode_base64(format!("{username}:{password}").as_bytes())
        );
        let authorization = HeaderValue::from_str(&authorization)
            .map_err(|_| AuthError::new(AuthErrorKind::InvalidConfiguration))?;
        let body = Bytes::from({
            let mut form = url::form_urlencoded::Serializer::new(String::new());
            form.append_pair("grant_type", "client_credentials");
            if let Some(scope) = &self.scope {
                form.append_pair("scope", scope);
            }
            form.finish()
        });
        Ok(ExchangeParts {
            authorization,
            body,
        })
    }
}

impl fmt::Debug for OAuth2ClientCredentials {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OAuth2ClientCredentials")
            .field("token_url", &"<redacted>")
            .field("client_id", &"<redacted>")
            .field("client_secret", &"<redacted>")
            .field("scope_configured", &self.scope.is_some())
            .finish()
    }
}

impl CredentialProvider for OAuth2ClientCredentials {
    type Material = AccessToken;

    fn acquire<'a>(
        &'a self,
        context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        Box::pin(self.exchange(context))
    }

    fn refresh<'a>(
        &'a self,
        _current: &'a Self::Material,
        context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        Box::pin(self.exchange(context))
    }

    fn recovery_mode(&self) -> RecoveryMode {
        RecoveryMode::Refresh
    }
}

/// Named, secret-safe builder for [`OAuth2ClientCredentials`].
pub struct OAuth2ClientCredentialsBuilder {
    token_url: Option<Url>,
    invalid_token_url: bool,
    client_id: Option<SecretString>,
    client_secret: Option<SecretString>,
    scope: Option<Box<str>>,
}

impl OAuth2ClientCredentialsBuilder {
    fn new(token_url: &str) -> Self {
        let token_url = Url::parse(token_url).ok().filter(valid_token_url);
        Self {
            invalid_token_url: token_url.is_none(),
            token_url,
            client_id: None,
            client_secret: None,
            scope: None,
        }
    }

    /// Sets the required protected OAuth client identifier.
    #[must_use]
    pub fn client_id(mut self, client_id: impl Into<SecretString>) -> Self {
        self.client_id = Some(client_id.into());
        self
    }

    /// Sets the required protected OAuth client secret.
    #[must_use]
    pub fn client_secret(mut self, client_secret: impl Into<SecretString>) -> Self {
        self.client_secret = Some(client_secret.into());
        self
    }

    /// Sets the optional OAuth scope string.
    #[must_use]
    pub fn scope(mut self, scope: impl Into<String>) -> Self {
        self.scope = Some(scope.into().into_boxed_str());
        self
    }

    /// Validates required inputs and builds the immutable provider.
    pub fn build(self) -> Result<OAuth2ClientCredentials, OAuth2ClientCredentialsBuildError> {
        if self.invalid_token_url {
            return Err(OAuth2ClientCredentialsBuildError::new(
                OAuth2ClientCredentialsBuildErrorKind::InvalidTokenUrl,
            ));
        }
        let token_url = self.token_url.ok_or_else(|| {
            OAuth2ClientCredentialsBuildError::new(
                OAuth2ClientCredentialsBuildErrorKind::InvalidTokenUrl,
            )
        })?;
        let client_id = self.client_id.ok_or_else(|| {
            OAuth2ClientCredentialsBuildError::new(
                OAuth2ClientCredentialsBuildErrorKind::MissingClientId,
            )
        })?;
        let client_secret = self.client_secret.ok_or_else(|| {
            OAuth2ClientCredentialsBuildError::new(
                OAuth2ClientCredentialsBuildErrorKind::MissingClientSecret,
            )
        })?;
        Ok(OAuth2ClientCredentials {
            token_url,
            client_id,
            client_secret,
            scope: self.scope,
        })
    }
}

impl fmt::Debug for OAuth2ClientCredentialsBuilder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OAuth2ClientCredentialsBuilder")
            .field("token_url_valid", &self.token_url.is_some())
            .field("client_id_configured", &self.client_id.is_some())
            .field("client_secret_configured", &self.client_secret.is_some())
            .field("scope_configured", &self.scope.is_some())
            .finish()
    }
}

/// Stable category for an OAuth2 client-credentials build failure.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OAuth2ClientCredentialsBuildErrorKind {
    /// The token URL was not an absolute HTTPS URL with a host, or contained
    /// user information or a fragment.
    InvalidTokenUrl,
    /// No client identifier was supplied.
    MissingClientId,
    /// No client secret was supplied.
    MissingClientSecret,
}

/// A structural, secret-safe OAuth2 client-credentials build failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OAuth2ClientCredentialsBuildError {
    kind: OAuth2ClientCredentialsBuildErrorKind,
}

impl OAuth2ClientCredentialsBuildError {
    const fn new(kind: OAuth2ClientCredentialsBuildErrorKind) -> Self {
        Self { kind }
    }

    /// Returns the stable build-failure category.
    #[must_use]
    pub const fn kind(&self) -> OAuth2ClientCredentialsBuildErrorKind {
        self.kind
    }
}

impl fmt::Display for OAuth2ClientCredentialsBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let detail = match self.kind {
            OAuth2ClientCredentialsBuildErrorKind::InvalidTokenUrl => {
                "token URL must be absolute HTTPS with a host and no user information or fragment"
            }
            OAuth2ClientCredentialsBuildErrorKind::MissingClientId => {
                "client identifier is required"
            }
            OAuth2ClientCredentialsBuildErrorKind::MissingClientSecret => {
                "client secret is required"
            }
        };
        write!(
            formatter,
            "OAuth2 client-credentials configuration failed: {detail}"
        )
    }
}

impl Error for OAuth2ClientCredentialsBuildError {}

struct ExchangeParts {
    authorization: HeaderValue,
    body: Bytes,
}

fn valid_token_url(url: &Url) -> bool {
    url.scheme() == "https"
        && url.host().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment().is_none()
}

fn decode_token_response(status: StatusCode, body: &Bytes) -> Result<AccessToken, AuthError> {
    if !status.is_success() {
        return Err(AuthError::new(AuthErrorKind::Acquisition));
    }
    let value: serde_json::Value = serde_json::from_slice(body).map_err(|error| {
        drop(error);
        AuthError::new(AuthErrorKind::Acquisition)
    })?;
    let object = value
        .as_object()
        .ok_or_else(|| AuthError::new(AuthErrorKind::Acquisition))?;
    let token = object
        .get("access_token")
        .and_then(serde_json::Value::as_str)
        .filter(|token| !token.is_empty())
        .ok_or_else(|| AuthError::new(AuthErrorKind::Acquisition))?;
    if object.get("token_type").is_some_and(|value| {
        value
            .as_str()
            .is_none_or(|token_type| !token_type.eq_ignore_ascii_case("bearer"))
    }) {
        return Err(AuthError::new(AuthErrorKind::Acquisition));
    }
    let expiration = object
        .get("expires_in")
        .map(|value| {
            let seconds = value
                .as_u64()
                .ok_or_else(|| AuthError::new(AuthErrorKind::Acquisition))?;
            Instant::now()
                .checked_add(Duration::from_secs(seconds))
                .ok_or_else(|| AuthError::new(AuthErrorKind::Acquisition))
        })
        .transpose()?;
    Ok(match expiration {
        Some(expires_at) => AccessToken::with_expiration(token, expires_at),
        None => AccessToken::new(token),
    })
}

fn form_component(value: &str) -> String {
    let encoded = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("", value)
        .finish();
    encoded
        .strip_prefix('=')
        .expect("form serializer always emits the empty key separator")
        .to_owned()
}

fn encode_base64(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        output.push(char::from(TABLE[usize::from(first >> 2)]));
        output.push(char::from(
            TABLE[usize::from(((first & 3) << 4) | (second >> 4))],
        ));
        output.push(if chunk.len() > 1 {
            char::from(TABLE[usize::from(((second & 15) << 2) | (third >> 6))])
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            char::from(TABLE[usize::from(third & 63)])
        } else {
            '='
        });
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use concord_core::advanced::{CredentialMaterial, SecretCredential};

    const CLIENT_ID: &str = "oauth-client-id-sentinel-4db370ebb046";
    const CLIENT_SECRET: &str = "oauth-client-secret-sentinel-9f8b14c2e6a1";

    fn provider(scope: Option<&str>) -> OAuth2ClientCredentials {
        let builder = OAuth2ClientCredentials::builder(
            "https://auth.example.test/oauth/token?tenant=protected",
        )
        .client_id(CLIENT_ID)
        .client_secret(CLIENT_SECRET);
        match scope {
            Some(scope) => builder.scope(scope),
            None => builder,
        }
        .build()
        .expect("OAuth provider")
    }

    fn auth_error<T>(result: Result<T, AuthError>) -> AuthError {
        match result {
            Ok(_) => panic!("expected OAuth authentication failure"),
            Err(error) => error,
        }
    }

    #[test]
    fn builder_validates_urls_required_inputs_and_redacts_all_dynamic_values() {
        let invalid = [
            "not a URL",
            "http://auth.example.test/token",
            "https://user@auth.example.test/token",
            "https://user:password@auth.example.test/token",
            "https://auth.example.test/token#fragment",
        ];
        for url in invalid {
            let error = OAuth2ClientCredentials::builder(url)
                .client_id(CLIENT_ID)
                .client_secret(CLIENT_SECRET)
                .build()
                .expect_err("unsafe token URL");
            assert_eq!(
                error.kind(),
                OAuth2ClientCredentialsBuildErrorKind::InvalidTokenUrl
            );
            for rendered in [error.to_string(), format!("{error:?}")] {
                assert!(!rendered.contains(url));
                assert!(!rendered.contains(CLIENT_ID));
                assert!(!rendered.contains(CLIENT_SECRET));
            }
            assert!(Error::source(&error).is_none());
        }

        let missing_id = OAuth2ClientCredentials::builder("https://auth.example.test/token")
            .client_secret(CLIENT_SECRET)
            .build()
            .expect_err("missing client ID");
        assert_eq!(
            missing_id.kind(),
            OAuth2ClientCredentialsBuildErrorKind::MissingClientId
        );
        let missing_secret = OAuth2ClientCredentials::builder("https://auth.example.test/token")
            .client_id(CLIENT_ID)
            .build()
            .expect_err("missing client secret");
        assert_eq!(
            missing_secret.kind(),
            OAuth2ClientCredentialsBuildErrorKind::MissingClientSecret
        );

        let builder =
            OAuth2ClientCredentials::builder("https://auth.example.test/token?url-sentinel=0f94")
                .client_id(CLIENT_ID)
                .client_secret(CLIENT_SECRET)
                .scope("scope-sentinel-d9ab");
        let rendered = format!("{builder:?}");
        for sentinel in [
            "url-sentinel=0f94",
            CLIENT_ID,
            CLIENT_SECRET,
            "scope-sentinel-d9ab",
        ] {
            assert!(!rendered.contains(sentinel));
        }
        let provider = builder.build().expect("valid provider");
        let rendered = format!("{provider:?}");
        for sentinel in [
            "url-sentinel=0f94",
            CLIENT_ID,
            CLIENT_SECRET,
            "scope-sentinel-d9ab",
        ] {
            assert!(!rendered.contains(sentinel));
        }
    }

    #[test]
    fn oauth_success_uses_exact_post_basic_form_scope_and_expiry() {
        const TOKEN: &str = "oauth-result-token-41e79c";
        let provider = provider(Some("users.read offline"));
        let parts = provider.exchange_parts().expect("exchange parts");
        assert_eq!(
            parts.authorization,
            HeaderValue::from_static(
                "Basic b2F1dGgtY2xpZW50LWlkLXNlbnRpbmVsLTRkYjM3MGViYjA0NjpvYXV0aC1jbGllbnQtc2VjcmV0LXNlbnRpbmVsLTlmOGIxNGMyZTZhMQ=="
            )
        );
        assert_eq!(
            parts.body,
            Bytes::from_static(b"grant_type=client_credentials&scope=users.read+offline")
        );
        let before = Instant::now();
        let token = decode_token_response(
            StatusCode::OK,
            &Bytes::from_static(
                br#"{"access_token":"oauth-result-token-41e79c","token_type":"BeArEr","expires_in":3600}"#,
            ),
        )
        .expect("token response");
        assert_eq!(token.expose_secret(), TOKEN);
        assert!(token.expires_at().is_some_and(|expires| {
            expires >= before + Duration::from_secs(3_599)
                && expires <= Instant::now() + Duration::from_secs(3_600)
        }));
    }

    #[test]
    fn oauth_refresh_reacquires_without_scope_or_refresh_token_grant() {
        let provider = provider(None);
        assert_eq!(provider.recovery_mode(), RecoveryMode::Refresh);
        let parts = provider.exchange_parts().expect("exchange parts");
        assert_eq!(
            parts.body,
            Bytes::from_static(b"grant_type=client_credentials")
        );
        assert!(!String::from_utf8_lossy(&parts.body).contains("refresh_token"));
    }

    #[test]
    fn oauth_basic_components_use_form_encoding_before_base64() {
        let provider = OAuth2ClientCredentials::builder("https://auth.example.test/oauth/token")
            .client_id("client id:plus+")
            .client_secret("s e:cret+")
            .build()
            .expect("OAuth provider");
        let parts = provider.exchange_parts().expect("exchange parts");
        assert_eq!(
            parts.authorization,
            HeaderValue::from_static("Basic Y2xpZW50K2lkJTNBcGx1cyUyQjpzK2UlM0FjcmV0JTJC")
        );
    }

    #[test]
    fn malformed_unsafe_and_non_success_oauth_responses_are_body_safe() {
        const SENTINEL: &str = "oauth-error-body-sentinel-e83a9d";
        let cases = [
            (StatusCode::OK, br#"{}"#.as_slice()),
            (StatusCode::OK, br#"{"access_token":""}"#.as_slice()),
            (
                StatusCode::OK,
                br#"{"access_token":"x","token_type":"mac"}"#.as_slice(),
            ),
            (StatusCode::OK, br#"{"#.as_slice()),
            (
                StatusCode::OK,
                br#"{"access_token":"x","expires_in":-1}"#.as_slice(),
            ),
            (
                StatusCode::OK,
                br#"{"access_token":"x","expires_in":18446744073709551616}"#.as_slice(),
            ),
            (StatusCode::BAD_REQUEST, SENTINEL.as_bytes()),
        ];
        for (status, body) in cases {
            let error = auth_error(decode_token_response(status, &Bytes::copy_from_slice(body)));
            assert_eq!(error.kind(), AuthErrorKind::Acquisition);
            for rendered in [error.to_string(), format!("{error:?}")] {
                assert!(!rendered.contains(SENTINEL));
            }
            assert!(Error::source(&error).is_none());
        }
    }
}

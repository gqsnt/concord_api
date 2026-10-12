use std::error::Error;
use std::fmt;
use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

use bytes::Bytes;
use concord_core::contracts::{
    AccessToken, AuthError, AuthErrorKind, BoxFuture, CredentialProvider, CredentialRequest,
    HeaderValue, Method, ProviderBody, ProviderOrigin, ProviderRequest, RecoveryMode, SecretString,
    StatusCode, Url,
};
use http::header::{AUTHORIZATION, CONTENT_TYPE};

/// Maximum UTF-8 bytes accepted for one OAuth2 token URL.
pub const MAX_OAUTH2_TOKEN_URL_BYTES: usize = 8_192;

/// Maximum UTF-8 bytes accepted for one OAuth2 client identifier.
pub const MAX_OAUTH2_CLIENT_ID_BYTES: usize = 8_192;

/// Maximum UTF-8 bytes accepted for one OAuth2 client secret.
pub const MAX_OAUTH2_CLIENT_SECRET_BYTES: usize = 8_192;

/// Maximum UTF-8 bytes accepted for one OAuth2 scope string.
pub const MAX_OAUTH2_SCOPE_BYTES: usize = 65_536;

/// Maximum bytes in the final OAuth2 Basic Authorization header value.
pub const MAX_OAUTH2_AUTHORIZATION_HEADER_BYTES: usize = 32_768;

/// A client-credentials provider that acquires bearer access tokens through
/// Concord's isolated, bounded provider-HTTP capability.
///
/// The provider stores the token URL and protected client credentials in one
/// immutable configuration. Provider requests remain outside application
/// hooks, application rate limiting, and application transport pools. Concord
/// bounds configuration strings, encoded headers, request and response bodies,
/// and the enclosing credential operation. Encoding lengths are checked before
/// final output allocation.
///
/// The provider declares exactly one helper origin — the validated origin of
/// its token URL — through [`CredentialProvider::http_origins`], so Concord's
/// provider HTTP helper refuses every other destination.
pub struct OAuth2ClientCredentials {
    token_url: Url,
    token_origin: [ProviderOrigin; 1],
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
        let parts = self.exchange_parts(context.http.request_body_limit())?;
        let request = ProviderRequest::new(Method::POST, self.token_url.clone())
            .header(AUTHORIZATION, parts.authorization)
            .header(
                CONTENT_TYPE,
                HeaderValue::from_static("application/x-www-form-urlencoded"),
            )
            .body(ProviderBody::Bytes(parts.body));
        let response = context.http.send(request).await?;
        decode_token_response(response.status(), response.body()).map_err(|error| {
            if !response.status().is_success() {
                if let Some(delay) = response.retry_after().filter(|delay| !delay.is_zero()) {
                    return error.with_retry_after(delay);
                }
            }
            error
        })
    }

    fn exchange_parts(&self, request_body_limit: NonZeroUsize) -> Result<ExchangeParts, AuthError> {
        let lengths = ExchangeLengths::checked(self, request_body_limit)?;
        let authorization = encode_authorization(self, lengths.authorization)?;
        let body = encode_form_body(self.scope.as_deref(), lengths.body)?;
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

    fn http_origins(&self) -> &[ProviderOrigin] {
        &self.token_origin
    }
}

/// Named, secret-safe builder for [`OAuth2ClientCredentials`].
pub struct OAuth2ClientCredentialsBuilder {
    token_url: Option<Url>,
    failure: Option<OAuth2ClientCredentialsBuildErrorKind>,
    client_id: Option<SecretString>,
    client_secret: Option<SecretString>,
    scope: Option<Box<str>>,
}

impl OAuth2ClientCredentialsBuilder {
    fn new(token_url: &str) -> Self {
        let (token_url, failure) = if token_url.len() > MAX_OAUTH2_TOKEN_URL_BYTES {
            (
                None,
                Some(OAuth2ClientCredentialsBuildErrorKind::TokenUrlTooLong),
            )
        } else {
            match Url::parse(token_url).ok().filter(valid_token_url) {
                Some(token_url) => (Some(token_url), None),
                None => (
                    None,
                    Some(OAuth2ClientCredentialsBuildErrorKind::InvalidTokenUrl),
                ),
            }
        };
        Self {
            token_url,
            failure,
            client_id: None,
            client_secret: None,
            scope: None,
        }
    }

    /// Copies one required protected OAuth client identifier after validating
    /// its borrowed UTF-8 byte length.
    #[must_use]
    pub fn client_id(mut self, client_id: impl AsRef<str>) -> Self {
        if self.failure.is_some() {
            return self;
        }
        self.client_id = None;
        let client_id = client_id.as_ref();
        if client_id.len() > MAX_OAUTH2_CLIENT_ID_BYTES {
            self.failure = Some(OAuth2ClientCredentialsBuildErrorKind::ClientIdTooLong);
        } else {
            self.client_id = Some(
                SecretString::try_new(client_id)
                    .expect("the OAuth2 client-ID limit is below the protected-string limit"),
            );
        }
        self
    }

    /// Retains one already-owned protected OAuth client identifier after
    /// validating its UTF-8 byte length.
    #[must_use]
    pub fn client_id_owned(mut self, client_id: SecretString) -> Self {
        if self.failure.is_some() {
            return self;
        }
        self.client_id = None;
        if client_id.expose_secret().len() > MAX_OAUTH2_CLIENT_ID_BYTES {
            self.failure = Some(OAuth2ClientCredentialsBuildErrorKind::ClientIdTooLong);
        } else {
            self.client_id = Some(client_id);
        }
        self
    }

    /// Copies one required protected OAuth client secret after validating its
    /// borrowed UTF-8 byte length.
    #[must_use]
    pub fn client_secret(mut self, client_secret: impl AsRef<str>) -> Self {
        if self.failure.is_some() {
            return self;
        }
        self.client_secret = None;
        let client_secret = client_secret.as_ref();
        if client_secret.len() > MAX_OAUTH2_CLIENT_SECRET_BYTES {
            self.failure = Some(OAuth2ClientCredentialsBuildErrorKind::ClientSecretTooLong);
        } else {
            self.client_secret = Some(
                SecretString::try_new(client_secret)
                    .expect("the OAuth2 client-secret limit is below the protected-string limit"),
            );
        }
        self
    }

    /// Retains one already-owned protected OAuth client secret after
    /// validating its UTF-8 byte length.
    #[must_use]
    pub fn client_secret_owned(mut self, client_secret: SecretString) -> Self {
        if self.failure.is_some() {
            return self;
        }
        self.client_secret = None;
        if client_secret.expose_secret().len() > MAX_OAUTH2_CLIENT_SECRET_BYTES {
            self.failure = Some(OAuth2ClientCredentialsBuildErrorKind::ClientSecretTooLong);
        } else {
            self.client_secret = Some(client_secret);
        }
        self
    }

    /// Copies one optional OAuth scope string after validating its borrowed
    /// UTF-8 byte length.
    #[must_use]
    pub fn scope(mut self, scope: impl AsRef<str>) -> Self {
        if self.failure.is_some() {
            return self;
        }
        self.scope = None;
        let scope = scope.as_ref();
        if scope.len() > MAX_OAUTH2_SCOPE_BYTES {
            self.failure = Some(OAuth2ClientCredentialsBuildErrorKind::ScopeTooLong);
        } else {
            self.scope = Some(scope.into());
        }
        self
    }

    /// Validates required inputs and builds the immutable provider.
    pub fn build(self) -> Result<OAuth2ClientCredentials, OAuth2ClientCredentialsBuildError> {
        if let Some(failure) = self.failure {
            return Err(OAuth2ClientCredentialsBuildError::new(failure));
        }
        let token_url = self.token_url.ok_or_else(|| {
            OAuth2ClientCredentialsBuildError::new(
                OAuth2ClientCredentialsBuildErrorKind::InvalidTokenUrl,
            )
        })?;
        let token_origin = ProviderOrigin::from_url(&token_url).map_err(|_| {
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
            token_origin: [token_origin],
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
            .field("configuration_valid", &self.failure.is_none())
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
    /// The supplied token URL exceeded [`MAX_OAUTH2_TOKEN_URL_BYTES`].
    TokenUrlTooLong,
    /// The token URL was not an absolute HTTPS URL with a host, or contained
    /// user information or a fragment.
    InvalidTokenUrl,
    /// The supplied client identifier exceeded
    /// [`MAX_OAUTH2_CLIENT_ID_BYTES`].
    ClientIdTooLong,
    /// The supplied client secret exceeded
    /// [`MAX_OAUTH2_CLIENT_SECRET_BYTES`].
    ClientSecretTooLong,
    /// The supplied scope exceeded [`MAX_OAUTH2_SCOPE_BYTES`].
    ScopeTooLong,
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
            OAuth2ClientCredentialsBuildErrorKind::TokenUrlTooLong => {
                "token URL exceeds the supported byte limit"
            }
            OAuth2ClientCredentialsBuildErrorKind::InvalidTokenUrl => {
                "token URL must be absolute HTTPS with a host and no user information or fragment"
            }
            OAuth2ClientCredentialsBuildErrorKind::ClientIdTooLong => {
                "client identifier exceeds the supported byte limit"
            }
            OAuth2ClientCredentialsBuildErrorKind::ClientSecretTooLong => {
                "client secret exceeds the supported byte limit"
            }
            OAuth2ClientCredentialsBuildErrorKind::ScopeTooLong => {
                "scope exceeds the supported byte limit"
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

struct ExchangeLengths {
    authorization: usize,
    body: usize,
}

impl ExchangeLengths {
    fn checked(
        provider: &OAuth2ClientCredentials,
        request_body_limit: NonZeroUsize,
    ) -> Result<Self, AuthError> {
        let client_id = form_component_length(provider.client_id.expose_secret())?;
        let client_secret = form_component_length(provider.client_secret.expose_secret())?;
        let authorization_source = client_id
            .checked_add(1)
            .and_then(|length| length.checked_add(client_secret))
            .ok_or_else(invalid_oauth_configuration)?;
        let authorization = BASIC_PREFIX
            .len()
            .checked_add(base64_output_length(authorization_source)?)
            .ok_or_else(invalid_oauth_configuration)?;
        if authorization > MAX_OAUTH2_AUTHORIZATION_HEADER_BYTES {
            return Err(invalid_oauth_configuration());
        }

        let body = if let Some(scope) = provider.scope.as_deref() {
            let scope = form_component_length(scope)?;
            FORM_GRANT
                .len()
                .checked_add(FORM_SCOPE_PREFIX.len())
                .and_then(|length| length.checked_add(scope))
                .ok_or_else(invalid_oauth_configuration)?
        } else {
            FORM_GRANT.len()
        };
        if body > request_body_limit.get() {
            return Err(AuthError::new(AuthErrorKind::RequestTooLarge));
        }

        Ok(Self {
            authorization,
            body,
        })
    }
}

const BASIC_PREFIX: &[u8] = b"Basic ";
const BASE64_TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const FORM_GRANT: &[u8] = b"grant_type=client_credentials";
const FORM_SCOPE_PREFIX: &[u8] = b"&scope=";

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
    match expiration {
        Some(expires_at) => {
            AccessToken::try_with_expiration(token, expires_at).map_err(AuthError::from)
        }
        None => AccessToken::try_new(token).map_err(AuthError::from),
    }
}

fn invalid_oauth_configuration() -> AuthError {
    AuthError::new(AuthErrorKind::InvalidConfiguration)
}

fn form_component_length(value: &str) -> Result<usize, AuthError> {
    url::form_urlencoded::byte_serialize(value.as_bytes()).try_fold(0_usize, |length, component| {
        length
            .checked_add(component.len())
            .ok_or_else(invalid_oauth_configuration)
    })
}

fn append_form_component(output: &mut Vec<u8>, value: &str) {
    for component in url::form_urlencoded::byte_serialize(value.as_bytes()) {
        output.extend_from_slice(component.as_bytes());
    }
}

fn base64_output_length(input: usize) -> Result<usize, AuthError> {
    let groups = (input / 3)
        .checked_add(usize::from(input % 3 != 0))
        .ok_or_else(invalid_oauth_configuration)?;
    groups
        .checked_mul(4)
        .ok_or_else(invalid_oauth_configuration)
}

fn encode_authorization(
    provider: &OAuth2ClientCredentials,
    length: usize,
) -> Result<HeaderValue, AuthError> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(length)
        .map_err(|_| invalid_oauth_configuration())?;
    output.extend_from_slice(BASIC_PREFIX);
    let mut encoder = Base64Encoder::new(&mut output);
    for component in
        url::form_urlencoded::byte_serialize(provider.client_id.expose_secret().as_bytes())
    {
        encoder.write(component.as_bytes());
    }
    encoder.write(b":");
    for component in
        url::form_urlencoded::byte_serialize(provider.client_secret.expose_secret().as_bytes())
    {
        encoder.write(component.as_bytes());
    }
    encoder.finish();
    debug_assert_eq!(output.len(), length);
    HeaderValue::from_maybe_shared(Bytes::from(output)).map_err(|_| invalid_oauth_configuration())
}

fn encode_form_body(scope: Option<&str>, length: usize) -> Result<Bytes, AuthError> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(length)
        .map_err(|_| invalid_oauth_configuration())?;
    output.extend_from_slice(FORM_GRANT);
    if let Some(scope) = scope {
        output.extend_from_slice(FORM_SCOPE_PREFIX);
        append_form_component(&mut output, scope);
    }
    debug_assert_eq!(output.len(), length);
    Ok(Bytes::from(output))
}

struct Base64Encoder<'a> {
    output: &'a mut Vec<u8>,
    pending: [u8; 3],
    pending_length: usize,
}

impl<'a> Base64Encoder<'a> {
    fn new(output: &'a mut Vec<u8>) -> Self {
        Self {
            output,
            pending: [0; 3],
            pending_length: 0,
        }
    }

    fn write(&mut self, input: &[u8]) {
        for byte in input {
            self.pending[self.pending_length] = *byte;
            self.pending_length += 1;
            if self.pending_length == self.pending.len() {
                self.write_group(3);
                self.pending_length = 0;
            }
        }
    }

    fn finish(mut self) {
        if self.pending_length != 0 {
            self.write_group(self.pending_length);
        }
    }

    fn write_group(&mut self, length: usize) {
        let first = self.pending[0];
        let second = self.pending[1];
        let third = self.pending[2];
        self.output.push(BASE64_TABLE[usize::from(first >> 2)]);
        self.output
            .push(BASE64_TABLE[usize::from(((first & 3) << 4) | (second >> 4))]);
        self.output.push(if length > 1 {
            BASE64_TABLE[usize::from(((second & 15) << 2) | (third >> 6))]
        } else {
            b'='
        });
        self.output.push(if length > 2 {
            BASE64_TABLE[usize::from(third & 63)]
        } else {
            b'='
        });
        self.pending = [0; 3];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use concord_core::contracts::{CredentialMaterial, SecretCredential};

    const CLIENT_ID: &str = "oauth-client-id-sentinel-4db370ebb046";
    const CLIENT_SECRET: &str = "oauth-client-secret-sentinel-9f8b14c2e6a1";

    fn request_limit(bytes: usize) -> NonZeroUsize {
        NonZeroUsize::new(bytes).expect("positive OAuth request limit")
    }

    fn exchange_parts(provider: &OAuth2ClientCredentials) -> Result<ExchangeParts, AuthError> {
        provider.exchange_parts(request_limit(1_048_576))
    }

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

    fn build_error(
        result: Result<OAuth2ClientCredentials, OAuth2ClientCredentialsBuildError>,
    ) -> OAuth2ClientCredentialsBuildError {
        match result {
            Ok(_) => panic!("expected OAuth builder failure"),
            Err(error) => error,
        }
    }

    fn assert_build_error_safe(
        error: OAuth2ClientCredentialsBuildError,
        expected: OAuth2ClientCredentialsBuildErrorKind,
        sentinels: &[&str],
    ) {
        assert_eq!(error.kind(), expected);
        for rendered in [error.to_string(), format!("{error:?}")] {
            for sentinel in sentinels {
                assert!(!rendered.contains(sentinel));
            }
        }
        assert!(Error::source(&error).is_none());
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
    fn oauth_token_url_max_and_first_excess() {
        const PREFIX: &str = "https://auth.example.test/";
        let maximum = format!(
            "{PREFIX}{}",
            "a".repeat(MAX_OAUTH2_TOKEN_URL_BYTES - PREFIX.len())
        );
        assert_eq!(maximum.len(), MAX_OAUTH2_TOKEN_URL_BYTES);
        OAuth2ClientCredentials::builder(&maximum)
            .client_id(CLIENT_ID)
            .client_secret(CLIENT_SECRET)
            .build()
            .expect("maximum token URL");

        let excessive = format!("{maximum}a");
        let error = build_error(
            OAuth2ClientCredentials::builder(&excessive)
                .client_id(CLIENT_ID)
                .client_secret(CLIENT_SECRET)
                .build(),
        );
        assert_build_error_safe(
            error,
            OAuth2ClientCredentialsBuildErrorKind::TokenUrlTooLong,
            &[&excessive, CLIENT_ID, CLIENT_SECRET],
        );
    }

    #[test]
    fn oauth_client_id_max_and_first_excess() {
        let maximum = "i".repeat(MAX_OAUTH2_CLIENT_ID_BYTES);
        OAuth2ClientCredentials::builder("https://auth.example.test/token")
            .client_id(&maximum)
            .client_secret(CLIENT_SECRET)
            .build()
            .expect("maximum client identifier");

        let excessive = format!("{maximum}x");
        let error = build_error(
            OAuth2ClientCredentials::builder("https://auth.example.test/token")
                .client_id(&excessive)
                .client_secret(CLIENT_SECRET)
                .build(),
        );
        assert_build_error_safe(
            error,
            OAuth2ClientCredentialsBuildErrorKind::ClientIdTooLong,
            &[&excessive, CLIENT_SECRET],
        );
    }

    #[test]
    fn oauth_client_secret_max_and_first_excess() {
        let maximum = "s".repeat(MAX_OAUTH2_CLIENT_SECRET_BYTES);
        OAuth2ClientCredentials::builder("https://auth.example.test/token")
            .client_id(CLIENT_ID)
            .client_secret_owned(
                SecretString::try_new(maximum).expect("bounded OAuth2 client secret"),
            )
            .build()
            .expect("maximum client secret");

        let excessive = format!("{}x", "s".repeat(MAX_OAUTH2_CLIENT_SECRET_BYTES));
        let error = build_error(
            OAuth2ClientCredentials::builder("https://auth.example.test/token")
                .client_id(CLIENT_ID)
                .client_secret_owned(
                    SecretString::try_new(excessive.clone())
                        .expect("bounded generic protected string"),
                )
                .build(),
        );
        assert_build_error_safe(
            error,
            OAuth2ClientCredentialsBuildErrorKind::ClientSecretTooLong,
            &[&excessive, CLIENT_ID],
        );
    }

    #[test]
    fn oauth_scope_max_and_first_excess() {
        let maximum = "r".repeat(MAX_OAUTH2_SCOPE_BYTES);
        OAuth2ClientCredentials::builder("https://auth.example.test/token")
            .client_id(CLIENT_ID)
            .client_secret(CLIENT_SECRET)
            .scope(&maximum)
            .build()
            .expect("maximum scope");

        let excessive = format!("{maximum}x");
        let error = build_error(
            OAuth2ClientCredentials::builder("https://auth.example.test/token")
                .client_id(CLIENT_ID)
                .client_secret(CLIENT_SECRET)
                .scope(&excessive)
                .build(),
        );
        assert_build_error_safe(
            error,
            OAuth2ClientCredentialsBuildErrorKind::ScopeTooLong,
            &[&excessive, CLIENT_ID, CLIENT_SECRET],
        );
    }

    #[test]
    fn oauth_encoded_authorization_checked_before_allocation() {
        let client_id = "%".repeat(MAX_OAUTH2_CLIENT_ID_BYTES);
        let client_secret = "%".repeat(MAX_OAUTH2_CLIENT_SECRET_BYTES);
        let provider = OAuth2ClientCredentials::builder("https://auth.example.test/token")
            .client_id(&client_id)
            .client_secret(&client_secret)
            .build()
            .expect("bounded raw credentials");
        let error = auth_error(exchange_parts(&provider));
        assert_eq!(error.kind(), AuthErrorKind::InvalidConfiguration);
        for rendered in [error.to_string(), format!("{error:?}")] {
            assert!(!rendered.contains(&client_id));
            assert!(!rendered.contains(&client_secret));
        }
    }

    #[test]
    fn oauth_form_body_checked_against_provider_limit() {
        const SCOPE: &str = "oauth-body-scope-sentinel-27d1";
        let provider = provider(Some(SCOPE));
        let expected_length = FORM_GRANT.len() + FORM_SCOPE_PREFIX.len() + SCOPE.len();
        let error = auth_error(provider.exchange_parts(request_limit(expected_length - 1)));
        assert_eq!(error.kind(), AuthErrorKind::RequestTooLarge);
        assert!(!error.to_string().contains(SCOPE));
        assert!(!format!("{error:?}").contains(SCOPE));
        assert!(Error::source(&error).is_none());
        assert!(
            provider
                .exchange_parts(request_limit(expected_length))
                .is_ok()
        );
    }

    #[test]
    fn oauth_boundary_request_is_protocol_correct() {
        const TOKEN: &str = "oauth-result-token-41e79c";
        let provider = provider(Some("users.read offline"));
        let parts = exchange_parts(&provider).expect("exchange parts");
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
    fn oauth_declares_exactly_its_validated_token_origin() {
        let provider = provider(None);
        let declared = provider.http_origins();
        assert_eq!(declared.len(), 1);
        assert_eq!(declared[0].as_str(), "https://auth.example.test");
        assert!(declared[0].matches(
            &Url::parse("https://auth.example.test/oauth/token?tenant=protected").expect("URL")
        ));
        assert!(
            !declared[0].matches(&Url::parse("https://other.example.test/token").expect("URL"))
        );

        let explicit_port = OAuth2ClientCredentials::builder("https://Auth.Example.TEST:8443/t")
            .client_id(CLIENT_ID)
            .client_secret(CLIENT_SECRET)
            .build()
            .expect("OAuth provider with explicit port");
        assert_eq!(
            explicit_port.http_origins()[0].as_str(),
            "https://auth.example.test:8443"
        );
    }

    #[test]
    fn oauth_refresh_reacquires_without_scope_or_refresh_token_grant() {
        let provider = provider(None);
        assert_eq!(provider.recovery_mode(), RecoveryMode::Refresh);
        let parts = exchange_parts(&provider).expect("exchange parts");
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
        let parts = exchange_parts(&provider).expect("exchange parts");
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

    #[test]
    fn oauth_first_excess_errors_are_secret_safe() {
        const URL_SENTINEL: &str = "oauth-url-first-excess-sentinel-c91e";
        const ID_SENTINEL: &str = "oauth-id-first-excess-sentinel-9a2f";
        const SECRET_SENTINEL: &str = "oauth-secret-first-excess-sentinel-5d63";
        const SCOPE_SENTINEL: &str = "oauth-scope-first-excess-sentinel-f12b";

        let url = format!(
            "https://auth.example.test/{URL_SENTINEL}{}",
            "u".repeat(MAX_OAUTH2_TOKEN_URL_BYTES)
        );
        assert_build_error_safe(
            build_error(
                OAuth2ClientCredentials::builder(&url)
                    .client_id(CLIENT_ID)
                    .client_secret(CLIENT_SECRET)
                    .build(),
            ),
            OAuth2ClientCredentialsBuildErrorKind::TokenUrlTooLong,
            &[URL_SENTINEL, CLIENT_ID, CLIENT_SECRET],
        );

        let client_id = format!("{ID_SENTINEL}{}", "i".repeat(MAX_OAUTH2_CLIENT_ID_BYTES));
        assert_build_error_safe(
            build_error(
                OAuth2ClientCredentials::builder("https://auth.example.test/token")
                    .client_id(&client_id)
                    .client_secret(CLIENT_SECRET)
                    .build(),
            ),
            OAuth2ClientCredentialsBuildErrorKind::ClientIdTooLong,
            &[ID_SENTINEL, CLIENT_SECRET],
        );

        let client_secret = format!(
            "{SECRET_SENTINEL}{}",
            "s".repeat(MAX_OAUTH2_CLIENT_SECRET_BYTES)
        );
        assert_build_error_safe(
            build_error(
                OAuth2ClientCredentials::builder("https://auth.example.test/token")
                    .client_id(CLIENT_ID)
                    .client_secret(&client_secret)
                    .build(),
            ),
            OAuth2ClientCredentialsBuildErrorKind::ClientSecretTooLong,
            &[SECRET_SENTINEL, CLIENT_ID],
        );

        let scope = format!("{SCOPE_SENTINEL}{}", "r".repeat(MAX_OAUTH2_SCOPE_BYTES));
        assert_build_error_safe(
            build_error(
                OAuth2ClientCredentials::builder("https://auth.example.test/token")
                    .client_id(CLIENT_ID)
                    .client_secret(CLIENT_SECRET)
                    .scope(&scope)
                    .build(),
            ),
            OAuth2ClientCredentialsBuildErrorKind::ScopeTooLong,
            &[SCOPE_SENTINEL, CLIENT_ID, CLIENT_SECRET],
        );
    }

    #[test]
    fn oauth_debug_and_display_remain_redacted() {
        const URL_SENTINEL: &str = "oauth-debug-url-sentinel-e791";
        const ID_SENTINEL: &str = "oauth-debug-id-sentinel-100a";
        const SECRET_SENTINEL: &str = "oauth-debug-secret-sentinel-2db5";
        const SCOPE_SENTINEL: &str = "oauth-debug-scope-sentinel-4f60";

        let builder = OAuth2ClientCredentials::builder(format!(
            "https://auth.example.test/token?value={URL_SENTINEL}"
        ))
        .client_id_owned(
            SecretString::try_new(ID_SENTINEL).expect("bounded OAuth2 client identifier"),
        )
        .client_secret_owned(
            SecretString::try_new(SECRET_SENTINEL).expect("bounded OAuth2 client secret"),
        )
        .scope(SCOPE_SENTINEL);
        let builder_debug = format!("{builder:?}");
        for sentinel in [URL_SENTINEL, ID_SENTINEL, SECRET_SENTINEL, SCOPE_SENTINEL] {
            assert!(!builder_debug.contains(sentinel));
        }

        let provider = builder.build().expect("redacted provider");
        let provider_debug = format!("{provider:?}");
        for sentinel in [URL_SENTINEL, ID_SENTINEL, SECRET_SENTINEL, SCOPE_SENTINEL] {
            assert!(!provider_debug.contains(sentinel));
        }

        let excessive = format!(
            "{SECRET_SENTINEL}{}",
            "x".repeat(MAX_OAUTH2_CLIENT_SECRET_BYTES)
        );
        let error = build_error(
            OAuth2ClientCredentials::builder("https://auth.example.test/token")
                .client_id(ID_SENTINEL)
                .client_secret(&excessive)
                .build(),
        );
        for rendered in [error.to_string(), format!("{error:?}")] {
            assert!(!rendered.contains(ID_SENTINEL));
            assert!(!rendered.contains(SECRET_SENTINEL));
        }
    }
}

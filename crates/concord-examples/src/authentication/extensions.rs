//! Custom credential material, response codec, and isolated provider HTTP.
//!
//! Required feature: `oauth2-client`. Mode: deterministic extension example.
//! Documentation owner: `docs/extensions.md`.
//! Proving tests: `tests/construction.rs` and façade provider tests.
//!
//! The provider declares the exact origin its helper HTTP may reach
//! ([`CredentialProvider::http_origins`], AUTH-02). Any other destination is
//! rejected before transport; the empty default grants none.

// ANCHOR: credential_provider_extension
use std::fmt;
use std::sync::LazyLock;
use std::time::Duration;

use concord::auth::{
    AuthError, AuthErrorKind, BoxFuture, CredentialMaterial, CredentialProvider, CredentialRequest,
    ProviderBody, ProviderOrigin, ProviderRequest, SecretCredential,
};
use concord::codec::{CodecError, CodecErrorKind, DecodeBody, DecodeContext};
use concord::http::{HeaderName, HeaderValue, Method, Url};
use concord::{Bytes, SecretString};

/// Redacted credential material returned by the custom provider example.
pub struct ExampleToken(SecretString);

impl CredentialMaterial for ExampleToken {}

impl SecretCredential for ExampleToken {
    fn expose_secret(&self) -> &str {
        self.0.expose_secret()
    }
}

impl fmt::Debug for ExampleToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ExampleToken(<redacted>)")
    }
}

/// Decoder used by the endpoint-backed credential example.
pub struct ExampleTokenCodec;

impl DecodeBody for ExampleTokenCodec {
    type Output = ExampleToken;

    const ACCEPT: Option<&'static str> = Some("application/vnd.concord.token");

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        let token = String::from_utf8(body.to_vec())
            .map_err(|_| CodecError::new(CodecErrorKind::Decode))?;
        Ok(ExampleToken(
            SecretString::try_from(token).map_err(|_| CodecError::new(CodecErrorKind::Limit))?,
        ))
    }
}

/// The only origin the example provider's helper HTTP may reach.
static IDENTITY_ORIGIN: LazyLock<[ProviderOrigin; 1]> = LazyLock::new(|| {
    [ProviderOrigin::parse("https://identity.example.test")
        .expect("the fixed example identity origin is a valid HTTPS origin")]
});

/// A provider that demonstrates Concord's isolated provider HTTP capability.
pub struct ExampleProvider;

impl CredentialProvider for ExampleProvider {
    type Material = ExampleToken;

    fn acquire<'a>(
        &'a self,
        context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        Box::pin(async move {
            let request = ProviderRequest::new(
                Method::POST,
                Url::parse("https://identity.example.test/token")
                    .expect("fixed provider URL is valid"),
            )
            .header(
                HeaderName::from_static("content-type"),
                HeaderValue::from_static("application/x-www-form-urlencoded"),
            )
            .body(ProviderBody::Bytes(Bytes::from_static(
                b"grant_type=client_credentials",
            )))
            .timeout(Duration::from_secs(2));
            let response = context.http.send(request).await?;
            if !response.status().is_success() {
                return Err(AuthError::new(AuthErrorKind::Acquisition));
            }
            let token = String::from_utf8(response.into_body().to_vec())
                .map_err(|_| AuthError::new(AuthErrorKind::Acquisition))?;
            Ok(ExampleToken(
                SecretString::try_new(token).map_err(AuthError::from)?,
            ))
        })
    }

    fn http_origins(&self) -> &[ProviderOrigin] {
        IDENTITY_ORIGIN.as_slice()
    }
}
// ANCHOR_END: credential_provider_extension

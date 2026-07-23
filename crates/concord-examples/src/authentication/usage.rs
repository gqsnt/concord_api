//! Named secret/provider construction with no positional credential inputs.
//!
//! Required features: `oauth2-client` and `default-tls`. Mode:
//! construction-only. Documentation owner: `docs/guide/04-authentication.md`.
//! Proving test: `tests/construction.rs`.

use concord::SecretString;
use concord::auth::OAuth2ClientCredentials;

use super::api::{AuthenticationApi, authentication_api};
use super::extensions::ExampleProvider;

/// Builds the configured authentication client and constructs each auth shape.
pub fn construct() -> Result<(), Box<dyn std::error::Error>> {
    let oauth = OAuth2ClientCredentials::builder("https://localhost/oauth/token")
        .client_id("example-client")
        .client_secret("example-client-secret")
        .scope("records.read")
        .build()?;
    let client = AuthenticationApi::builder()
        .api_key(SecretString::new("example-api-key"))
        .bearer_token(SecretString::new("example-bearer"))
        .username(SecretString::new("example-user"))
        .password(SecretString::new("example-password"))
        .oauth_provider(oauth)
        .custom_provider(ExampleProvider)
        .build()?;
    let _: authentication_api::ApiKeyRequest = client.api_key();
    let _: authentication_api::BearerRequest = client.bearer();
    let _: authentication_api::BasicRequest = client.basic();
    let _: authentication_api::OAuthRequest = client.o_auth();
    let _: authentication_api::EndpointBackedRequest = client.endpoint_backed();
    let _: authentication_api::ProviderBackedRequest = client.provider_backed();
    let _acquisition = client.login().store_as_acquired();
    Ok(())
}

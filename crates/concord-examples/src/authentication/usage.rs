//! Named secret/provider construction with no positional credential inputs.
//!
//! Required features: `oauth2-client` and `default-tls`. Mode:
//! construction-only. Documentation owners: `docs/features/authentication.md`
//! and `docs/features/credential-providers.md`.
//! Proving test: `tests/construction.rs`.

use concord::SecretString;
use concord::auth::OAuth2ClientCredentials;

use super::api::{AuthenticationApi, authentication_api};
use super::extensions::ExampleProvider;

/// Builds the configured authentication client and constructs each auth shape.
// ANCHOR: authentication_usage
pub fn construct() -> Result<(), Box<dyn std::error::Error>> {
    // ANCHOR: oauth2_provider
    let oauth = OAuth2ClientCredentials::builder("https://localhost/oauth/token")
        .client_id("example-client")
        .client_secret("example-client-secret")
        .scope("records.read")
        .build()?;
    // ANCHOR_END: oauth2_provider

    // ANCHOR: authentication_client
    let client = AuthenticationApi::builder()
        .api_key(SecretString::try_new("example-api-key")?)
        .bearer_token(SecretString::try_new("example-bearer")?)
        .username(SecretString::try_new("example-user")?)
        .password(SecretString::try_new("example-password")?)
        .oauth_provider(oauth)
        .custom_provider(ExampleProvider)
        .build()?;
    // ANCHOR_END: authentication_client

    // ANCHOR: authenticated_requests
    let _: authentication_api::ApiKeyRequest = client.api_key();
    let _: authentication_api::TokenAuthorizationRequest = client.token_authorization();
    let _: authentication_api::BearerRequest = client.bearer();
    let _: authentication_api::BasicRequest = client.basic();
    let _: authentication_api::OAuthRequest = client.o_auth();
    let _: authentication_api::EndpointBackedRequest = client.endpoint_backed();
    let _: authentication_api::ProviderBackedRequest = client.provider_backed();
    let _acquisition = client.login().store_as_acquired();
    // ANCHOR_END: authenticated_requests
    Ok(())
}
// ANCHOR_END: authentication_usage

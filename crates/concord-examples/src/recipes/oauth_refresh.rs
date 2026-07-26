//! Refreshable OAuth2 client-credentials authentication.
//!
//! Required features: `oauth2-client` and `default-tls`. Mode: construction
//! and managed execution. Documentation owner:
//! `docs/recipes/oauth-refresh.md`. Proving test: `tests/construction.rs`.

use concord::api;
use concord::auth::OAuth2ClientCredentials;

// ANCHOR: oauth_refresh_recipe
api! {
    client OAuthRecordsApi {
        base "https://records.example.test/";
        credentials {
            credential access = custom OAuth2ClientCredentials;
        }
    }

    GET records() -> NoContent {
        auth bearer access challenge unauthorized;
    }
}

/// Builds a client whose clones share one provider-backed credential slot.
pub fn build(
    token_url: &str,
    client_id: &str,
    client_secret: &str,
) -> Result<OAuthRecordsApi, Box<dyn std::error::Error>> {
    let provider = OAuth2ClientCredentials::builder(token_url)
        .client_id(client_id)
        .client_secret(client_secret)
        .scope("records.read")
        .build()?;
    let client = OAuthRecordsApi::builder()
        .access_provider(provider)
        .build()?;
    Ok(client)
}

/// Executes one provider-authenticated operation.
pub async fn fetch(client: &OAuthRecordsApi) -> Result<(), concord::ApiError> {
    client.records().await
}

/// A small application policy decision based only on stable error categories.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OAuthFailureAction {
    /// Retry later under application-owned finite backoff.
    RetryLater,
    /// Ask the credential owner to repair or rotate configuration.
    RepairCredentials,
    /// Preserve the original failure for ordinary handling.
    Propagate,
}

/// Classifies provider and protected-service authentication failures.
#[must_use]
pub fn failure_action(error: &concord::ApiError) -> OAuthFailureAction {
    match error.category() {
        concord::error::ErrorCategory::AuthenticationProvider
        | concord::error::ErrorCategory::Timeout
        | concord::error::ErrorCategory::Connect => OAuthFailureAction::RetryLater,
        concord::error::ErrorCategory::AuthenticationRejected => {
            OAuthFailureAction::RepairCredentials
        }
        _ => OAuthFailureAction::Propagate,
    }
}
// ANCHOR_END: oauth_refresh_recipe

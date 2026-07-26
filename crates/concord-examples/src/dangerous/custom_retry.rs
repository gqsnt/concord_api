//! Explicit managed-transport retry customization.
//!
//! Required feature: `dangerous-custom-retry`. Mode: construction-only.
//! Documentation owner: `docs/reference/dangerous.md`.
//! Proving test: `tests/construction.rs`.

use concord::api;

// ANCHOR: dangerous_retry_policy
pub fn bounded_custom_retry() -> concord::dangerous::ReqwestRetryBuilder {
    reqwest::retry::never()
}
// ANCHOR_END: dangerous_retry_policy

// ANCHOR: dangerous_retry_client
api! {
    client CustomRetryApi {
        base "https://custom-retry.example.test/";
    }

    GET health() -> NoContent;
}

/// Configures the dangerous retry hook explicitly on the runtime builder.
pub fn construct() -> Result<(), concord::prelude::ClientBuildError> {
    let client = CustomRetryApi::builder()
        .configure_http(|http| http.dangerous_custom_retry(bounded_custom_retry()))
        .build()?;
    let _: custom_retry_api::HealthRequest = client.health();
    Ok(())
}
// ANCHOR_END: dangerous_retry_client

//! API-key authentication in one private custom header.
//!
//! Required features: none. Mode: construction-only. Documentation owner:
//! `docs/recipes/api-key-authentication.md`. Proving test:
//! `tests/construction.rs`.

use concord::{SecretString, api};

// ANCHOR: api_key_recipe
api! {
    client ApiKeyApi {
        base "https://api-key.example.test/";
        credentials {
            secret api_key: String;
            credential application_key = api_key(secret.api_key);
        }
    }

    GET profile() -> NoContent {
        auth header "X-Api-Key" = application_key challenge never;
    }
}

/// Builds one immutable client from bounded protected input.
pub fn build(api_key: impl AsRef<str>) -> Result<ApiKeyApi, Box<dyn std::error::Error>> {
    let client = ApiKeyApi::builder()
        .api_key(SecretString::try_new(api_key)?)
        .build()?;
    Ok(client)
}
// ANCHOR_END: api_key_recipe

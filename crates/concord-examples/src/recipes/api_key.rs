//! API-key authentication in one private custom header.
//!
//! Required features: none. Mode: construction-only. Documentation owner:
//! `docs/auth.md`. Proving test:
//! `tests/construction.rs`.

use concord::{Empty, SecretString, api};

// ANCHOR: api_key_recipe
api! {
    client ApiKeyApi {
        base "https://api-key.example.test/";
        timeout 30s;

        credentials {
            secret api_key;
            credential application_key = api_key(secret.api_key);
        }

        GET profile() -> () via Empty {
            auth { header "X-Api-Key" = application_key; }
        }
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

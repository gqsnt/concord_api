//! Declares the smallest useful client and constructs its named request.
//!
//! Required features: none. Mode: construction-only. Documentation owner:
//! `docs/guide/getting-started.md`. Proving test: `tests/construction.rs`.

use concord::api;

api! {
    client MinimalApi {
        base "https://minimal.example.test/v1/";
    }

    GET health() -> NoContent;
}

/// Builds the minimal client and constructs a request without executing it.
pub fn construct() -> Result<minimal_api::HealthRequest, concord::ClientBuildError> {
    Ok(MinimalApi::builder().build()?.health())
}

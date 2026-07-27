//! Declares the smallest useful client, constructs its request, and executes
//! that request through the managed runtime.
//!
//! Required features: none. Mode: inert construction plus deterministic
//! loopback execution. Documentation owners: `docs/start/first-api.md` and
//! `docs/start/first-request.md`. Proving tests: `tests/construction.rs` and
//! `tests/local_runtime.rs`.

// ANCHOR: first_api
use concord::api;

api! {
    client MinimalApi {
        base "http://minimal.example.test/v1/";
    }

    GET health() -> NoContent {
        path ["health"];
    }
}
// ANCHOR_END: first_api

// ANCHOR: build_client
/// Builds the minimal client with Concord's default runtime components.
pub fn build_client() -> Result<MinimalApi, concord::ClientBuildError> {
    MinimalApi::new()
}
// ANCHOR_END: build_client

/// Builds the minimal client and constructs a request without executing it.
pub fn construct() -> Result<minimal_api::HealthRequest, concord::ClientBuildError> {
    Ok(build_client()?.health())
}

// ANCHOR: first_request
/// Executes one health request and accepts a declared empty response.
pub async fn execute(client: &MinimalApi) -> Result<(), concord::ApiError> {
    let request = client.health();
    request.await
}
// ANCHOR_END: first_request

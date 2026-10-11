//! Declares the smallest useful client, constructs its request, and executes
//! that request through the managed runtime.
//!
//! Required features: none. Mode: inert construction plus deterministic
//! loopback execution. Documentation owner: `docs/quick_start.md`.
//! Proving tests: `tests/construction.rs` and
//! `tests/local_runtime.rs`.
//!
//! The declaration omits `timeout`, so the root selects the visible v3 default
//! of 30 seconds. `via Empty` is an ordinary imported Rust type: no response
//! representation is guessed from the `()` output.

// ANCHOR: first_api
use concord::{Empty, api};

api! {
    client MinimalApi {
        base "http://minimal.example.test/v1/";

        GET health() -> () via Empty {
            path "health";
        }
    }
}
// ANCHOR_END: first_api

// ANCHOR: build_client
/// Builds the minimal client with Concord's default runtime components.
///
/// `new()` exists because nothing is required: no root value, secret,
/// provider or limiter choice remains.
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

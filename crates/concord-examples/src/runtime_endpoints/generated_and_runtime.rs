//! Generated and runtime-defined endpoint values share one request/executor.
//!
//! Required features: none. Mode: construction, credential-free planning, and local
//! execution. Documentation owner:
//! `docs/runtime_endpoints.md`.
//! Proving tests: `tests/construction.rs` and `tests/local_runtime.rs`.

use std::time::Duration;

use concord::api;
use concord::endpoint::{EndpointId, Request, RuntimeEndpoint};
use concord::http::{HeaderName, Method};

// ANCHOR: runtime_endpoint_client
api! {
    client PreparedApi {
        timeout 30s;
        base "http://prepared.example.test/";


    GET generated() -> () via Empty;
}}
// ANCHOR_END: runtime_endpoint_client

/// Constructs one generated and one runtime-defined request.
// ANCHOR: runtime_endpoint
pub fn construct() -> Result<(), Box<dyn std::error::Error>> {
    let client = PreparedApi::builder().build()?;
    let endpoint = RuntimeEndpoint::builder(EndpointId::new("prepared_api.lookup")?, Method::POST)
        .path("records")
        .path("42")
        .query("view", "summary")
        .header(HeaderName::from_static("x-example"), "canonical")
        .timeout(Duration::from_secs(5))
        .body_codec::<crate::request_response::specifications::ExampleCodec>(
            "request body".to_owned(),
        )
        .response::<crate::request_response::specifications::ExampleCodec, String>()
        .build()?;
    let _: Request<_> = endpoint.try_bind(&client)?;
    Ok(())
}
// ANCHOR_END: runtime_endpoint

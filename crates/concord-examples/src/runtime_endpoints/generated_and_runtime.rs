//! Generated and runtime-defined endpoint values share one request/executor.
//!
//! Required features: none. Mode: construction, safe planning, and local
//! execution. Documentation owner:
//! `docs/advanced_endpoints.md`.
//! Proving tests: `tests/construction.rs` and `tests/local_runtime.rs`.

use std::time::Duration;

use concord::api;
use concord::endpoint::{EndpointId, Request, RuntimeEndpoint};
use concord::http::{HeaderName, Method};

// ANCHOR: runtime_endpoint_client
api! {
    client PreparedApi {
        base "http://prepared.example.test/";
    }

    GET generated() -> NoContent;
}
// ANCHOR_END: runtime_endpoint_client

/// Constructs one generated and one runtime-defined request.
// ANCHOR: runtime_endpoint
pub fn construct() -> Result<(), Box<dyn std::error::Error>> {
    let client = PreparedApi::builder().build()?;
    let endpoint = RuntimeEndpoint::builder(EndpointId::new("prepared_api.lookup")?, Method::POST)
        .path_segment("records")
        .path_segment("42")
        .query("view", "summary")
        .header(HeaderName::from_static("x-example"), "canonical")
        .timeout(Duration::from_secs(5))
        .body_codec::<crate::request_response::specifications::ExampleCodec>(
            "request body".to_owned(),
        )
        .response::<crate::request_response::specifications::ExampleCodec>()
        .build()?;
    let _: Request<_> = client.request(endpoint);
    Ok(())
}
// ANCHOR_END: runtime_endpoint

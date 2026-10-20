//! Generated and runtime-defined endpoints share one owner projection and
//! one managed executor.
//!
//! Required features: none. Mode: construction, binding and credential-free
//! planning. Documentation owner: `docs/runtime_endpoints.md`.
//! Proving tests: `tests/construction.rs` and `tests/local_runtime.rs`.

use std::time::Duration;

use concord::endpoint::{BoundRuntimeRequest, EndpointId, RequestPlan, RuntimeEndpoint};
use concord::error::ConfigurationError;
use concord::http::Method;
use concord::{Empty, Text, api};

// ANCHOR: runtime_endpoint_client
api! {
    client PreparedApi {
        base "http://prepared.example.test/";
        timeout 30s;

        GET declared() -> () via Empty;

        scope regional(region: String) {
            timeout 10s;
            path "regions/{region}";
            headers { "X-Region" = region; }

            GET declared() -> () via Empty;
        }
    }
}
// ANCHOR_END: runtime_endpoint_client

// ANCHOR: runtime_endpoint
/// Builds one runtime-defined endpoint descriptor.
///
/// Construction records a symbolic program: no value is encoded, no body is
/// prepared and nothing is sent until the endpoint is bound and executed.
///
/// # Errors
///
/// The first latched structural error (invalid ID, key, method or a
/// repeated singleton selection).
pub fn lookup() -> Result<RuntimeEndpoint<Text, String>, ConfigurationError> {
    RuntimeEndpoint::builder(EndpointId::new("prepared_api.lookup")?, Method::POST)
        .path("records")
        .path("42")
        .query("view", "summary")
        .header("x-example", "canonical")
        .timeout(Duration::from_secs(5))
        .body_repr::<Text, _>("request body".to_owned())
        .response::<Text, String>()
        .build()
}

/// Binds the runtime endpoint at the client root and at a generated scope,
/// and returns the scope's inert plan URL.
///
/// The scope binding inherits the scope's path, headers and timeout exactly
/// as its generated `declared()` endpoint does; the runtime-local timeout
/// selection still wins.
///
/// # Errors
///
/// Client construction, descriptor construction, or binding failures.
pub fn construct() -> Result<String, Box<dyn std::error::Error>> {
    let client = PreparedApi::builder().build()?;
    let _root: BoundRuntimeRequest<Text, String> = client.bind(lookup()?)?;

    let scope = client.regional("eu");
    let plan: RequestPlan = scope.plan_runtime(&lookup()?)?;
    let _scoped: BoundRuntimeRequest<Text, String> = scope.bind(lookup()?)?;
    Ok(plan.observed_url().to_owned())
}
// ANCHOR_END: runtime_endpoint

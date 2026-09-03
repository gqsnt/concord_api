//! Explicit production HTTP and finite runtime configuration.
//!
//! Required feature: `default-tls`. Mode: construction-only. Documentation
//! owner: `docs/extensions.md`.

use std::num::NonZeroUsize;
use std::time::Duration;

use concord::api;
use concord::http::SafeProxy;

// ANCHOR: production_recipe
api! {
    client ProductionApi {
        timeout off;
        base "https://api.example.test/";


    GET health() -> () using NoContent {
        timeout 10s;
    }
}}

/// Builds an HTTPS-only client with an explicit proxy and private trust root.
// ANCHOR: production_client
pub fn build_client(
    proxy_origin: &str,
    root_certificate_pem: &[u8],
) -> Result<ProductionApi, Box<dyn std::error::Error>> {
    let proxy = SafeProxy::parse(proxy_origin)?;
    let root_certificate_pem = root_certificate_pem.to_vec();
    let client = ProductionApi::builder()
        .configure_http(move |http| {
            http.require_https()
                .proxy(proxy)
                .connect_timeout(Duration::from_secs(5))
                .read_timeout(Duration::from_secs(30))
                .pool_idle_timeout(Duration::from_secs(90))
                .pool_max_idle_per_host(8)
                .tcp_keepalive(Duration::from_secs(60))
                .tcp_nodelay(true)
                .add_root_certificate_pem(root_certificate_pem)
        })
        .configure_runtime(|runtime| {
            runtime
                .runtime_endpoint_timeout(Duration::from_secs(30))
                .request_body_limit(NonZeroUsize::new(8 * 1024 * 1024).expect("positive limit"))
                .response_body_limit(NonZeroUsize::new(8 * 1024 * 1024).expect("positive limit"))
                .pagination_total_response_body_limit(
                    NonZeroUsize::new(64 * 1024 * 1024).expect("positive limit"),
                )
                .stream_response_limit(NonZeroUsize::new(64 * 1024 * 1024).expect("positive limit"))
        })
        .build()?;
    Ok(client)
}
// ANCHOR_END: production_client
// ANCHOR_END: production_recipe

// ANCHOR: protocol_recovery_toggle
/// Disables private known-safe protocol recovery when deployment policy
/// requires one physical dispatch for each visible Concord attempt.
pub fn build_single_dispatch_client() -> Result<ProductionApi, concord::ClientBuildError> {
    ProductionApi::builder()
        .configure_http(|http| http.protocol_recovery(false))
        .build()
}
// ANCHOR_END: protocol_recovery_toggle

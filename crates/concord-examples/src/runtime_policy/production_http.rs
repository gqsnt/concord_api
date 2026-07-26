//! Explicit production HTTP and finite runtime configuration.
//!
//! Required feature: `default-tls`. Mode: construction-only. Documentation
//! owner: `docs/features/production-http.md`.

use std::num::NonZeroUsize;
use std::time::Duration;

use concord::api;
use concord::http::SafeProxy;

// ANCHOR: production_recipe
api! {
    client ProductionApi {
        base "https://api.example.test/";
    }

    GET health() -> NoContent {
        timeout 10s;
    }
}

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
            http.https_only(true)
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
                .stream_response_limit(NonZeroUsize::new(64 * 1024 * 1024).expect("positive limit"))
        })
        .build()?;
    Ok(client)
}
// ANCHOR_END: production_client
// ANCHOR_END: production_recipe

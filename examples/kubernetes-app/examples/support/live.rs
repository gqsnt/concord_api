//! Shared configuration of the live runners (read-only; included by `ku_a.rs` and `ku_b.rs`).
//!
//! - `KUBE_API_URL` — the API server (`https://127.0.0.1:6443`, a `kubectl proxy` URL). Unset:
//!   the runner prints `NOT_RUN` and exits 0.
//! - `KUBE_TOKEN` — optional bearer token (never printed).
//! - `KUBE_CA_FILE` — optional PEM file of the cluster CA.

use concord::reqwest;
use concord_example_kubernetes_sdk::{BearerToken, KubeContext};

pub fn context() -> Result<Option<KubeContext>, Box<dyn std::error::Error>> {
    let Ok(url) = std::env::var("KUBE_API_URL") else {
        return Ok(None);
    };
    let mut http = reqwest::Client::builder().timeout(std::time::Duration::from_secs(600));
    if let Ok(path) = std::env::var("KUBE_CA_FILE") {
        http = http.add_root_certificate(reqwest::Certificate::from_pem(&std::fs::read(path)?)?);
    }
    let mut kube = KubeContext::new(http.build()?).with_base_url(url)?;
    if let Ok(token) = std::env::var("KUBE_TOKEN") {
        kube = kube.with_bearer(&BearerToken::new(token))?;
    }
    Ok(Some(kube))
}

//! Shared live-mode configuration of the binaries (not a library module of the app).

use concord::reqwest;
use concord_example_salesforce_sdk::OrgContext;
use concord_example_salesforce_sdk::ids::{AccessToken, ApiVersion, InstanceUrl};

/// The org context from `SF_INSTANCE_URL`, `SF_ACCESS_TOKEN` and the optional `SF_API_VERSION`
/// (default: the pinned version), or the reason it is not configured.
pub fn context() -> Result<OrgContext, String> {
    let instance = std::env::var("SF_INSTANCE_URL").map_err(|_| "SF_INSTANCE_URL is not set")?;
    let token = std::env::var("SF_ACCESS_TOKEN").map_err(|_| "SF_ACCESS_TOKEN is not set")?;
    let version = match std::env::var("SF_API_VERSION") {
        Ok(v) => v.parse::<ApiVersion>().map_err(|e| e.to_string())?,
        Err(_) => ApiVersion::PINNED,
    };
    let instance = InstanceUrl::parse(&instance).map_err(|e| e.to_string())?;
    let token = AccessToken::new(token).map_err(|e| e.to_string())?;
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|_| "the HTTP client could not be built".to_owned())?;
    OrgContext::new(http, instance, version, token).map_err(|e| e.to_string())
}

/// Whether an explicit opt-in variable is exactly `1`.
pub fn opted_in(variable: &str) -> bool {
    std::env::var(variable).is_ok_and(|v| v == "1")
}

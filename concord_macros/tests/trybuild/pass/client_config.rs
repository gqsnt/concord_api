use concord_core::advanced::{NoopCacheStore, NoopRateLimiter};
use concord_core::prelude::*;
use concord_macros::api;
use std::sync::Arc;
use self::usage_config_api::UsageConfigApi;

api! {
    client UsageConfigApi {
        base "https://example.com"
        var tenant: String
        secret api_key: String
        credential key = api_key(secret.api_key)
    }

    GET Ping
        path ["ping"]
        auth header "X-Api-Key" = key
        -> Json<String>
}

fn construction_and_configure() -> Result<(), ApiClientError> {
    let _api = UsageConfigApi::new("acme".to_string(), "key".to_string());

    let _api = UsageConfigApi::builder()
        .tenant("acme".to_string())
        .api_key("key".to_string())
        .build()?;

    let _api = UsageConfigApi::new("acme".to_string(), "key".to_string())
        .configure(|cfg| {
            cfg.debug(DebugLevel::V);
            cfg.pagination_detect_loops(true);
        });

    let _api = UsageConfigApi::new("acme".to_string(), "key".to_string())
        .configure(|cfg| {
            cfg.cache_store(Arc::new(NoopCacheStore));
            cfg.rate_limiter(Arc::new(NoopRateLimiter));
        });

    let mut api = UsageConfigApi::new("acme".to_string(), "key".to_string());
    api.configure_mut(|cfg| {
        cfg.debug(DebugLevel::V);
        cfg.pagination_detect_loops(false);
    });

    Ok(())
}

fn main() {}

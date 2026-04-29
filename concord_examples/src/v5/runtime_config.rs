use concord_core::prelude::*;
use concord_macros::api;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Health {
    pub ok: bool,
}

api! {
    client RuntimeConfigApi {
        base https "example.com"

        default {
            retry read
        }

        retry read {
            max_attempts 2
            methods [GET]
            on [429, 500]
            retry_after
        }
    }

    GET HealthCheck
        as health
        path ["health"]
        -> Json<Health>
}

pub async fn configured_client() -> Result<Health, ApiClientError> {
    let api = runtime_config_api::RuntimeConfigApi::new()
        .with_debug_level(DebugLevel::V)
        .configure(|cfg| {
            cfg.pagination.max_pages = 10;
            cfg.pagination.max_items = 1_000;
        });

    api.health().await
}

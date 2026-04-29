use concord_core::prelude::*;
use concord_macros::api;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub id: String,
}

api! {
    client FmtApi {
        base https "example.com"
        var tenant: String

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

    scope tenant {
        host [vars.tenant]

        GET Search(prefix: String, q?: String)
            as search
            path ["search", fmt["prefix-", prefix]]
            -> Json<Vec<SearchHit>>
        {
            query {
                q
                "trace" = fmt["tenant:", vars.tenant, ":prefix:", prefix]
            }
            headers {
                "x-tenant-prefix" = fmt[vars.tenant, ":", prefix]
            }
        }
    }
}

pub async fn formatted_values() -> Result<Vec<SearchHit>, ApiClientError> {
    let api = fmt_api::FmtApi::new("acme".to_string());

    api.tenant()
        .search("users".to_string())
        .q("alice".to_string())
        .await
}

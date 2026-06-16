use concord_core::prelude::*;
use concord_macros::api;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    pub id: u64,
}

api! {
    client BehaviorRateLimitKeyApi {
        base "https://example.com"

        rate_limit tenant_bucket {
            bucket method by [tenant_key] {
                5 / 1s
            }
        }

        behavior tenant_read {
            rate_limit tenant_bucket
        }
    }

    scope tenants(tenant: String) {
        path ["tenants", tenant]
        rate_limit key tenant_key = tenant
        behavior tenant_read

        GET List
        as list
        path ["items"]
        -> Json<Vec<Item>>
    }
}

fn main() {}

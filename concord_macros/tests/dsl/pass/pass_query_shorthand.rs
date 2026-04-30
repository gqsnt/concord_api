use concord_core::prelude::*;
use concord_macros::api;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResponse;

api! {
    client CurrentQueryApi {
        base "https://example.com"
        var tenant: String
    }

    scope tenant(tenant: String) {
        path [tenant]

        GET Search(count: u32 = 20, page?: u32, start_time?: u64)
            as search
            path ["search"]
            query {
                count
                page
                "startTime" = start_time
                tenant
            }
            -> Json<SearchResponse>
    }
}

fn main() {}

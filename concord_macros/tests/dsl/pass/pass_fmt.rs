use concord_core::prelude::*;
use concord_macros::api;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Item;

api! {
    client V5FmtApi {
        base https "example.com"
        var tenant_id: String
        var trace_id: String
    }

    scope tenant(tenant_id: String) {
        host [fmt["tenant-", tenant_id], "api"]
        path ["users", fmt["u-", tenant_id]]
        headers { "x-trace" = fmt["trace-", vars.trace_id] }

        GET Search(prefix?: String, start: u32 = 0, count: u32 = 20)
            as search
            path ["search"]
            query {
                "range" = fmt[start, "-", count]
                "q" = fmt["prefix:", prefix]
            }
            -> Json<Vec<Item>>
    }
}

fn main() {}

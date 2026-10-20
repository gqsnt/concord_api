// IDENTITY-01 / V3-R05: the `endpoint` dimension consumes the operational
// endpoint ID, so every covered endpoint must pin an explicit ID; a
// navigation-derived ID changes on renames.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client Api {
        base "https://example.test";

        scope regional() {
            rate_limit {
                id "concord.rate_limit_endpoint_dimension_requires_id.1";
                key [origin, endpoint];
                allow 1 per 1s;
            }

            GET pinned() -> Bytes via Binary { id "api.regional.pinned"; }
            GET derived() -> Bytes via Binary;
        }
    }
}

fn main() {}

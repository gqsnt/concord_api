use concord_core::prelude::*;
use concord_macros::api;

api! {
    client ZeroRateLimitCostApi {
        base "https://example.com"

        rate_limit app {
            bucket application by [endpoint] {
                cost 0
                10 / 1s
            }
        }
    }

    GET Ping
    path ["ping"]
    rate_limit app
    -> Text<String>
}

fn main() {}

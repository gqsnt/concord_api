use concord_core::prelude::*;
use concord_macros::api;

api! {
    client ZeroRateLimitMaxApi {
        base "https://example.com"

        rate_limit app {
            bucket application by [endpoint] {
                0 / 1s
            }
        }
    }

    GET Ping
    path ["ping"]
    rate_limit app
    -> Text<String>
}

fn main() {}

use concord_core::prelude::*;
use concord_macros::api;

api! {
    client RateLimitCostExceedsMaxApi {
        base "https://example.com"

        policies {
            rate_limit expensive {
                bucket application by [host] {
                    cost 10
                    5 / 1s
                }
            }
        }

        defaults {
            rate_limit expensive
        }
    }

    GET Ping
    path ["ping"]
    -> Text<String>
}

fn main() {}

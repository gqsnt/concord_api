use concord_core::prelude::*;
use concord_macros::api;

api! {
    client DuplicateRateLimitAcrossLayersAllowedApi {
        base "https://example.com"

        rate_limit app {
            bucket application by [host] {
                10 / 1s
            }
        }

        defaults {
            rate_limit app
        }
    }

    scope users {
        path ["users"]
        rate_limit app

        GET Ping
        path ["ping"]
        -> Text<String>
    }
}

fn main() {}

use concord_core::prelude::*;
use concord_macros::api;

api! {
    client OldRateLimitEveryApi {
        base https "example.com"
        rate_limit app {
            limit 500 every 10 seconds
        }
    }
}

fn main() {}

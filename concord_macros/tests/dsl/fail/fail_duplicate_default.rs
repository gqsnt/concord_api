use concord_core::prelude::*;
use concord_macros::api;

api! {
    client DuplicateDefaultApi {
        base "https://example.com"
        default { retry read }
        default { rate_limit app }

        retry read { max_attempts 2 }
        rate_limit app {
            bucket application by [host] { 1 / 1s }
        }
    }

    GET Ping
        path ["ping"]
        -> Json<String>
}

fn main() {}

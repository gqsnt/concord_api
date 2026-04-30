use concord_core::prelude::*;
use concord_macros::api;

api! {
    client MaxAttemptsZeroApi {
        base https "example.com"

        retry read {
            max_attempts 0
            methods [GET]
            on [500]
        }
    }
}

fn main() {}

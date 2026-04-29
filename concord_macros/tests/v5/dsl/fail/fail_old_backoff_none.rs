use concord_core::prelude::*;
use concord_macros::api;

api! {
    client OldBackoffNoneApi {
        base https "example.com"
        retry read {
            max_attempts 2
            backoff none
        }
    }
}

fn main() {}

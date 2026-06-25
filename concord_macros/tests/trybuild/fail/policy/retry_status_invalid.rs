use concord_core::prelude::*;
use concord_macros::api;

api! {
    client InvalidRetryStatusApi {
        base "https://example.com"

        retry read {
            max_attempts 2
            methods [GET]
            on [99]
        }
    }

    GET Ping
    path ["ping"]
    retry read
    -> Text<String>
}

fn main() {}

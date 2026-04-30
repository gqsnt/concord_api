use concord_core::prelude::*;
use concord_macros::api;

api! {
    client PolicyApi {
        base https "example.com"

        default {
            retry read
            cache standard
            rate_limit app
        }

        retry read {
            max_attempts 2
            methods [GET]
            on [429, 500]
            retry_after
        }

        cache standard {
            ttl 60s
            revalidate
            on_error serve_stale
        }

        rate_limit app {
            bucket application by [host] {
                100 / 1s
            }
        }
    }

    GET Text
        as text
        path ["text"]
        -> Text<String>

    GET RetryOnly
        as retry_only
        path ["retry"]
        cache off
        rate_limit off
        -> Text<String>

    GET RateLimited
        as rate_limited
        path ["limited"]
        cache off
        -> Text<String>
}

pub use self::policy_api::PolicyApi;

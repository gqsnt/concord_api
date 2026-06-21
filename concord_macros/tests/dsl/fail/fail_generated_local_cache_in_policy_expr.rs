use concord_core::prelude::*;
use concord_macros::api;

api! {
    client GeneratedLocalCachePolicyApi {
        base "https://example.com"
    }

    GET Ping
    path ["ping"]
    header "X-Leak" = cache.key()
    -> Text<String>
}

fn main() {}

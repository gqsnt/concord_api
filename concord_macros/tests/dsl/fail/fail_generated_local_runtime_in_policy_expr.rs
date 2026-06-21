use concord_core::prelude::*;
use concord_macros::api;

api! {
    client GeneratedLocalRuntimePolicyApi {
        base "https://example.com"
    }

    GET Ping
    path ["ping"]
    header "X-Leak" = runtime.config()
    -> Text<String>
}

fn main() {}

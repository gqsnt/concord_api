use concord_core::prelude::*;
use concord_macros::api;

api! {
    client GeneratedLocalPolicyPolicyApi {
        base "https://example.com"
    }

    GET Ping
    path ["ping"]
    header "X-Leak" = policy.clone()
    -> Text<String>
}

fn main() {}

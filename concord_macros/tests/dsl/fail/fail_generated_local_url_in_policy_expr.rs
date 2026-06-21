use concord_core::prelude::*;
use concord_macros::api;

api! {
    client GeneratedLocalUrlPolicyApi {
        base "https://example.com"
    }

    GET Ping
    path ["ping"]
    header "X-Leak" = url.to_string()
    -> Text<String>
}

fn main() {}

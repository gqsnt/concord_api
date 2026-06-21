use concord_core::prelude::*;
use concord_macros::api;

api! {
    client GeneratedLocalHeadersPolicyApi {
        base "https://example.com"
    }

    GET Ping
    path ["ping"]
    header "X-Leak" = headers.len()
    -> Text<String>
}

fn main() {}

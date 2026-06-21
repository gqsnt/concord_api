use concord_core::prelude::*;
use concord_macros::api;

api! {
    client GeneratedLocalRequestPolicyApi {
        base "https://example.com"
    }

    GET Ping
    path ["ping"]
    header "X-Leak" = request.url.to_string()
    -> Text<String>
}

fn main() {}

use concord_core::prelude::*;
use concord_macros::api;

api! {
    client GeneratedLocalClientPolicyApi {
        base "https://example.com"
    }

    GET Ping
    path ["ping"]
    header "X-Leak" = client.name()
    -> Text<String>
}

fn main() {}

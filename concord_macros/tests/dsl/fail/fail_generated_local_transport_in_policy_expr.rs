use concord_core::prelude::*;
use concord_macros::api;

api! {
    client GeneratedLocalTransportPolicyApi {
        base "https://example.com"
    }

    GET Ping
    path ["ping"]
    header "X-Leak" = transport.send()
    -> Text<String>
}

fn main() {}

use concord_core::prelude::*;
use concord_macros::api;

api! {
    client GeneratedLocalEpPolicyApi {
        base "https://example.com"
    }

    GET Ping
    path ["ping"]
    header "X-Leak" = format!("{}", ep.id)
    -> Text<String>
}

fn main() {}

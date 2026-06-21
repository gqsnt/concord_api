use concord_core::prelude::*;
use concord_macros::api;

api! {
    client GeneratedLocalReqPolicyApi {
        base "https://example.com"
    }

    GET Ping
    path ["ping"]
    header "X-Leak" = req.url()
    -> Text<String>
}

fn main() {}

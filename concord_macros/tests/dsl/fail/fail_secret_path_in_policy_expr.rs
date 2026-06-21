use concord_core::prelude::*;
use concord_macros::api;

api! {
    client SecretPathPolicyApi {
        base "https://example.com"
    }

    GET Ping
    path ["ping"]
    header "X-Leak" = secret::token()
    -> Text<String>
}

fn main() {}

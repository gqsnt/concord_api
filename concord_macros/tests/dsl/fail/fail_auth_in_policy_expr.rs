use concord_core::prelude::*;
use concord_macros::api;

api! {
    client AuthPolicyExprApi {
        base "https://example.com"
    }

    GET Ping
    path ["ping"]
    header "X-Leak" = auth.token
    -> Text<String>
}

fn main() {}

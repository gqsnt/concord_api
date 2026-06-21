use concord_core::prelude::*;
use concord_macros::api;

api! {
    client SecretBlockPolicyApi {
        base "https://example.com"
        secret token: String
    }

    GET Ping
    path ["ping"]
    header "X-Leak" = { let x = secret.token; x }
    -> Text<String>
}

fn main() {}

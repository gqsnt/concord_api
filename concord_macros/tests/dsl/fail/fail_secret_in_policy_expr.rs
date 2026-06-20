use concord_core::prelude::*;
use concord_macros::api;

api! {
    client SecretPolicyExprApi {
        base "https://example.com"
        secret api_key: String
    }

    GET Ping
    path ["ping"]
    header "X-Api-Key" = secret.api_key
    -> Text<String>
}

fn main() {}

use concord_core::prelude::*;
use concord_macros::api;

api! {
    client SecretPolicyFmtApi {
        base "https://example.com"
        secret api_key: String
    }

    GET Ping
    path ["ping"]
    query {
        "token" = fmt["bearer-", secret.api_key]
    }
    -> Text<String>
}

fn main() {}

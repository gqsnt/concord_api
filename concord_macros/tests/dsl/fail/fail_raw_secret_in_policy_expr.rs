use concord_core::prelude::*;
use concord_macros::api;

api! {
    client RawSecretPolicyExprApi {
        base "https://example.com"
        secret token: String
    }

    GET Ping
    path ["ping"]
    header "X-Leak" = r#secret.token
    -> Text<String>
}

fn main() {}

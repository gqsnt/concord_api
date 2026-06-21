use concord_core::prelude::*;
use concord_macros::api;

api! {
    client RawAuthPolicyExprApi {
        base "https://example.com"
    }

    GET Ping
    path ["ping"]
    header "X-Leak" = r#auth.token
    -> Text<String>
}

fn main() {}

use concord_core::prelude::*;
use concord_macros::api;

api! {
    client RawCxPolicyExprApi {
        base "https://example.com"
    }

    GET Ping
    path ["ping"]
    header "X-Leak" = r#cx.token
    -> Text<String>
}

fn main() {}

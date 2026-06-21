use concord_core::prelude::*;
use concord_macros::api;

api! {
    client RawRequestPolicyExprApi {
        base "https://example.com"
    }

    GET Ping
    path ["ping"]
    header "X-Leak" = r#request.url.to_string()
    -> Text<String>
}

fn main() {}

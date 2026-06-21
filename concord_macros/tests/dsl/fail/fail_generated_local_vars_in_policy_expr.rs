use concord_core::prelude::*;
use concord_macros::api;

api! {
    client GeneratedLocalVarsPolicyApi {
        base "https://example.com"
        var token: String
    }

    GET Ping
    path ["ping"]
    header "X-Leak" = format!("{}", vars.token)
    -> Text<String>
}

fn main() {}

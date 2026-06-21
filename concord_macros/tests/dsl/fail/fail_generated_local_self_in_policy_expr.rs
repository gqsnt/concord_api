use concord_core::prelude::*;
use concord_macros::api;

api! {
    client GeneratedLocalSelfPolicyApi {
        base "https://example.com"
    }

    GET Ping
    path ["ping"]
    header "X-Leak" = self.token
    -> Text<String>
}

fn main() {}

use concord_core::prelude::*;
use concord_macros::api;

api! {
    client RawAuthRouteAtomApi {
        base "https://example.com"
    }

    GET Ping
    path [r#auth.token]
    -> Text<String>
}

fn main() {}

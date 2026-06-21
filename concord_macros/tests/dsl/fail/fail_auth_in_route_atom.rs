use concord_core::prelude::*;
use concord_macros::api;

api! {
    client AuthRouteAtomApi {
        base "https://example.com"
    }

    GET Ping
    path [auth.token]
    -> Text<String>
}

fn main() {}

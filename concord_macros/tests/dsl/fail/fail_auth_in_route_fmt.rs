use concord_core::prelude::*;
use concord_macros::api;

api! {
    client AuthRouteFmtApi {
        base "https://example.com"
    }

    GET Ping
    path [fmt["prefix-", auth.token]]
    -> Text<String>
}

fn main() {}

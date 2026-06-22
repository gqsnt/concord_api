use concord_core::prelude::*;
use concord_macros::api;

api! {
    client InvalidStaticHeaderValueApi {
        base "https://example.com"
    }

    GET Ping
    path ["ping"]
    header "X-Bad" = "bad\nvalue"
    -> Text<String>
}

fn main() {}

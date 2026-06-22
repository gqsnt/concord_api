use concord_core::prelude::*;
use concord_macros::api;

api! {
    client BaseQueryApi {
        base "https://example.com?api_key=value"
    }
}

fn main() {}

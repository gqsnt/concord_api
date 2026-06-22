use concord_core::prelude::*;
use concord_macros::api;

api! {
    client BaseBackslashApi {
        base "https://example.com\\evil"
    }
}

fn main() {}

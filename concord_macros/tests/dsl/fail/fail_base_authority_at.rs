use concord_core::prelude::*;
use concord_macros::api;

api! {
    client BaseAuthorityAtApi {
        base "https://user@example.com"
    }
}

fn main() {}

use concord_core::prelude::*;
use concord_macros::api;

api! {
    client BaseFragmentApi {
        base "https://example.com#fragment"
    }
}

fn main() {}

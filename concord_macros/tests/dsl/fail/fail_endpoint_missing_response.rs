use concord_core::prelude::*;
use concord_macros::api;

api! {
    client MissingResponseApi { base "https://example.com" }

    GET Broken
        path ["broken"]
}

fn main() {}

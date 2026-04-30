use concord_core::prelude::*;
use concord_macros::api;

api! {
    client DuplicateResponseApi { base https "example.com" }

    GET Broken
        path ["broken"]
        -> Json<String>
        -> Json<u32>
}

fn main() {}

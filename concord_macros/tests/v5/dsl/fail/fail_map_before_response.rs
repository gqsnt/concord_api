use concord_core::prelude::*;
use concord_macros::api;

api! {
    client MapBeforeResponseApi { base https "example.com" }

    GET Broken
        path ["broken"]
        map String { r }
        -> Json<String>
}

fn main() {}

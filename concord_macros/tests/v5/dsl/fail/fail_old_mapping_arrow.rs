use concord_core::prelude::*;
use concord_macros::api;

api! {
    client OldMappingArrowApi { base https "example.com" }

    GET Broken
        path ["broken"]
        -> Json<String> | String => r
}

fn main() {}

use concord_core::prelude::*;
use concord_macros::api;

api! {
    client OldEndpointOuterBlockApi { base https "example.com" }

    GET Broken {
        path ["broken"]
        -> Json<String>
    }
}

fn main() {}

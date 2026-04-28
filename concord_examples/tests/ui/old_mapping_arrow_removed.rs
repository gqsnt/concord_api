use concord_core::prelude::*;
use concord_macros::api;

api! {
    client OldMappingSyntax {
        base https "example.com"
    }

    GET Ping -> Json<String> | String => r;
}

fn main() {}

use concord_core::prelude::*;
use concord_macros::api;

api! {
    client OldClientSchemeHostApi {
        scheme: https
        host: "example.com"
    }
}

fn main() {}

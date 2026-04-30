use concord_core::prelude::*;
use concord_macros::api;

api! {
    client FmtEmptyApi { base https "example.com" }

    GET Broken
        path [fmt[]]
        -> Json<String>
}

fn main() {}

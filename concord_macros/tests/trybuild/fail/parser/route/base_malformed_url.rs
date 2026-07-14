use concord_core::prelude::*;
use concord_macros::api;

api! {
    client MalformedBaseApi { base "https://example.com/api" }
}

fn main() {}

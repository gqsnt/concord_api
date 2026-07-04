use concord_core::prelude::*;
use concord_macros::api;

api! {
    client QueryUnknownApi { base "https://example.com" }

    GET Broken(count: u32)
        path ["search"]
        query {
            cout
        }
        -> Json<String>
}

fn main() {}

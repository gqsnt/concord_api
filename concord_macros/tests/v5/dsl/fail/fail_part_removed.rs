use concord_core::prelude::*;
use concord_macros::api;

api! {
    client PartRemovedApi { base https "example.com" }

    GET Broken(id: u64)
        path [part["u-", id]]
        -> Json<String>
}

fn main() {}

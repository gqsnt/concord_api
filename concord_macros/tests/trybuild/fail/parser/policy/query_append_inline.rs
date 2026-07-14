#![allow(unused_imports)]
use concord_core::prelude::*;
use concord_macros::api;

api! {
    client InvalidQueryAppendInline {
        base "https://example.com"
    }

    GET Search(tag: String)
        path ["search"]
        query "tag" += tag,
        -> Json<String>
}

fn main() {}

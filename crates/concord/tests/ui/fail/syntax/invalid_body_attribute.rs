// §3.5: v1 `#[body] name: Spec` is not an attribute Concord knows; unknown
// attributes are rejected rather than silently discarded. A body is written
// `body name: Input via Representation`.
#![allow(unused_imports)]

use concord::{Binary, Bytes, Text, api};

api! {
    client InvalidBodyAttributeApi {
        base "http://body.example.test/";

        POST submit(#[body(replay)] payload: Text) -> Bytes via Binary;
    }
}

fn main() {}

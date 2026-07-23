#![allow(unused_imports)]

use concord::{Bytes, Text, api};

api! {
    client InvalidBodyAttributeApi {
        base "http://body.example.test/";
    }

    POST submit(#[body(replay)] payload: Text) -> Bytes;
}

fn main() {}

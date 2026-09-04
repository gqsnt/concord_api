#![allow(unused_imports)]

use concord::{Bytes, Text, api};

api! {
    client InvalidBodyAttributeApi {
        timeout 30s;
        base "http://body.example.test/";


    POST submit(#[body(replay)] payload: Text) -> Bytes via Binary;
}}

fn main() {}

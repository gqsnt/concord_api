#![allow(unused_imports)]

use concord::{Bytes, Text, api};

api! {
    client InvalidBodyAttributeApi {
        timeout off;
        base "http://body.example.test/";


    POST submit(#[body(replay)] payload: Text) -> Bytes using Bytes;
}}

fn main() {}

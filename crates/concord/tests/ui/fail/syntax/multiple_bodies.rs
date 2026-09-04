#![allow(unused_imports)]

use concord::{Bytes, Text, api};

api! {
    client MultipleBodiesApi {
        timeout 30s;
        base "http://body.example.test/";


    POST submit(body first: String via Text, body second: Bytes via Binary) -> Bytes via Binary;
}}

fn main() {}

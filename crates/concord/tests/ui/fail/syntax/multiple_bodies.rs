#![allow(unused_imports)]

use concord::{Bytes, Text, api};

api! {
    client MultipleBodiesApi {
        timeout off;
        base "http://body.example.test/";


    POST submit(body first: String using Text, body second: Bytes using Bytes) -> Bytes using Bytes;
}}

fn main() {}

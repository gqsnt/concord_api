#![allow(unused_imports)]

use concord::{Bytes, Text, api};

api! {
    client MultipleBodiesApi {
        base "http://body.example.test/";
    }

    POST submit(#[body] first: Text, #[body] second: Bytes) -> Bytes;
}

fn main() {}

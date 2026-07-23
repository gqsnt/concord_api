#![allow(unused_imports)]

use concord::{Bytes, Text, api};

api! {
    client InferredBodyApi {
        base "http://body.example.test/";
    }

    POST submit(body: Text) -> Bytes;
}

fn main() {}

#![allow(unused_imports)]

use concord::{Bytes, api};

api! {
    client DuplicateIdApi {
        timeout off;
        base "http://identity.example.test/";


    GET first() -> Bytes using Bytes {
        id "identity.same";
    }
    GET second() -> Bytes using Bytes {
        id "identity.same";
    }
}}

fn main() {}

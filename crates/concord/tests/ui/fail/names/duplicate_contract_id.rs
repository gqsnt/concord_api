#![allow(unused_imports)]

use concord::{Bytes, api};

api! {
    client DuplicateIdApi {
        timeout 30s;
        base "http://identity.example.test/";


    GET first() -> Bytes via Binary {
        id "identity.same";
    }
    GET second() -> Bytes via Binary {
        id "identity.same";
    }
}}

fn main() {}

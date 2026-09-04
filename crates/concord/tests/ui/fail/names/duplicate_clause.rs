#![allow(unused_imports)]

use concord::{Bytes, api};

api! {
    client DuplicateClauseApi {
        timeout 30s;
        base "http://duplicate.example.test/";


    GET read() -> Bytes via Binary {
        timeout 1s;
        path ["read"];
        timeout 2s;
    }
}}

fn main() {}

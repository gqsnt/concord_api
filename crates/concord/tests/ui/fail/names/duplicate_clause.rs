#![allow(unused_imports)]

use concord::{Bytes, api};

api! {
    client DuplicateClauseApi {
        timeout off;
        base "http://duplicate.example.test/";


    GET read() -> Bytes using Bytes {
        timeout 1s;
        path ["read"];
        timeout 2s;
    }
}}

fn main() {}

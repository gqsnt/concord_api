#![allow(unused_imports)]

use concord::{Bytes, api};

api! {
    client DuplicateClauseApi {
        base "http://duplicate.example.test/";
    }

    GET read() -> Bytes {
        timeout 1s;
        path ["read"];
        timeout 2s;
    }
}

fn main() {}

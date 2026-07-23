#![allow(unused_imports)]

use concord::{Bytes, api};

api! {
    client DuplicateIdApi {
        base "http://identity.example.test/";
    }

    GET first() -> Bytes {
        id "identity.same";
    }
    GET second() -> Bytes {
        id "identity.same";
    }
}

fn main() {}

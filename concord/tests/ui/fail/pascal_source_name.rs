#![allow(unused_imports)]

use concord::{Bytes, api};

api! {
    client PascalSourceApi {
        base "http://names.example.test/";
    }

    GET HTTPStatus() -> Bytes;
}

fn main() {}

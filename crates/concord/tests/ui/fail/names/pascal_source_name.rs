#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client PascalSourceApi {
        base "http://names.example.test/";

        GET HTTPStatus() -> Bytes via Binary;
    }
}

fn main() {}

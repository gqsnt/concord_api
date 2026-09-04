#![allow(unused_imports)]

use concord::{Bytes, api};

api! {
    client PascalSourceApi {
        timeout 30s;
        base "http://names.example.test/";


    GET HTTPStatus() -> Bytes via Binary;
}}

fn main() {}

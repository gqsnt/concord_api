#![allow(unused_imports)]

use concord::{Bytes, api};

api! {
    client PascalSourceApi {
        timeout off;
        base "http://names.example.test/";


    GET HTTPStatus() -> Bytes using Bytes;
}}

fn main() {}

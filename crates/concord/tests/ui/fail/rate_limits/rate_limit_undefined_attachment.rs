// §12.1: a named attachment references a declared policy.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client Api {
        base "https://example.test";
        rate_limit missing();

        GET read() -> Bytes via Binary;
    }
}

fn main() {}

// One closed invocation is the unit of semantic validation (§1.4): every
// scope and endpoint lives inside its client's braces.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client NestedChildApi {
        base "https://example.test";
    }

    scope users() {
        GET list() -> Bytes via Binary;
    }
}

fn main() {}

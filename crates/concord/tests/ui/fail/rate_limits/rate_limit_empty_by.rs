// §12.3: every key has at least one dimension.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client Api {
        base "https://example.test";
        rate_limit { id "concord.rate_limit_empty_by.1"; key []; allow 1 per 1s; }

        GET read() -> Bytes via Binary;
    }
}

fn main() {}

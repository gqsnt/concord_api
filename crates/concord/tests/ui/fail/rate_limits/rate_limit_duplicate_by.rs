// §12.1: an inline policy has exactly one `key [...]` after its `id`.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client Api {
        base "https://example.test";
        rate_limit {
            id "concord.rate_limit_duplicate_by.1";
            key [origin];
            key [endpoint];
            allow 1 per 1s;
        }

        GET read() -> Bytes via Binary;
    }
}

fn main() {}

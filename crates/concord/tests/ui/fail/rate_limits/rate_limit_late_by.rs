// §12.1: an inline policy is `id; key [...]; cost N;? windows`.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client Api {
        base "https://example.test";
        rate_limit {
            id "concord.rate_limit_late_by.1";
            allow 1 per 1s;
            key [origin];
        }
    }
}

fn main() {}

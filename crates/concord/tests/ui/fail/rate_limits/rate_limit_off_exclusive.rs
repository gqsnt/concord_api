// RATE-01: attachments are additive; no descendant, runtime endpoint or
// request control can disable an ancestor constraint, so there is no
// `rate_limit off`.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client Api {
        base "https://example.test";
        rate_limit { id "concord.rate_limit_off_exclusive.1"; key [origin]; allow 1 per 1s; }

        GET read() -> Bytes via Binary {
            rate_limit off;
        }
    }
}

fn main() {}

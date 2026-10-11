// §3.3: the rate registry is client-only.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client Api {
        base "https://example.test";

        scope accounts() {
            rate_limits {
                policy account_reads() { id "concord.rate_limits_scope_block.1"; key [origin]; allow 1 per 1s; }
            }
            GET read() -> Bytes via Binary;
        }
    }
}

fn main() {}

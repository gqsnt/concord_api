// LANG-03: a scope-level policy cannot partition by a descendant endpoint's
// value.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client Api {
        base "https://example.test";

        scope regional(query region: String) {
            rate_limit {
                id "concord.rate_limit_scope_endpoint_dimension.1";
                key ["account" = account];
                allow 1 per 1s;
            }

            GET read(query account: String) -> Bytes via Binary;
        }
    }
}

fn main() {}

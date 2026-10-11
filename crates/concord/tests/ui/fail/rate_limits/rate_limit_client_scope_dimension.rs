// LANG-03: a client-level policy cannot reference a descendant scope's
// value; owner qualification names only visible ancestors.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client Api {
        base "https://example.test";
        rate_limit {
            id "concord.rate_limit_client_scope_dimension.1";
            key ["region" = regional.region];
            allow 1 per 1s;
        }

        scope regional(query region: String) {
            GET read() -> Bytes via Binary;
        }
    }
}

fn main() {}

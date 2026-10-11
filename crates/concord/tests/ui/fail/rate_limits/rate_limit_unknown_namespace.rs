// LANG-03: an owner qualifier names the client or a visible ancestor scope.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client Api {
        base "https://example.test";
        rate_limit {
            id "concord.rate_limit_unknown_namespace.1";
            key ["tenant" = other.tenant];
            allow 1 per 1s;
        }
    }
}

fn main() {}

// RATE-01 / §12.1: bindings are named and exact; built-in dimensions are
// not formals, so binding `host` (or `origin`) is an unknown binding.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client Api {
        base "https://example.test";
        values { query tenant: String; }
        rate_limits {
            policy account_reads(account) {
                id "concord.rate_limit_bind_builtin_dimension.1";
                key [origin, "account" = account];
                allow 1 per 1s;
            }
        }
        rate_limit account_reads(host = tenant, account = tenant);

        GET read() -> Bytes via Binary;
    }
}

fn main() {}

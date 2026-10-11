// §12.1: bindings are exact; no unknown or positional binding.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client Api {
        base "https://example.test";
        values { query tenant: String; }
        rate_limits {
            policy account_reads(account) {
                id "concord.rate_limit_unknown_named_binding.1";
                key ["account" = account];
                allow 1 per 1s;
            }
        }
        rate_limit account_reads(tenant = tenant);

        GET read() -> Bytes via Binary;
    }
}

api! {
    client PositionalApi {
        base "https://example.test";
        values { query tenant: String; }
        rate_limits {
            policy account_reads(account) {
                id "concord.rate_limit_unknown_named_binding.2";
                key ["account" = account];
                allow 1 per 1s;
            }
        }
        rate_limit account_reads(tenant);

        GET read() -> Bytes via Binary;
    }
}

fn main() {}

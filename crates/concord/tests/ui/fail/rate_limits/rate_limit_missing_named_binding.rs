// §12.1: named bindings are exact and total.
#![allow(unused_imports)]

use concord::api;

api! {
    client Api {
        base "https://example.test";
        rate_limits {
            policy account_reads(account) {
                id "concord.rate_limit_missing_named_binding.1";
                key ["account" = account];
                allow 1 per 1s;
            }
        }
        rate_limit account_reads();
    }
}

fn main() {}

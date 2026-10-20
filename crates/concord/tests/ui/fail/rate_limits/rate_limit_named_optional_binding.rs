// §12.3: a custom dimension uses a required ordinary value; an outer-optional
// slot is rejected, including through a named formal.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client Api {
        base "https://example.test";
        values { query account?: String; }
        rate_limits {
            policy account_reads(account) {
                id "concord.rate_limit_named_optional_binding.1";
                key ["account" = account];
                allow 1 per 1s;
            }
        }
        rate_limit account_reads(account = account);

        GET read() -> Bytes via Binary;
    }
}

fn main() {}

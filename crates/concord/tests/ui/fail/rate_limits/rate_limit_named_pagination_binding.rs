// PAGE-01 / V3-P02: a cursor destination cannot participate in an effective
// rate key, directly or through a named formal (a page-live value would
// silently move each page into a different bucket).
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

use concord::pagination::{CursorInit, CursorPagination};

api! {
    client Api {
        base "https://example.test";
        rate_limits {
            policy account_reads(account) {
                id "concord.rate_limit_named_pagination_binding.1";
                key ["account" = account];
                allow 1 per 1s;
            }
        }

        GET list(query account: String) -> Bytes via Binary {
            rate_limit account_reads(account = account);
            paginate via CursorPagination<String> {
                init CursorInit { cursor: account };
                cursor -> account;
            }
        }
    }
}

fn main() {}

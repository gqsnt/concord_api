// PAGE-01: an inherited scope policy keyed by a scope value is fine, but that
// scope value can never become a cursor destination.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

use concord::pagination::{CursorInit, CursorPagination};

api! {
    client Api {
        base "https://example.test";

        scope pages(query cursor: String) {
            rate_limit {
                id "concord.rate_limit_pagination_inherited_scope_rule.1";
                key ["cursor" = cursor];
                allow 1 per 1s;
            }

            GET list() -> Bytes via Binary {
                paginate via CursorPagination<String> {
                    init CursorInit { cursor: pages.cursor };
                    cursor -> pages.cursor;
                }
            }
        }
    }
}

fn main() {}

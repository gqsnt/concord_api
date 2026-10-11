// PAGE-01: an endpoint-local policy keyed by a scope value, with the scope
// value named as cursor destination, is rejected at the destination.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

use concord::pagination::{CursorInit, CursorPagination};

api! {
    client Api {
        base "https://example.test";

        scope pages(query cursor: String) {
            GET list() -> Bytes via Binary {
                rate_limit {
                    id "concord.rate_limit_pagination_scope_dimension.1";
                    key ["cursor" = pages.cursor];
                    allow 1 per 1s;
                }
                paginate via CursorPagination<String> {
                    init CursorInit { cursor: pages.cursor };
                    cursor -> cursor;
                }
            }
        }
    }
}

fn main() {}

// PAGE-01: root values are never cursor destinations (a controller cannot
// mutate root or scope snapshots).
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

use concord::pagination::{CursorInit, CursorPagination};

api! {
    client Api {
        base "https://example.test";
        values { query cursor: String; }
        rate_limit {
            id "concord.rate_limit_pagination_vars_dimension.1";
            key ["cursor" = cursor];
            allow 1 per 1s;
        }

        GET list() -> Bytes via Binary {
            paginate via CursorPagination<String> {
                init CursorInit { cursor: client.cursor };
                cursor -> client.cursor;
            }
        }
    }
}

fn main() {}

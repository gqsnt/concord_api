use concord::api;

api! {
    client Api {
        base "https://example.test";
    }

    scope pages(cursor: String) {
        GET list() -> Bytes {
            rate_limit { by [scope.cursor]; 1 / 1s; }
            paginate crate::CursorPagination {
                cursor: bind scope.cursor,
            }
        }
    }
}

fn main() {}

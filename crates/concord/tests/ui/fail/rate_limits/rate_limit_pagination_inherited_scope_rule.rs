use concord::api;

api! {
    client Api {
        base "https://example.test";
    }

    scope pages(cursor: String) {
        rate_limit { by [scope.cursor]; 1 / 1s; }

        GET list() -> Bytes {
            paginate crate::CursorPagination {
                cursor: bind scope.cursor,
            }
        }
    }
}

fn main() {}

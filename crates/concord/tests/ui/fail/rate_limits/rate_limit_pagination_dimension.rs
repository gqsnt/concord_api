use concord::api;

api! {
    client Api {
        base "https://example.test";
    }

    GET list(cursor: String) -> Bytes {
        rate_limit { by [ep.cursor]; 1 / 1s; }
        paginate crate::CursorPagination {
            cursor: bind ep.cursor,
        }
    }
}

fn main() {}

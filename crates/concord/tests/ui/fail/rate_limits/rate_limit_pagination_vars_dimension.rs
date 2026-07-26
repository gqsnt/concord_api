use concord::api;

api! {
    client Api {
        base "https://example.test";
        vars { cursor: String; }
        rate_limit { by [vars.cursor]; 1 / 1s; }
    }

    GET list() -> Bytes {
        paginate crate::CursorPagination {
            cursor: bind vars.cursor,
        }
    }
}

fn main() {}

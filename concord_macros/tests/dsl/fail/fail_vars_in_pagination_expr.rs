use concord_core::prelude::*;
use concord_macros::api;

api! {
    client VarsPaginationExprApi {
        base "https://example.com"
        var cursor: String
    }

    GET List(cursor?: String)
    path ["items"]
    query {
        cursor
    }
    paginate CursorPagination {
        cursor = format!("{}", vars.cursor)
    }
    -> Json<Vec<String>>
}

fn main() {}

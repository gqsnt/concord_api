use concord_core::prelude::*;
use concord_macros::api;

api! {
    client PaginationUnknownFieldCursorApi {
        base "https://example.com"
    }

    GET List(cursor?: String, count: u64 = 2)
        path ["items"]
        query {
            cursor
            count
        }
        paginate CursorPagination<String> {
            offset = count,
            per_page = count
        }
        -> Json<Vec<String>>
}

fn main() {}

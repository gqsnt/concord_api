use concord_core::prelude::*;
use concord_macros::api;

api! {
    client SecretPaginationApi {
        base "https://example.com"
        secret cursor: String
    }

    GET List(cursor?: String)
    path ["items"]
    query {
        cursor
    }
    paginate CursorPagination {
        cursor = secret.cursor
    }
    -> Json<Vec<String>>
}

fn main() {}

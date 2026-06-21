use concord_core::prelude::*;
use concord_macros::api;

api! {
    client AuthPaginationApi {
        base "https://example.com"
    }

    GET List(count: u32)
    path ["items"]
    paginate OffsetLimitPagination {
        offset = auth.cursor.len(),
        limit = count
    }
    -> Json<Vec<String>>
}

fn main() {}

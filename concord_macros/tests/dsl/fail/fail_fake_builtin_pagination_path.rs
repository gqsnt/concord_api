use concord_core::prelude::*;
use concord_macros::api;

mod fake {
    pub struct OffsetLimitPagination;
}

api! {
    client FakeBuiltinPaginationApi {
        base "https://example.com"
    }

    GET List(start: u64 = 0, count: u64 = 20)
    path ["items"]
    query {
        start,
        count
    }
    paginate fake::OffsetLimitPagination {
        offset = start,
        limit = count
    }
    -> Json<Vec<String>>
}

fn main() {}

use concord_core::prelude::*;
use concord_macros::api;

#[derive(Clone)]
struct CreateBody {
    name: String,
}

api! {
    client BodyPaginateApi {
        base "https://example.com"
    }

    POST Create(limit: u64 = 20, body: Json<CreateBody>)
    path ["items"]
    query {
        limit
    }
    paginate OffsetLimitPagination {
        offset = 0,
        limit = limit
    }
    -> Json<Vec<String>>
}

fn main() {}

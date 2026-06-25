use concord_macros::api;

api! {
    client PaginationUnknownFieldOffsetLimitApi {
        base "https://example.com"
    }

    GET List(start: u64 = 0, count: u64 = 2)
        path ["items"]
        query {
            start
            count
        }
        paginate OffsetLimitPagination {
            cursor = start,
            limit = count
        }
        -> Json<Vec<String>>
}

fn main() {}

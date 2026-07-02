use concord_macros::api;

api! {
    client CursorPaginationRequiresStringApi {
        base "https://example.com"
    }

    GET List(cursor?: String, count: u64 = 2)
        path ["items"]
        paginate CursorPagination {
            cursor = cursor,
            per_page = count
        }
        -> Json<Vec<String>>
}

fn main() {}

use concord_macros::api;

api! {
    client PaginationUnknownFieldPagedApi {
        base "https://example.com"
    }

    GET List(page: u64 = 1, count: u64 = 2)
        path ["items"]
        query {
            page
            count
        }
        paginate PagedPagination {
            cursor = page,
            per_page = count
        }
        -> Json<Vec<String>>
}

fn main() {}

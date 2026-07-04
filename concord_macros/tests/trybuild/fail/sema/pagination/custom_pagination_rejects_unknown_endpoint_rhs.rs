use concord_macros::api;

#[derive(Default)]
struct HeaderPagePagination {
    page: u64,
    count: u64,
}

api! {
    client UnknownEndpointPaginationApi {
        base "https://example.com"
    }

    GET List(page: u64 = 1, count: u64 = 2)
        paginate HeaderPagePagination {
            page = does_not_exist,
            count = count
        }
        -> Json<Vec<String>>
}

fn main() {}

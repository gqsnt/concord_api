use concord_macros::api;

#[derive(Default)]
struct HeaderCursorPagination;

api! {
    client MissingDefaultPaginationApi { base "https://example.com" }

    GET List
        as list
        path ["items"]
        paginate HeaderCursorPagination
        -> Json<Vec<String>>
}

fn main() {}

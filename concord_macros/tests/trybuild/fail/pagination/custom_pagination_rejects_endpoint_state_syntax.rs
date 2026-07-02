use concord_macros::api;

#[derive(Default)]
struct HeaderCursorPagination;

#[derive(Default)]
struct HeaderCursorPaginationBindings;

api! {
    client MissingDefaultPaginationApi { base "https://example.com" }

    GET List
        as list
        path ["items"]
        paginate endpoint_state HeaderCursorPagination bindings HeaderCursorPaginationBindings {
            page = page
        }
        -> Json<Vec<String>>
}

fn main() {}

use concord_macros::api;

struct Foo;

api! {
    client CursorPaginationRejectsNonStringApi {
        base "https://example.com"
    }

    GET List(cursor?: String, count: u64 = 2)
        path ["items"]
        paginate CursorPagination<Foo> {
            cursor = cursor,
            per_page = count
        }
        -> Json<Vec<String>>
}

fn main() {}

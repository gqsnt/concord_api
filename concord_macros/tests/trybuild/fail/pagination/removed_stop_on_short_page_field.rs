use concord_macros::api;

api! {
    client RemovedStopOnShortPageFieldApi {
        base "https://example.com"
    }

    GET List(page: u64 = 1, count: u64 = 2)
        path ["items"]
        query {
            page
            count
        }
        paginate CursorPagination<String> {
            stop_on_short_page = true
        }
        -> Json<Vec<String>>
}

fn main() {}

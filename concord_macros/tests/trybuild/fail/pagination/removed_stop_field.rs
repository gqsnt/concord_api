use concord_macros::api;

api! {
    client RemovedStopFieldApi {
        base "https://example.com"
    }

    GET List(page: u64 = 1, count: u64 = 2)
        path ["items"]
        query {
            page
            count
        }
        paginate CursorPagination {
            stop = true
        }
        -> Json<Vec<String>>
}

fn main() {}

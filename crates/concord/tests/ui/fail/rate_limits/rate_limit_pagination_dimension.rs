use concord::api;

api! {
    client Api {
        timeout off;
        base "https://example.test";


    GET list(cursor: String) -> Bytes using Bytes {
        rate_limit { key [endpoint.cursor]; allow 1 per 1s; }
        paginate with crate::CursorPagination {
            cursor -> endpoint.cursor;
        }
    }
}}

fn main() {}

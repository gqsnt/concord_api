use concord::api;

api! {
    client Api {
        timeout off;
        base "https://example.test";


    scope pages(cursor: String) {
        GET list() -> Bytes using Bytes {
            rate_limit { key [pages.cursor]; allow 1 per 1s; }
            paginate with crate::CursorPagination {
                cursor -> pages.cursor;
            }
        }
    }
}}

fn main() {}

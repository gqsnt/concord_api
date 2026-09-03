use concord::api;

api! {
    client Api {
        timeout off;
        base "https://example.test";


    scope pages(cursor: String) {
        rate_limit { key [pages.cursor]; allow 1 per 1s; }

        GET list() -> Bytes using Bytes {
            paginate with crate::CursorPagination {
                cursor -> pages.cursor;
            }
        }
    }
}}

fn main() {}

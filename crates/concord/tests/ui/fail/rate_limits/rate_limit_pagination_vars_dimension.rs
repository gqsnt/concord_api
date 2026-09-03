use concord::api;

api! {
    client Api {
        timeout off;
        base "https://example.test";
        values { cursor: String; }
        rate_limit { key [client.cursor]; allow 1 per 1s; }


    GET list() -> Bytes using Bytes {
        paginate with crate::CursorPagination {
            cursor -> client.cursor;
        }
    }
}}

fn main() {}

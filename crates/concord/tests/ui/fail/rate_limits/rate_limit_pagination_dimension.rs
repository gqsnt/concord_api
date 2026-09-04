use concord::api;

api! {
    client Api {
        timeout 30s;
        base "https://example.test";


    GET list(cursor: String) -> Bytes via Binary {
        rate_limit { id "concord.rate_limit_pagination_dimension.1"; cost 1;  key [endpoint.cursor]; allow 1 per 1s; }
        paginate via crate::CursorPagination {
            cursor -> endpoint.cursor;
        }
    }
}}

fn main() {}

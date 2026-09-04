use concord::api;

api! {
    client Api {
        timeout 30s;
        base "https://example.test";
        values { cursor: String; }
        rate_limit { id "concord.rate_limit_pagination_vars_dimension.1"; cost 1;  key [client.cursor]; allow 1 per 1s; }


    GET list() -> Bytes via Binary {
        paginate via crate::CursorPagination {
            cursor -> client.cursor;
        }
    }
}}

fn main() {}

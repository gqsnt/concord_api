use concord::api;

api! {
    client Api {
        timeout 30s;
        base "https://example.test";


    scope pages(cursor: String) {
        GET list() -> Bytes via Binary {
            rate_limit { id "concord.rate_limit_pagination_scope_dimension.1"; cost 1;  key [pages.cursor]; allow 1 per 1s; }
            paginate via crate::CursorPagination {
                cursor -> pages.cursor;
            }
        }
    }
}}

fn main() {}

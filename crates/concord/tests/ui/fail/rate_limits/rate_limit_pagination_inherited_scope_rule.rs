use concord::api;

api! {
    client Api {
        timeout 30s;
        base "https://example.test";


    scope pages(cursor: String) {
        rate_limit { id "concord.rate_limit_pagination_inherited_scope_rule.1"; cost 1;  key [pages.cursor]; allow 1 per 1s; }

        GET list() -> Bytes via Binary {
            paginate via crate::CursorPagination {
                cursor -> pages.cursor;
            }
        }
    }
}}

fn main() {}

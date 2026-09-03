use concord::api;

api! {
    client Api {
        timeout off;
        base "https://example.test";


    scope pages(region: String) {
        subdomain replace [pages.region, "api"];
        rate_limit { key [host]; allow 1 per 1s; }

        GET list() -> Bytes using Bytes {
            paginate with crate::CursorPagination {
                region -> pages.region;
            }
        }
    }
}}

fn main() {}

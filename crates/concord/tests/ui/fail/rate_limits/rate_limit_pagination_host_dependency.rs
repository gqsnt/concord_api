use concord::api;

api! {
    client Api {
        timeout 30s;
        base "https://example.test";


    scope pages(region: String) {
        subdomain replace [pages.region, "api"];
        rate_limit { id "concord.rate_limit_pagination_host_dependency.1"; cost 1;  key [origin]; allow 1 per 1s; }

        GET list() -> Bytes via Binary {
            paginate via crate::CursorPagination {
                region -> pages.region;
            }
        }
    }
}}

fn main() {}

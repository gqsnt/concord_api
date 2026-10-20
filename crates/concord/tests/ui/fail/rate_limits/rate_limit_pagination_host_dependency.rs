// PAGE-01: cursor destinations are endpoint-owned scalar slots. A scope
// value that selects the DNS label (and so the rate `origin`) cannot be a
// cursor destination; endpoints cannot own subdomains.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

use concord::pagination::{CursorInit, CursorPagination};

api! {
    client Api {
        base "https://example.test";

        scope pages(region: String) {
            subdomain replace [region, "api"];
            rate_limit { id "concord.rate_limit_pagination_host_dependency.1"; key [origin]; allow 1 per 1s; }

            GET list(query after?: String) -> Bytes via Binary {
                paginate via CursorPagination<String> {
                    init CursorInit { cursor: after };
                    cursor -> region;
                }
            }
        }
    }
}

fn main() {}

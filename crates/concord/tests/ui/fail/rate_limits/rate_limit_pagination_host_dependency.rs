use concord::api;

api! {
    client Api {
        base "https://example.test";
    }

    scope pages(region: String) {
        host [scope.region, "api"];
        rate_limit { by [host]; 1 / 1s; }

        GET list() -> Bytes {
            paginate crate::CursorPagination {
                region: bind scope.region,
            }
        }
    }
}

fn main() {}

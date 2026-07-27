use concord::api;

api! {
    client Api {
        base "https://example.test";
        rate_limits {
            rule account_reads {
                by [account];
                1 / 1s;
            }
        }
    }

    GET list(account: String) -> Bytes {
        rate_limit use account_reads {
            account = ep.account;
        }
        paginate crate::CursorPagination {
            account: bind ep.account,
        }
    }
}

fn main() {}

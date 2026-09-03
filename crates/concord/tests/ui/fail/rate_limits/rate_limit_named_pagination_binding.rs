use concord::api;

api! {
    client Api {
        timeout off;
        base "https://example.test";
        rate_limits {
            account_reads(account) {
                key [account];
                allow 1 per 1s;
            }
        }


    GET list(account: String) -> Bytes using Bytes {
        rate_limit account_reads(account = endpoint.account);
        paginate with crate::CursorPagination {
            account -> endpoint.account;
        }
    }
}}

fn main() {}

use concord::api;

api! {
    client Api {
        timeout 30s;
        base "https://example.test";
        rate_limits {
            policy account_reads(account) { id "concord.rate_limit_named_pagination_binding.1"; cost 1;
                key [account];
                allow 1 per 1s;
            }
        }


    GET list(account: String) -> Bytes via Binary {
        rate_limit account_reads(account = endpoint.account);
        paginate via crate::CursorPagination {
            account -> endpoint.account;
        }
    }
}}

fn main() {}

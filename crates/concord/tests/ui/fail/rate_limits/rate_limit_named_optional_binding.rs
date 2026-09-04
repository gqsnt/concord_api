use concord::api;

api! {
    client Api {
        timeout 30s;
        base "https://example.test";
        values {
            account?: String;
        }
        rate_limits {
            policy account_reads(account) { id "concord.rate_limit_named_optional_binding.1"; cost 1;
                key [account];
                allow 1 per 1s;
            }
        }
        rate_limit account_reads(account = client.account);
    }
}

fn main() {}

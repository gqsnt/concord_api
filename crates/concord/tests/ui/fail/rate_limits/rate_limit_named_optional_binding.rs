use concord::api;

api! {
    client Api {
        timeout off;
        base "https://example.test";
        values {
            account?: String;
        }
        rate_limits {
            account_reads(account) {
                key [account];
                allow 1 per 1s;
            }
        }
        rate_limit account_reads(account = client.account);
    }
}

fn main() {}

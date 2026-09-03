use concord::api;

api! {
    client Api {
        timeout off;
        base "https://example.test";
        values {
            tenant: String;
        }
        rate_limits {
            account_reads(account) {
                key [host, account];
                allow 1 per 1s;
            }
        }
        rate_limit account_reads(host = client.tenant, account = client.tenant);
    }
}

fn main() {}

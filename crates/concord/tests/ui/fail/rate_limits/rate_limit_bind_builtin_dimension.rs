use concord::api;

api! {
    client Api {
        timeout 30s;
        base "https://example.test";
        values {
            tenant: String;
        }
        rate_limits {
            policy account_reads(account) { id "concord.rate_limit_bind_builtin_dimension.1"; cost 1;
                key [origin, account];
                allow 1 per 1s;
            }
        }
        rate_limit account_reads(host = client.tenant, account = client.tenant);
    }
}

fn main() {}

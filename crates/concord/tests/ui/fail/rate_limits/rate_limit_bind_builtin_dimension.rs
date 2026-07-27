use concord::api;

api! {
    client Api {
        base "https://example.test";
        vars {
            tenant: String;
        }
        rate_limits {
            rule account_reads {
                by [host, account];
                1 / 1s;
            }
        }
        rate_limit use account_reads {
            host = vars.tenant;
            account = vars.tenant;
        }
    }
}

fn main() {}

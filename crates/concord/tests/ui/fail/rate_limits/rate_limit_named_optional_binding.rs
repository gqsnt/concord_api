use concord::api;

api! {
    client Api {
        base "https://example.test";
        vars {
            account: Option<String>;
        }
        rate_limits {
            rule account_reads {
                by [account];
                1 / 1s;
            }
        }
        rate_limit use account_reads {
            account = vars.account;
        }
    }
}

fn main() {}

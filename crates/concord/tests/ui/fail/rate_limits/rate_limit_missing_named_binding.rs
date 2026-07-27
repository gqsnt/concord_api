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
        rate_limit use account_reads;
    }
}

fn main() {}

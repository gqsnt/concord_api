use concord::api;

api! {
    client Api {
        base "https://example.test";
    }

    scope accounts() {
        rate_limits {
            rule account_reads { 1 / 1s; }
        }
        GET read() -> Bytes;
    }
}

fn main() {}

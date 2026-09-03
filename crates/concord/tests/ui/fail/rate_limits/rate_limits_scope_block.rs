use concord::api;

api! {
    client Api {
        timeout off;
        base "https://example.test";


    scope accounts() {
        rate_limits {
            account_reads() { allow 1 per 1s; }
        }
        GET read() -> Bytes using Bytes;
    }
}}

fn main() {}

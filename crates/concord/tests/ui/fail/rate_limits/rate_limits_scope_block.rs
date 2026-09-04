use concord::api;

api! {
    client Api {
        timeout 30s;
        base "https://example.test";


    scope accounts() {
        rate_limits {
            policy account_reads() { id "concord.rate_limits_scope_block.1"; key [origin]; cost 1;  allow 1 per 1s; }
        }
        GET read() -> Bytes via Binary;
    }
}}

fn main() {}

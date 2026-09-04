use concord::api;

api! {
    client Api {
        timeout 30s;
        base "https://example.test";
        rate_limits {
            policy unused_rule() { id "concord.rate_limit_unused_definition.1"; key [origin]; cost 1;
                allow 1 per 1s;
            }
        }


    GET read() -> Bytes via Binary;
}}

fn main() {}

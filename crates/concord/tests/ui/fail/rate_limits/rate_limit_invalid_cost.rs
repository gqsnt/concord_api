use concord::api;

api! {
    client Api {
        timeout 30s;
        base "https://example.test";
        rate_limit { id "concord.rate_limit_invalid_cost.1"; key [origin];  cost 2; allow 1 per 1s; }


    GET read() -> Bytes via Binary;
}}

fn main() {}

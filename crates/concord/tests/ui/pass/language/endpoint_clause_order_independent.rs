use concord::api;

api! {
    client Api {
        timeout 30s;
        base "https://example.test";


    GET read() -> Bytes via Binary {
        rate_limit { id "concord.endpoint_clause_order_independent.1"; key [origin]; cost 1;  allow 1 per 1s; }
        timeout 1s;
    }
}}

fn main() {}

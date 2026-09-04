use concord::api;

api! {
    client Api {
        timeout 30s;
        base "https://example.test";


    scope regional(region: String) {
        rate_limit { id "concord.rate_limit_scope_endpoint_dimension.1"; cost 1;  key [endpoint.account]; allow 1 per 1s; }
        GET read(account: String) -> Bytes via Binary;
    }
}}

fn main() {}

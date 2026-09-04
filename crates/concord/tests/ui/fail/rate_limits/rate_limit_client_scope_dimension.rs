use concord::api;

api! {
    client Api {
        timeout 30s;
        base "https://example.test";
        rate_limit { id "concord.rate_limit_client_scope_dimension.1"; cost 1;  key [scope.region]; allow 1 per 1s; }


    scope regional(region: String) {
        GET read() -> Bytes via Binary;
    }
}}

fn main() {}

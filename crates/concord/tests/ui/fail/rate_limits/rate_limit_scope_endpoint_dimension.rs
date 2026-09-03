use concord::api;

api! {
    client Api {
        timeout off;
        base "https://example.test";


    scope regional(region: String) {
        rate_limit { key [endpoint.account]; allow 1 per 1s; }
        GET read(account: String) -> Bytes using Bytes;
    }
}}

fn main() {}

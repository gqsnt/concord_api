use concord::api;

api! {
    client Api {
        timeout off;
        base "https://example.test";
        rate_limit { key [scope.region]; allow 1 per 1s; }


    scope regional(region: String) {
        GET read() -> Bytes using Bytes;
    }
}}

fn main() {}

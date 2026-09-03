use concord::api;

api! {
    client Api {
        timeout off;
        base "https://example.test";


    GET read() -> Bytes using Bytes {
        rate_limit { allow 1 per 1s; }
        timeout 1s;
    }
}}

fn main() {}

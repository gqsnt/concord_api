use concord::api;

api! {
    client Api {
        timeout off;
        base "https://example.test";
        rate_limit { cost 2; allow 1 per 1s; }


    GET read() -> Bytes using Bytes;
}}

fn main() {}

use concord::api;

api! {
    client Api {
        timeout off;
        base "https://example.test";
        rate_limit off;


    GET read() -> Bytes using Bytes;
}}

fn main() {}

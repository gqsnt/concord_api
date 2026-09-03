use concord::api;

api! {
    client Api {
        timeout off;
        base "https://example.test";
        rate_limit missing();


    GET read() -> Bytes using Bytes;
}}

fn main() {}

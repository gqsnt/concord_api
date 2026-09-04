use concord::api;

api! {
    client Api {
        timeout 30s;
        base "https://example.test";
        rate_limit missing();


    GET read() -> Bytes via Binary;
}}

fn main() {}

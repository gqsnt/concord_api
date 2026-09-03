use concord::api;

api! {
    client Api {
        timeout off;
        base "https://example.test";
        values {
            tenant?: String;
        }
        rate_limit { key [client.tenant]; allow 1 per 1s; }


    GET read() -> Bytes using Bytes;
}}

fn main() {}

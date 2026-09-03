use concord::api;

api! {
    client Api {
        timeout off;
        base "https://example.test";
        rate_limit {
            id "service.shared";
            allow 10 per 1s;
        }


    GET read() -> Bytes using Bytes {
        rate_limit {
            id "service.shared";
            cost 2;
            allow 10 per 1s;
        }
    }
}}

fn main() {}

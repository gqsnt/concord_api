use concord::api;

api! {
    client Api {
        timeout off;
        base "https://example.test";


    scope first() {
        rate_limit {
            id "service.shared";
            key [host, endpoint];
            allow 10 per 1s;
        }
        GET read() -> Bytes using Bytes;
    }

    scope second() {
        rate_limit {
            id "service.shared";
            key [endpoint, host];
            allow 10 per 1s;
        }
        GET write() -> Bytes using Bytes;
    }
}}

fn main() {}

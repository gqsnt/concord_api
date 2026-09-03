use concord::api;

api! {
    client Api {
        timeout off;
        base "https://example.test";


    scope first() {
        rate_limit {
            id "service.shared";
            allow 10 per 1s;
            allow 100 per 1m;
        }
        GET read() -> Bytes using Bytes;
    }

    scope second() {
        rate_limit {
            id "service.shared";
            allow 100 per 1m;
            allow 10 per 1s;
        }
        GET write() -> Bytes using Bytes;
    }
}}

fn main() {}

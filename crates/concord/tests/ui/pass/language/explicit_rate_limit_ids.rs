use concord::api;

api! {
    client Api {
        timeout off;
        base "https://example.test";


    scope first() {
        rate_limit {
            id "service.shared.quota";
            key [host, endpoint];
            cost 1;
            allow 10 per 1s;
            allow 100 per 1m;
        }
        GET read() -> Bytes using Bytes;
    }

    scope renamed() {
        rate_limit {
            cost 2;
            key [host, endpoint];
            id "service.shared.quota";
            allow 10 per 1s;
            allow 100 per 1m;
        }
        GET write() -> Bytes using Bytes;
    }
}}

fn main() {}

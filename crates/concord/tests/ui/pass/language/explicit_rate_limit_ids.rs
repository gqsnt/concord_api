use concord::api;

api! {
    client Api {
        timeout 30s;
        base "https://example.test";


    scope first() {
        rate_limit {
            id "service.shared.quota";
            key [origin, endpoint];
            cost 1;
            allow 10 per 1s;
            allow 100 per 1m;
        }
        GET read() -> Bytes via Binary;
    }

    scope renamed() {
        rate_limit {
            cost 2;
            key [origin, endpoint];
            id "service.shared.quota";
            allow 10 per 1s;
            allow 100 per 1m;
        }
        GET write() -> Bytes via Binary;
    }
}}

fn main() {}

use concord::api;

api! {
    client Api {
        timeout 30s;
        base "https://example.test";


    scope first() {
        rate_limit { key [origin]; cost 1;
            id "service.shared";
            allow 10 per 1s;
            allow 100 per 1m;
        }
        GET read() -> Bytes via Binary;
    }

    scope second() {
        rate_limit { key [origin]; cost 1;
            id "service.shared";
            allow 100 per 1m;
            allow 10 per 1s;
        }
        GET write() -> Bytes via Binary;
    }
}}

fn main() {}

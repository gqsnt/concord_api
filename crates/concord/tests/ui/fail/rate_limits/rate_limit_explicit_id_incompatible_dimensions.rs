use concord::api;

api! {
    client Api {
        timeout 30s;
        base "https://example.test";


    scope first() {
        rate_limit { cost 1;
            id "service.shared";
            key [origin, endpoint];
            allow 10 per 1s;
        }
        GET read() -> Bytes via Binary;
    }

    scope second() {
        rate_limit { cost 1;
            id "service.shared";
            key [endpoint, origin];
            allow 10 per 1s;
        }
        GET write() -> Bytes via Binary;
    }
}}

fn main() {}

use concord::api;

api! {
    client Api {
        timeout 30s;
        base "https://example.test";
        rate_limit { key [origin]; cost 1;
            id "service.shared";
            allow 10 per 1s;
        }


    GET read() -> Bytes via Binary {
        rate_limit { key [origin];
            id "service.shared";
            cost 2;
            allow 10 per 1s;
        }
    }
}}

fn main() {}

use concord::api;

api! {
    client Api {
        timeout 30s;
        base "https://example.test";
        rate_limit { id "concord.rate_limit_duplicate_by.1"; cost 1;
            key [origin];
            key [endpoint];
            allow 1 per 1s;
        }
    }
}

fn main() {}

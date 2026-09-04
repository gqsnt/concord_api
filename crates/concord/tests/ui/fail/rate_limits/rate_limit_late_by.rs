use concord::api;

api! {
    client Api {
        timeout 30s;
        base "https://example.test";
        rate_limit { id "concord.rate_limit_late_by.1"; cost 1;
            allow 1 per 1s;
            key [origin];
        }
    }
}

fn main() {}

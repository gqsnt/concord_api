use concord::api;

api! {
    client Api {
        timeout 30s;
        base "https://example.test";
        rate_limit { id "concord.rate_limit_second_anonymous_rule.1"; key [origin]; cost 1;  allow 1 per 1s; }
        rate_limit shared();
        rate_limit { id "concord.rate_limit_second_anonymous_rule.2"; key [origin]; cost 1;  allow 2 per 1s; }
        rate_limits {
            policy shared() { id "concord.rate_limit_second_anonymous_rule.3"; key [origin]; cost 1;  allow 3 per 1s; }
        }
    }
}

fn main() {}

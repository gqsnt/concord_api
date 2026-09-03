use concord::api;

api! {
    client Api {
        timeout off;
        base "https://example.test";
        rate_limit { allow 1 per 1s; }
        rate_limit shared();
        rate_limit { allow 2 per 1s; }
        rate_limits {
            shared() { allow 3 per 1s; }
        }
    }
}

fn main() {}

use concord::api;

api! {
    client Api {
        base "https://example.test";
        rate_limit { 1 / 1s; }
        rate_limit use shared;
        rate_limit { 2 / 1s; }
        rate_limits {
            rule shared { 3 / 1s; }
        }
    }
}

fn main() {}

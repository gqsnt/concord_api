use concord::prelude::*;

api! {
    client RateWindowLimitApi {
        base "https://example.test";
        rate_limit {
            1 / 1s;
            1 / 2s;
            1 / 3s;
            1 / 4s;
            1 / 5s;
            1 / 6s;
            1 / 7s;
            1 / 8s;
            1 / 9s;
            1 / 10s;
            1 / 11s;
            1 / 12s;
            1 / 13s;
            1 / 14s;
            1 / 15s;
            1 / 16s;
            1 / 17s;
        }
    }
}

fn main() {}

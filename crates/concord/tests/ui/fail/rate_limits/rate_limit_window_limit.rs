// §20.1: at most 16 windows per policy.
#![allow(unused_imports)]

use concord::api;

api! {
    client RateWindowLimitApi {
        base "https://example.test";
        rate_limit {
            id "concord.rate_limit_window_limit.1";
            key [origin];
            allow 1 per 1s;
            allow 1 per 2s;
            allow 1 per 3s;
            allow 1 per 4s;
            allow 1 per 5s;
            allow 1 per 6s;
            allow 1 per 7s;
            allow 1 per 8s;
            allow 1 per 9s;
            allow 1 per 10s;
            allow 1 per 11s;
            allow 1 per 12s;
            allow 1 per 13s;
            allow 1 per 14s;
            allow 1 per 15s;
            allow 1 per 16s;
            allow 1 per 17s;
        }
    }
}

fn main() {}

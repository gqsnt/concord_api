// §12.1: every rate policy begins with exactly one explicit `id`.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client Duplicate {
        base "https://example.test";
        rate_limit {
            id "service.first";
            id "service.second";
            key [origin];
            allow 1 per 1s;
        }
    }
}

api! {
    client Late {
        base "https://example.test";
        rate_limit {
            key [origin];
            allow 1 per 1s;
            id "service.late";
        }
    }
}

api! {
    client Missing {
        base "https://example.test";
        rate_limits {
            policy anonymous() { key [origin]; allow 1 per 1s; }
        }
    }
}

fn main() {}

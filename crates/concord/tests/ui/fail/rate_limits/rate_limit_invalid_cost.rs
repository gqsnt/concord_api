// §12.1: a cost greater than any window capacity could never be admitted.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client Api {
        base "https://example.test";
        rate_limit { id "concord.rate_limit_invalid_cost.1"; key [origin]; cost 2; allow 1 per 1s; }

        GET read() -> Bytes via Binary;
    }
}

api! {
    client AttachmentCostApi {
        base "https://example.test";
        rate_limits {
            policy reads() { id "concord.rate_limit_invalid_cost.2"; key [origin]; allow 5 per 1s; }
        }

        GET read() -> Bytes via Binary {
            rate_limit reads() { cost 6; }
        }
    }
}

api! {
    client ZeroCostApi {
        base "https://example.test";
        rate_limit { id "concord.rate_limit_invalid_cost.3"; key [origin]; cost 0; allow 1 per 1s; }
    }
}

fn main() {}

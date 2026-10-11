// v2 allowed at most one anonymous `rate_limit { ... }` per owner; v3 rate
// attachments are additive and may repeat (RATE-01; multiple inline
// attachments compile in pass/language/reusable_rate_rules.rs). What remains
// an error is two attachments of one ID with different schemas on one owner.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client Api {
        base "https://example.test";
        rate_limit { id "concord.rate_limit_second_anonymous_rule.1"; key [origin]; allow 1 per 1s; }
        rate_limit { id "concord.rate_limit_second_anonymous_rule.1"; key [origin]; allow 2 per 1s; }

        GET read() -> Bytes via Binary;
    }
}

fn main() {}

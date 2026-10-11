// §12.3: a policy ID is a nonempty string of at most 256 UTF-8 bytes. (The
// v2 endpoint-style segment grammar is not applied to policy IDs in v3; see
// pass/language/explicit_rate_limit_ids.rs.)
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client EmptyIdApi {
        base "https://example.test";
        rate_limit { id ""; key [origin]; allow 1 per 1s; }
    }
}

api! {
    client OversizedIdApi {
        base "https://example.test";
        rate_limit { id "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"; key [origin]; allow 1 per 1s; }
    }
}

api! {
    client ShortFormEmptyIdApi {
        base "https://example.test";
        rate_limits { windows standard { allow 1 per 1s; } }
        rate_limit "" key [origin] using standard;
    }
}

fn main() {}

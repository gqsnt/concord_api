// §12.2: an unused formal inside a policy is an error because it falsely
// advertises a key relation. (Unused policy/window definitions themselves are
// allowed in v3 and shown as unused by inspection — see
// pass/language/unused_rate_definitions.rs; this replaces the v2
// "never attached" rule.)
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client Api {
        base "https://example.test";
        rate_limits {
            policy per_tenant(tenant, region) {
                id "concord.rate_limit_unused_formal.1";
                key [origin, "tenant" = tenant];
                allow 1 per 1s;
            }
        }

        GET read() -> Bytes via Binary;
    }
}

fn main() {}

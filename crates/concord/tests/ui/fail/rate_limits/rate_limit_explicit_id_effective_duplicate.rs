// RATE-01 / V3-R04: a repeated identity on one effective endpoint is
// deduplicated only when structurally identical; a conflicting repetition
// (here: a different cost) is an error naming both attachments.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client Api {
        base "https://example.test";
        rate_limit {
            id "service.shared";
            key [origin];
            allow 10 per 1s;
        }

        GET read() -> Bytes via Binary {
            rate_limit {
                id "service.shared";
                key [origin];
                cost 2;
                allow 10 per 1s;
            }
        }
    }
}

api! {
    client NamedApi {
        base "https://example.test";
        values { query tenant: String; query other: String; }
        rate_limits {
            policy per_tenant(tenant) {
                id "service.tenant";
                key [origin, "tenant" = tenant];
                allow 10 per 1s;
            }
        }
        rate_limit per_tenant(tenant = tenant);

        GET read() -> Bytes via Binary {
            rate_limit per_tenant(tenant = client.other);
        }
    }
}

fn main() {}

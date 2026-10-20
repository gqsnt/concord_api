// §12.3: dimension labels are unique within one key.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client InlineApi {
        base "https://example.test";
        values { query tenant: String; query other: String; }
        rate_limit {
            id "concord.rate_limit_duplicate_dimension.1";
            key ["tenant" = tenant, "tenant" = other];
            allow 1 per 1s;
        }

        GET read() -> Bytes via Binary;
    }
}

api! {
    client PolicyApi {
        base "https://example.test";
        values { query tenant: String; }
        rate_limits {
            policy per_tenant(tenant, other) {
                id "concord.rate_limit_duplicate_dimension.2";
                key ["tenant" = tenant, "tenant" = other];
                allow 1 per 1s;
            }
        }
        rate_limit per_tenant(tenant = tenant, other = tenant);

        GET read() -> Bytes via Binary;
    }
}

fn main() {}

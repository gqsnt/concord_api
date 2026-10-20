// LANG-03 / §12.3: a client-level policy cannot partition by an endpoint
// value; a bare name binds only the current owner (the client).
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client Api {
        base "https://example.test";
        rate_limit {
            id "concord.rate_limit_client_endpoint_dimension.1";
            key ["account" = account];
            allow 1 per 1s;
        }

        GET read(query account: String) -> Bytes via Binary;
    }
}

api! {
    client QualifiedApi {
        base "https://example.test";
        rate_limit {
            id "concord.rate_limit_client_endpoint_dimension.2";
            key ["account" = endpoint.account];
            allow 1 per 1s;
        }

        GET read(query account: String) -> Bytes via Binary;
    }
}

fn main() {}

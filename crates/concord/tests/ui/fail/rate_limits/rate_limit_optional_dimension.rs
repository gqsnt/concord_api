// §12.3: optional slots cannot be custom dimensions.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client Api {
        base "https://example.test";
        values { query tenant?: String; }
        rate_limit {
            id "concord.rate_limit_optional_dimension.1";
            key ["tenant" = tenant];
            allow 1 per 1s;
        }

        GET read() -> Bytes via Binary;
    }
}

fn main() {}

// §12.3: a custom dimension value is canonically encoded through
// `RateKeyPart` (or an explicit `RateKeyEncoder`), never `Display`; a type
// with neither is rejected.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

pub struct NoKeyPart;

api! {
    client Api {
        base "https://example.test";

        GET read(resource: crate::NoKeyPart) -> Bytes via Binary {
            rate_limit {
                id "concord.rate_limit_dimension_requires_display.1";
                key ["resource" = resource];
                allow 1 per 1s;
            }
        }
    }
}

fn main() {}

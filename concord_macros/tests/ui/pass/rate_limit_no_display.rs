use concord_core::prelude::Bytes;
use concord_macros::api;

pub struct Displayless;

api! {
    client Api {
        base "https://example.test";
        vars {
            client_value: crate::Displayless;
        }
        rate_limit { 10 / 1s; }
    }

    scope regions(region: crate::Displayless) {
        rate_limit { 5 / 1s; }
        GET read(item: crate::Displayless) -> Bytes {
            rate_limit { 1 / 1s; }
        }
    }
}

fn main() {
    let _ = core::mem::size_of::<Displayless>();
}

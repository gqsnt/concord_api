use concord::prelude::Bytes;
use concord::api;

pub struct Displayless;

api! {
    client Api {
        timeout off;
        base "https://example.test";
        values {
            client_value: crate::Displayless;
        }
        rate_limit { allow 10 per 1s; }


    scope regions(region: crate::Displayless) {
        rate_limit { allow 5 per 1s; }
        GET read(item: crate::Displayless) -> Bytes using Bytes {
            rate_limit { allow 1 per 1s; }
        }
    }
}}

fn main() {
    let _ = core::mem::size_of::<Displayless>();
}

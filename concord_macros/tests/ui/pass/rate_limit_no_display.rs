use concord_core::prelude::Bytes;
use concord_macros::api;

pub struct Displayless;

api! {
    client Api {
        base "https://example.test";
        var client_value: crate::Displayless;
        rate_limit { 10 / 1s; }
        scope Regions(region: crate::Displayless) {
            rate_limit { 5 / 1s; }
            GET Read(item: crate::Displayless) -> Bytes {
                rate_limit { 1 / 1s; }
            }
        }
    }
}

fn main() {
    let _ = core::mem::size_of::<Displayless>();
}

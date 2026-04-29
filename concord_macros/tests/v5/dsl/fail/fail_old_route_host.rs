use concord_core::prelude::*;
use concord_macros::api;

api! {
    client OldRouteHostApi {
        base https "example.com"
        rate_limit app {
            bucket application by [route.host] { 1 / 1s }
        }
    }
}

fn main() {}

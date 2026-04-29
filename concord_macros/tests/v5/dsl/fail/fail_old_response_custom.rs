use concord_core::prelude::*;
use concord_macros::api;

api! {
    client OldResponseCustomApi {
        base https "example.com"
        response custom MyObserver
    }
}

fn main() {}

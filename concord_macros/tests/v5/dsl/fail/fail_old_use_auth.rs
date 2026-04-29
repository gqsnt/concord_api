use concord_core::prelude::*;
use concord_macros::api;

api! {
    client OldUseAuthApi {
        base https "example.com"
        use_auth HeaderAuth("X-Api-Key")
    }
}

fn main() {}

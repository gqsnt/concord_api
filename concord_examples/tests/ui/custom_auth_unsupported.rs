use concord_core::prelude::*;
use concord_macros::api;

struct Provider;

api! {
    client CustomAuthUnsupportedApi {
        base https "example.com"
        credential key = custom<Provider>(Provider)
    }

    GET Me -> Json<()> {
        path ["me"]
        auth header "X-Token" = key
    }
}

fn main() {}

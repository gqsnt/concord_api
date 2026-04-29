use concord_core::prelude::*;
use concord_macros::api;

#[derive(Clone)]
struct CustomUsage;

api! {
    client CustomAuthPlacementApi {
        base https "example.com"
        secret token: String
        credential key = api_key(secret.token)
    }

    GET Broken
        auth custom<CustomUsage>(CustomUsage, key)
        path ["broken"]
        -> Json<String>
}

fn main() {}

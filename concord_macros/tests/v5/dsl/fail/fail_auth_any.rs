use concord_core::prelude::*;
use concord_macros::api;

api! {
    client AuthAnyApi {
        base https "example.com"
        secret a: String
        secret b: String
        credential a_key = api_key(secret.a)
        credential b_key = api_key(secret.b)
    }

    GET Broken
        auth any { bearer a_key bearer b_key }
        path ["broken"]
        -> Json<String>
}

fn main() {}

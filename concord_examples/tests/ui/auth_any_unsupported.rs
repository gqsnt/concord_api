use concord_core::prelude::*;
use concord_macros::api;

api! {
    client AuthAnyUnsupportedApi {
        base https "example.com"
        secret token: String
        credential key = api_key(secret.token)
    }

    GET Me -> Json<()> {
        path ["me"]
        auth any {
            header "X-Token" = key
            query "token" = key
        }
    }
}

fn main() {}

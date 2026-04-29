use concord_core::prelude::*;
use concord_macros::api;

api! {
    client FmtSecretPathApi {
        base https "example.com"
        secret api_key: String
    }

    GET Broken
        path [fmt["key-", secret.api_key]]
        -> Json<String>
}

fn main() {}

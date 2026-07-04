use concord_core::prelude::*;
use concord_macros::api;

api! {
    client SecretRouteAtomApi {
        base "https://example.com"
        secret token: String
    }

    GET Ping
    path [secret.token]
    -> Text<String>
}

fn main() {}

use concord_core::prelude::*;
use concord_macros::api;

api! {
    client RawSecretRouteAtomApi {
        base "https://example.com"
        secret token: String
    }

    GET Ping
    path [r#secret.token]
    -> Text<String>
}

fn main() {}

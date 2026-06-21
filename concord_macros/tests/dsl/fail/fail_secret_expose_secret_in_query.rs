use concord_core::prelude::*;
use concord_macros::api;

api! {
    client SecretExposeSecretQueryApi {
        base "https://example.com"
        secret token: String
    }

    GET Ping
    path ["ping"]
    query {
        "leak" = secret.token.expose_secret().to_string()
    }
    -> Text<String>
}

fn main() {}

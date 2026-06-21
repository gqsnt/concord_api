use concord_core::prelude::*;
use concord_macros::api;

api! {
    client SecretExposeQueryApi {
        base "https://example.com"
        secret token: String
    }

    GET Ping
    path ["ping"]
    query {
        "leak" = secret.token.expose().to_string()
    }
    -> Text<String>
}

fn main() {}

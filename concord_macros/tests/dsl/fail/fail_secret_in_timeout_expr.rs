use concord_core::prelude::*;
use concord_macros::api;

api! {
    client SecretTimeoutApi {
        base "https://example.com"
        secret token: String
    }

    GET Ping
    path ["ping"]
    timeout std::time::Duration::from_secs(secret.token.len() as u64)
    -> Text<String>
}

fn main() {}

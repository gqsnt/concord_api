use concord_core::prelude::*;
use concord_macros::api;

api! {
    client SecretMacroPolicyApi {
        base "https://example.com"
        secret token: String
    }

    GET Ping
    path ["ping"]
    header "X-Leak" = format!("{}", secret.token)
    -> Text<String>
}

fn main() {}

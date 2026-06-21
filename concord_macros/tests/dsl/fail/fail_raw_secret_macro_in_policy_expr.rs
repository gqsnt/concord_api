use concord_core::prelude::*;
use concord_macros::api;

api! {
    client RawSecretMacroPolicyExprApi {
        base "https://example.com"
        secret token: String
    }

    GET Ping
    path ["ping"]
    header "X-Leak" = format!("{}", r#secret.token)
    -> Text<String>
}

fn main() {}

use concord_core::prelude::*;
use concord_macros::api;

api! {
    client SecretExposureSecretMethodPolicyApi {
        base "https://example.com"
    }

    GET Ping(token: SecretString)
    path ["ping"]
    header "X-Leak" = token.expose_secret()
    -> Text<String>
}

fn main() {}

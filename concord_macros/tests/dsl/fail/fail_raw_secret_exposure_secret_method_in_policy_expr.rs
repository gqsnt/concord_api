use concord_core::prelude::*;
use concord_macros::api;

api! {
    client RawSecretExposureSecretMethodPolicyApi {
        base "https://example.com"
    }

    GET Ping(token: SecretString)
    path ["ping"]
    header "X-Leak" = token.r#expose_secret()
    -> Text<String>
}

fn main() {}

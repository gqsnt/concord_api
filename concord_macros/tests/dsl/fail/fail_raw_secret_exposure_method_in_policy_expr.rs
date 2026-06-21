use concord_core::prelude::*;
use concord_macros::api;

api! {
    client RawSecretExposureMethodPolicyApi {
        base "https://example.com"
    }

    GET Ping(token: SecretString)
    path ["ping"]
    header "X-Leak" = token.r#expose()
    -> Text<String>
}

fn main() {}

use concord_core::prelude::*;
use concord_macros::api;

api! {
    client SecretExposureMethodPolicyApi {
        base "https://example.com"
    }

    GET Ping(token: SecretString)
    path ["ping"]
    header "X-Leak" = token.expose()
    -> Text<String>
}

fn main() {}

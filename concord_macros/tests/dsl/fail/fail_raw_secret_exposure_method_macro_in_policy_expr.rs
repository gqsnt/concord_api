use concord_core::prelude::*;
use concord_macros::api;

api! {
    client RawSecretExposureMethodMacroPolicyApi {
        base "https://example.com"
    }

    GET Ping(token: SecretString)
    path ["ping"]
    header "X-Leak" = format!("{}", token.r#expose())
    -> Text<String>
}

fn main() {}

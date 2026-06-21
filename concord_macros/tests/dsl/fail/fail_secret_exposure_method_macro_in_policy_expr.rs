use concord_core::prelude::*;
use concord_macros::api;

api! {
    client SecretExposureMethodMacroPolicyApi {
        base "https://example.com"
    }

    GET Ping(token: SecretString)
    path ["ping"]
    header "X-Leak" = format!("{}", token.expose())
    -> Text<String>
}

fn main() {}

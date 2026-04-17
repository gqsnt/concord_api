use concord_macros::api;

api! {
    client UiRemovedAuthSecretRefAlias {
        scheme: https,
        host: "example.com",
        secret {
            api_key: String
        }
        auth {
            credential key: ApiKey(auth.api_key)
        }
    }
}

fn main() {}

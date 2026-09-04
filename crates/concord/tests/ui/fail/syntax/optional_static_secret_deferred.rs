use concord::api;

api! {
    client OptionalSecretApi {
        timeout 30s;
        base "https://example.test";
        credentials {
            secret optional_api_key?;
            credential key = api_key(secret.optional_api_key);
        }

        GET read() -> Bytes via Binary {
            auth { header "X-Api-Key" = key; }
        }
    }
}

fn main() {}

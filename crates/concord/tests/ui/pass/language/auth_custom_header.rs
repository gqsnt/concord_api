use concord::api;

api! {
    client CustomAuthHeaderApi {
        base "https://example.test/";
        credentials {
            secret key: String;
            credential key = api_key(secret.key);
        }
        auth header "X-API-Key" = key;
    }

    GET read() -> NoContent;
}

fn main() {}

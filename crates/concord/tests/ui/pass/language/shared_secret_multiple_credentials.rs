use concord::api;

api! {
    client SharedSecretApi {
        base "https://example.test/";
        credentials {
            secret shared: String;
            credential header_key = api_key(secret.shared);
            credential query_key = api_key(secret.shared);
        }
        auth header "x-api-key" = header_key;
        auth query "api_key" = query_key;
    }

    GET read() -> NoContent;
}

fn main() {}

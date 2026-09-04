use concord::api;

api! {
    client SharedSecretApi {
        timeout 30s;
        base "https://example.test/";
        credentials {
            secret shared;
            credential header_key = api_key(secret.shared);
            credential query_key = api_key(secret.shared);
        }
        auth {
            header "x-api-key" = header_key;
            query "api_key" = query_key;
        }


    GET read() -> () via Empty;
}}

fn main() {}

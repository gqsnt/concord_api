use concord::api;

api! {
    client CustomAuthHeaderApi {
        timeout 30s;
        base "https://example.test/";
        credentials {
            secret key;
            credential key = api_key(secret.key);
        }
        auth { header "X-API-Key" = key; }


    GET read() -> () via Empty;
}}

fn main() {}

use concord::api;

api! {
    client CustomAuthHeaderApi {
        timeout off;
        base "https://example.test/";
        credentials {
            secret key;
            credential key = api_key(secret.key);
        }
        auth { header "X-API-Key" = key; }


    GET read() -> () using NoContent;
}}

fn main() {}

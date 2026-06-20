use concord_macros::api;

api! {
    client DuplicateAuthHeaderEndpointApi {
        base "https://example.com"
        secret first: String
        secret second: String
        credential first_key = api_key(secret.first)
        credential second_key = api_key(secret.second)
    }

    GET Ping
        path ["ping"]
        auth header "X-Endpoint-Key" = first_key
        auth header "x-endpoint-key" = second_key
        -> concord_core::prelude::Json<String>
}

fn main() {}

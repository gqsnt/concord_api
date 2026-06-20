use concord_macros::api;

api! {
    client DuplicateAuthHeaderClientApi {
        base "https://example.com"
        secret first: String
        secret second: String
        credential first_key = api_key(secret.first)
        credential second_key = api_key(secret.second)

        auth header "X-Api-Key" = first_key
        auth header "x-api-key" = second_key
    }

    GET Ping
        path ["ping"]
        -> concord_core::prelude::Json<String>
}

fn main() {}

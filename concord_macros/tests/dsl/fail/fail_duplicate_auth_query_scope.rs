use concord_macros::api;

api! {
    client DuplicateAuthQueryScopeApi {
        base "https://example.com"
        secret first: String
        secret second: String
        credential first_key = api_key(secret.first)
        credential second_key = api_key(secret.second)
    }

    scope protected {
        auth query "api_key" = first_key
        auth query "api_key" = second_key

        GET Ping
            path ["ping"]
            -> concord_core::prelude::Json<String>
    }
}

fn main() {}

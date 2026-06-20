use concord_macros::api;

api! {
    client ReservedCredentialApi {
        base "https://example.com"
        credential session = endpoint auth_api::Login
    }

    GET AcquireAuthSession
        as acquire_auth_session
        path ["acquire-auth-session"]
        -> Json<String>

    scope auth_api {
        GET Login
            path ["login"]
            -> Json<String>
    }
}

fn main() {}

use concord_macros::api;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct LoginResponse {
    access_token: String,
}

#[derive(Debug, Deserialize)]
pub struct User;

api! {
    client EndpointAccessTokenAsBasicApi {
        base "https://example.com"
        credential session = endpoint auth_api::Login
    }

    scope auth_api {
        GET Login
            path ["login"]
            -> Json<LoginResponse>
            map AccessToken {
                AccessToken::new(r.access_token)
            }
    }

    scope protected {
        auth basic session

        GET Me
            path ["me"]
            -> Json<User>
    }
}

fn main() {}

use concord_macros::api;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct LoginResponse {
    username: String,
    password: String,
}

#[derive(Debug, Deserialize)]
pub struct User;

api! {
    client EndpointBasicAsBearerApi {
        base "https://example.com"
        credential session = endpoint auth_api::Login
    }

    scope auth_api {
        GET Login
            path ["login"]
            -> Json<LoginResponse>
            map BasicCredential {
                BasicCredential::new(r.username, r.password)
            }
    }

    scope protected {
        auth bearer session

        GET Me
            path ["me"]
            -> Json<User>
    }
}

fn main() {}

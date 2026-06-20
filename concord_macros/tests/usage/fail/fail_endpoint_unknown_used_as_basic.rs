use concord_macros::api;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct LoginResponse {
    token: String,
}

pub struct MyBasic(String);

#[derive(Debug, Deserialize)]
pub struct User;

api! {
    client EndpointUnknownAsBasicApi {
        base "https://example.com"
        credential session = endpoint auth_api::Login
    }

    scope auth_api {
        GET Login
            path ["login"]
            -> Json<LoginResponse>
            map MyBasic {
                MyBasic(r.token)
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

use concord_macros::api;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginRequest {
    username: String,
}

#[derive(Debug, Deserialize)]
pub struct User {
    name: String,
}

api! {
    client EndpointBasicUsedAsBearerApi {
        base "https://example.com"
        credential session = endpoint auth_api::Login
    }

    scope auth_api {
        POST Login(body: Json<LoginRequest>)
            path ["login"]
            -> Json<BasicCredential>
    }

    scope protected {
        auth bearer session

        GET Me
            path ["me"]
            -> Json<User>
    }
}

fn main() {}

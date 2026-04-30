use concord_core::prelude::*;
use concord_macros::api;
use serde::{Deserialize, Serialize};
use self::usage_non_credential_acquire_api::UsageNonCredentialAcquireApi;

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginRequest;

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginResponse {
    access_token: String,
}

api! {
    client UsageNonCredentialAcquireApi {
        base https "example.com"
        credential session = endpoint auth_api::LoginForSession
    }

    scope auth_api {
        POST LoginForSession(body: Json<LoginRequest>)
            path ["login"]
            -> Json<LoginResponse>
            map AccessToken { AccessToken::new(r.access_token) }
    }

    GET Ping
        path ["ping"]
        -> Json<String>
}

async fn bad_usage(api: UsageNonCredentialAcquireApi) -> Result<(), ApiClientError> {
    api.ping().acquire_as_session().await?;
    Ok(())
}

fn main() {}

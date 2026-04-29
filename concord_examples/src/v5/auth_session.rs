use concord_core::prelude::*;
use concord_macros::api;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginResponse {
    pub access_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Me {
    pub id: u64,
    pub username: String,
}

api! {
    client V5SessionApi {
        base https "example.com"

        secret upstream_key: String

        credential upstream = api_key(secret.upstream_key)
        credential session = endpoint auth_api::LoginForSession

        default {
            retry read
        }

        retry read {
            max_attempts 2
            methods [GET, POST]
            on [429, 500]
            retry_after
        }
    }

    scope auth_api {
        POST LoginForSession(body: Json<LoginRequest>)
            as login_for_session
            path ["login"]
            -> Json<LoginResponse>
            map AccessToken {
                AccessToken::new(r.access_token)
            }
        {
            auth header "X-Upstream-Key" = upstream
        }
    }

    scope protected {
        auth bearer session

        GET Me
            as me
            path ["me"]
            -> Json<Me>
    }
}

pub async fn session_auth_flow() -> Result<(), ApiClientError> {
    let api = v5_session_api::V5SessionApi::new("upstream-key".to_string());

    api.auth_api()
        .login_for_session(LoginRequest {
            username: "alice".to_string(),
            password: "secret".to_string(),
        })
        .acquire_as_session()
        .await?;

    let _me = api.protected().me().await?;
    api.auth_state().session().clear().await;
    Ok(())
}

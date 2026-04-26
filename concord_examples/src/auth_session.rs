use concord_core::prelude::*;
use concord_macros::api;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionLoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionLoginResponse {
    pub access_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionUser {
    pub id: u64,
    pub username: String,
}

api! {
    client SessionApi {
        base https "example.com"

        secret upstream_key: String

        credential upstream = api_key(secret.upstream_key)
        credential session = endpoint auth::LoginForSession
    }

    scope auth {
        POST LoginForSession(body: Json<SessionLoginRequest>) -> Json<SessionLoginResponse>
                map AccessToken {
            AccessToken::new(r.access_token)
        }
            {
            path ["login"]
            auth header "X-Upstream-Key" = upstream
        }
    }

    scope protected {
        auth bearer session

        GET Me -> Json<SessionUser> {
            path ["me"]
        }
    }
}

pub async fn session_flow_example() -> Result<(), ApiClientError> {
    let api = session_api::SessionApi::new("upstream-key".to_string());

    // This will fail until session is acquired.
    let _ = api
        .request(session_api::endpoints::protected::Me::new())
        .execute()
        .await;

    api.acquire_auth_session(session_api::endpoints::auth::LoginForSession::new(
        SessionLoginRequest {
            username: "alice".to_string(),
            password: "secret".to_string(),
        },
    ))
    .await?;

    let _me = api
        .request(session_api::endpoints::protected::Me::new())
        .execute()
        .await?;

    api.clear_auth_session().await;
    Ok(())
}

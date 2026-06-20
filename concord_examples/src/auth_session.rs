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
        base "https://example.com"

        secret upstream_key: String

        credential upstream = api_key(secret.upstream_key)
        credential session = endpoint auth_api::LoginForSession
    }

    scope auth_api {
        POST LoginForSession(body: Json<SessionLoginRequest>)
            path ["login"]
            auth header "X-Upstream-Key" = upstream
            -> Json<SessionLoginResponse>
            map AccessToken {
            AccessToken::new(r.access_token)
        }
    }

    scope protected {
        auth bearer session

        GET Me
            as me
            path ["me"]
            -> Json<SessionUser>
    }
}

pub use self::session_api::SessionApi;

pub async fn session_flow_example() -> Result<(), ApiClientError> {
    let api = session_api::SessionApi::new("upstream-key".to_string());

    // This fails until the session is explicitly acquired; integration tests
    // assert the exact error category.
    let missing = api.protected().me().await;
    debug_assert!(missing.is_err());

    api.auth_api()
        .login_for_session(SessionLoginRequest {
            username: "alice".to_string(),
            password: "secret".to_string(),
        })
        .acquire_as_session()
        .await?;

    let _me = api.protected().me().await?;

    api.auth_state()
        .session()
        .clear()
        .await
        .map_err(|source| ApiClientError::Auth {
            ctx: concord_core::advanced::ErrorContext {
                endpoint: "auth::session",
                method: http::Method::GET,
            },
            source,
        })?;
    Ok(())
}

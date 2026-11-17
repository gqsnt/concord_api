use concord::{form, Json, Secret};
use serde::Deserialize;

concord::api! {
    /// GitHub's token endpoint, where a GitHub App renews the token of one of its users.
    pub client GithubLogin {
        base "https://github.com";
        headers { "accept" = "application/json"; }
        read Json;

        /// A new access token and a new refresh token, for the refresh token that is sent.
        POST refresh(client_id: String, client_secret: Secret, refresh_token: Secret) -> TokenGrant {
            path "login/oauth/access_token";
            body form {
                client_id; client_secret; refresh_token;
                "grant_type" = "refresh_token";
            }
        }
    }
}

/// What the token endpoint returns: a grant, or an error that it may return with a `200`.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum TokenGrant {
    Granted { access_token: String, refresh_token: String, expires_in: u64 },
    Refused { error: String },
}

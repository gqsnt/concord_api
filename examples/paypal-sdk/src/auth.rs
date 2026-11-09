//! Client credentials and the token endpoint (`POST /v1/oauth2/token`,
//! `grant_type=client_credentials`). There is no refresh token: an expired access token is
//! obtained again with the same grant.

use std::time::Duration;

use concord::reqwest::StatusCode;
use concord::reqwest::header::{AUTHORIZATION, HeaderValue};
use concord::{ErrorKind, Form, Json};
use concord_auth::Token;
use serde::{Deserialize, Serialize};

use crate::{ERROR_BODY_LIMIT, Error, TOKEN_BODY_LIMIT};

/// A REST app's client ID and secret. Only their `Basic` header is kept, flagged sensitive;
/// `Debug` shows neither.
#[derive(Clone, concord::Debug)]
#[sensitive]
pub struct Credentials {
    basic: HeaderValue,
}

impl Credentials {
    /// # Errors
    ///
    /// [`Error::Invalid`] if either value is empty, the ID contains `:` or either contains a
    /// control character.
    pub fn new(client_id: &str, client_secret: &str) -> Result<Self, Error> {
        if client_id.is_empty() || client_secret.is_empty() {
            return Err(Error::Invalid("client ID and secret must not be empty"));
        }
        let basic = concord_auth::basic(client_id, client_secret)
            .map_err(|_| Error::Invalid("client ID or secret is not valid Basic credentials"))?;
        Ok(Self { basic })
    }
}

/// The form body `grant_type=client_credentials`.
#[derive(Clone, Copy, Serialize)]
pub(crate) struct ClientCredentials {
    grant_type: &'static str,
}

const CLIENT_CREDENTIALS: ClientCredentials = ClientCredentials {
    grant_type: "client_credentials",
};

/// The token document. Only what the SDK uses is decoded; `Debug` is redacted.
#[derive(Deserialize, concord::Debug)]
#[sensitive]
pub(crate) struct AccessToken {
    access_token: String,
    token_type: String,
    expires_in: u64,
}

concord::api! {
    /// The token endpoint. Its `Basic` credential is the only header this client sets.
    pub client OAuth(origin: concord::BaseUrl, #[sensitive] header(AUTHORIZATION) basic: HeaderValue) {
        base origin;
        error crate::auth::IdentityError in concord::Json;
        response Json;
        limit TOKEN_BODY_LIMIT;

        POST token(body grant: ClientCredentials) -> AccessToken {
            path "v1/oauth2/token";
            request Form;
        }
    }
}

impl OAuth {
    pub(crate) fn for_base(
        http: concord::reqwest::Client,
        credentials: Credentials,
        base: &str,
    ) -> Result<Self, concord::Error> {
        Ok(Self::new(
            http,
            OAuthConfig {
                origin: concord::BaseUrl::parse(base)?,
                basic: credentials.basic,
            },
        ))
    }
}

/// The identity error document of the token endpoint: `{"error": .., "error_description": ..}`.
#[derive(Clone, PartialEq, Eq, Deserialize, concord::Debug)]
pub struct IdentityError {
    error: String,
    #[serde(default)]
    #[sensitive]
    error_description: Option<String>,
}

impl IdentityError {
    /// The OAuth error code (`invalid_client`, ...).
    pub fn error(&self) -> &str {
        &self.error
    }

    /// The provider's description. Never formatted by this SDK.
    pub fn description_unredacted(&self) -> Option<&str> {
        self.error_description.as_deref()
    }
}

/// Why no access token could be obtained. It holds no credential, token or body; every waiter of
/// one shared acquisition receives a clone.
#[derive(Clone, Debug, PartialEq, Eq, concord::derive::Error)]
#[display(with = display_token_error)]
pub enum TokenError {
    /// The token endpoint answered with a non-2xx status (`401` with `invalid_client` for wrong
    /// credentials). `document` is `None` when the body was not a readable identity document.
    Refused {
        /// The HTTP status.
        status: StatusCode,
        /// The identity error document, when readable.
        document: Option<IdentityError>,
    },
    /// No usable answer: a transport failure, or a `2xx` whose head or body was refused
    /// (`Metadata`, `Decode`, `Limit`).
    Failed {
        /// The stage at which the token request failed.
        kind: ErrorKind,
    },
    /// A `2xx` token document that cannot be used (not `Bearer`, an invalid token, a lifetime
    /// not longer than the refresh margin).
    #[from(skip)]
    Unusable(&'static str),
}

fn display_token_error(value: &TokenError, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match value {
        TokenError::Refused { status, document } => {
            write!(f, "token request refused with HTTP {}", status.as_u16())?;
            if let Some(document) = document {
                write!(f, " ({})", document.error)?;
            }
            Ok(())
        }
        TokenError::Failed { kind } => write!(f, "token request failed ({kind:?})"),
        TokenError::Unusable(why) => write!(f, "token response unusable: {why}"),
    }
}

/// One client-credentials acquisition, run at most once at a time by the session's cache.
pub(crate) async fn fetch_token(oauth: OAuth, margin: Duration) -> Result<Token, TokenError> {
    let failed = |e: concord::Error| TokenError::Failed { kind: e.kind() };
    let received = oauth
        .token(CLIENT_CREDENTIALS)
        .prepare()
        .map_err(failed)?
        .receive()
        .await
        .map_err(failed)?;
    let status = received.status();
    if !status.is_success() {
        let (_, document) = received.failure(ERROR_BODY_LIMIT).await.into_document();
        return Err(TokenError::Refused {
            status,
            document: document.ok(),
        });
    }
    let token = received
        .into_success()
        .map_err(failed)?
        .decode()
        .await
        .map_err(failed)?;
    if !token.token_type.eq_ignore_ascii_case("bearer") {
        return Err(TokenError::Unusable("token_type is not Bearer"));
    }
    let header = concord_auth::bearer(&token.access_token)
        .map_err(|_| TokenError::Unusable("access_token is not a valid bearer token"))?;
    let lifetime = Duration::from_secs(token.expires_in);
    if lifetime <= margin {
        return Err(TokenError::Unusable(
            "expires_in is not longer than the refresh margin",
        ));
    }
    Ok(Token::new(header, lifetime))
}

#[cfg(test)]
mod sdk_type_canaries {
    use super::*;

    #[test]
    fn decoded_oauth_token_is_withheld_in_both_debug_modes() {
        let mut c = concord_check_canary::Canaries::new("paypal-token-body");
        let token = c.issue();
        let kind = c.issue();
        let body: AccessToken = serde_json::from_value(serde_json::json!({
            "access_token": token, "token_type": kind, "expires_in": 3600
        }))
        .unwrap();
        assert_eq!(body.access_token, token);
        assert_eq!(body.token_type, kind);
        c.assert_debug_absent("AccessToken", &body);
    }
}

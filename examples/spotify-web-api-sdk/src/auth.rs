//! The accounts service: the authorization URL, the code exchange and the refresh grant.
//!
//! The refresh rule is the provider's: a refresh response may omit `refresh_token`, and then the
//! refresh token the session holds stays valid. A missing (or empty) replacement never erases it.

use std::fmt;
use std::time::Duration;

use concord::reqwest::{
    self, Url,
    header::{AUTHORIZATION, HeaderValue},
};
use concord::{Form, Json};
use serde::{Deserialize, Serialize};

use crate::Error;
use crate::error::{AuthError, OAuthErrorObject, TokenFailure};
use crate::session::{Session, SessionOptions};

/// Error documents of the token endpoint are read up to this many bytes.
pub(crate) const TOKEN_ERROR_LIMIT: usize = 16 * 1024;

macro_rules! secret {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, PartialEq, Eq)]
        pub struct $name(String);

        impl $name {
            /// Wraps the secret text.
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            /// The secret text, for storage. Never log it.
            pub fn expose(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(concat!(stringify!($name), "(<redacted>)"))
            }
        }
    };
}

secret! {
    /// The app's client secret (Developer Dashboard).
    ClientSecret
}
secret! {
    /// A refresh token. Persist it after each call that may have rotated it
    /// ([`Session::refresh_token`]).
    RefreshToken
}
secret! {
    /// An authorization code returned to the redirect URI.
    AuthorizationCode
}
secret! {
    /// The `state` value the app generated for one authorization request (CSRF protection).
    State
}

/// The app's credentials, sent as HTTP Basic to the token endpoint.
#[derive(Clone)]
pub struct AppCredentials {
    client_id: String,
    basic: HeaderValue,
}

impl AppCredentials {
    /// Builds the Basic credential `base64(client_id:client_secret)`.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] for an empty client ID, a client ID with a colon, or control
    /// characters in either value.
    pub fn new(client_id: impl Into<String>, secret: &ClientSecret) -> Result<Self, Error> {
        let client_id = client_id.into();
        if client_id.is_empty() {
            return Err(Error::InvalidInput("the client ID is empty"));
        }
        let basic = concord_auth::basic(&client_id, secret.expose()).map_err(|_| {
            Error::InvalidInput("the client ID or secret cannot form a Basic credential")
        })?;
        Ok(Self { client_id, basic })
    }

    /// The client ID (public).
    pub fn client_id(&self) -> &str {
        &self.client_id
    }
}

impl fmt::Debug for AppCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppCredentials")
            .field("client_id", &self.client_id)
            .field("secret", &"<redacted>")
            .finish()
    }
}

/// The parameters of one authorization request (the user's browser is sent there).
#[derive(Clone)]
pub struct AuthorizeRequest {
    /// The redirect URI registered for the app.
    pub redirect_uri: String,
    /// The scopes to request (`playlist-read-private`, ...); sent space-separated.
    pub scopes: Vec<String>,
    /// The state the app checks on the redirect.
    pub state: State,
    /// Ask the user to approve again even if they already did (`show_dialog=true`).
    pub show_dialog: bool,
}

impl fmt::Debug for AuthorizeRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthorizeRequest")
            .field("redirect_uri", &self.redirect_uri)
            .field("scopes", &self.scopes)
            .field("state", &self.state)
            .field("show_dialog", &self.show_dialog)
            .finish()
    }
}

/// Space-separated scopes, one query value.
#[derive(Clone)]
pub(crate) struct Scopes(Vec<String>);

impl fmt::Display for Scopes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.join(" "))
    }
}

/// The form of a token request. Fields are written in this order; absent ones are skipped.
#[derive(Serialize)]
pub(crate) struct TokenForm {
    grant_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    redirect_uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    refresh_token: Option<String>,
}

/// The token endpoint's success document.
#[derive(Deserialize)]
pub(crate) struct TokenResponse {
    access_token: String,
    token_type: String,
    #[serde(default)]
    scope: Option<String>,
    expires_in: u64,
    #[serde(default)]
    refresh_token: Option<String>,
}

concord::api! {
    /// The accounts service (`accounts.spotify.com`).
    client Accounts {
        base "https://accounts.spotify.com";

        /// The authorization page. Never sent by the SDK: its prepared URL is what the user's
        /// browser opens.
        GET authorize(
            query client_id: String,
            query redirect_uri: String,
            query scope?: Scopes,
            #[sensitive] query("state") state: String,
            query show_dialog?: bool,
        ) {
            path "authorize";
            query { "response_type" = "code"; }
        }

        /// The token endpoint: code exchange and refresh.
        POST token(
            #[sensitive] header(AUTHORIZATION) credential: HeaderValue,
            body form: TokenForm,
        ) -> TokenResponse {
            path "api/token";
            request Form;
            response Json;
            limit 64 * 1024;
        }
    }
}

/// A token grant: what the code exchange (or a refresh) returned.
#[derive(Clone)]
pub struct Grant {
    pub(crate) access: HeaderValue,
    pub(crate) expires_in: Duration,
    pub(crate) refresh: Option<RefreshToken>,
    pub(crate) scope: Option<String>,
}

impl Grant {
    /// The access token's lifetime as the provider stated it (`expires_in`).
    pub fn expires_in(&self) -> Duration {
        self.expires_in
    }

    /// The refresh token, when the response carried one.
    pub fn refresh_token(&self) -> Option<&RefreshToken> {
        self.refresh.as_ref()
    }

    /// The granted scopes, space-separated, when the response listed them.
    pub fn scope(&self) -> Option<&str> {
        self.scope.as_deref()
    }
}

impl fmt::Debug for Grant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Grant")
            .field("access_token", &"<redacted>")
            .field("expires_in", &self.expires_in)
            .field(
                "refresh_token",
                &self.refresh.as_ref().map(|_| "<redacted>"),
            )
            .field("scope", &self.scope)
            .finish()
    }
}

/// The accounts service client of one app.
#[derive(Clone)]
pub struct OAuth {
    accounts: Accounts,
    app: AppCredentials,
}

impl fmt::Debug for OAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OAuth")
            .field("accounts", &self.accounts)
            .field("app", &self.app)
            .finish()
    }
}

impl OAuth {
    /// The accounts service at `https://accounts.spotify.com`, through `http`.
    pub fn new(http: reqwest::Client, app: AppCredentials) -> Self {
        Self {
            accounts: Accounts::new(http),
            app,
        }
    }

    /// Points the client at another accounts host (a local stand-in in tests).
    ///
    /// # Errors
    ///
    /// [`Error::Concord`] for a base URL Concord refuses.
    pub fn with_base_url(self, url: impl AsRef<str>) -> Result<Self, Error> {
        Ok(Self {
            accounts: self.accounts.with_base_url(url)?,
            app: self.app,
        })
    }

    /// The URL of the authorization page for `request` (`response_type=code`). Nothing is sent.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] for a redirect URI that is not an absolute URL or has a fragment
    /// (RFC 6749 section 3.1.2); [`Error::Concord`] if the URL cannot be built.
    pub fn authorize_url(&self, request: &AuthorizeRequest) -> Result<Url, Error> {
        let redirect = Url::parse(&request.redirect_uri)
            .map_err(|_| Error::InvalidInput("the redirect URI is not an absolute URL"))?;
        if redirect.fragment().is_some() {
            return Err(Error::InvalidInput("the redirect URI has a fragment"));
        }
        let scopes = (!request.scopes.is_empty()).then(|| Scopes(request.scopes.clone()));
        let prepared = self
            .accounts
            .authorize(
                self.app.client_id.clone(),
                request.redirect_uri.clone(),
                request.state.expose(),
            )
            .maybe_scope(scopes)
            .maybe_show_dialog(request.show_dialog.then_some(true))
            .prepare()?;
        Ok(prepared.request().url().clone())
    }

    /// Exchanges an authorization code (`grant_type=authorization_code`).
    ///
    /// # Errors
    ///
    /// [`Error::Auth`] with [`AuthError::InvalidGrant`] for an expired or reused code, another
    /// [`AuthError`] for any other refusal or a broken token document.
    pub async fn exchange_code(
        &self,
        code: &AuthorizationCode,
        redirect_uri: &str,
    ) -> Result<Grant, Error> {
        let form = TokenForm {
            grant_type: "authorization_code",
            code: Some(code.expose().to_owned()),
            redirect_uri: Some(redirect_uri.to_owned()),
            refresh_token: None,
        };
        Ok(self.request_token(form).await?)
    }

    /// Refreshes once (`grant_type=refresh_token`). A [`Session`] calls this itself; use it
    /// directly only to manage tokens without a session. The returned grant's refresh token is
    /// `None` when the provider did not rotate it: keep using `refresh_token` then.
    ///
    /// # Errors
    ///
    /// As [`exchange_code`](Self::exchange_code).
    pub async fn refresh(&self, refresh_token: &RefreshToken) -> Result<Grant, Error> {
        Ok(self.refresh_grant(refresh_token).await?)
    }

    /// Starts a session from a fresh grant: its access token is used until the refresh margin,
    /// then the grant's refresh token refreshes it.
    pub fn session(&self, grant: Grant, options: SessionOptions) -> Session {
        Session::start(self.clone(), Some(grant), None, options)
    }

    /// Resumes a session from a stored refresh token: the first request refreshes.
    pub fn resume(&self, refresh_token: RefreshToken, options: SessionOptions) -> Session {
        Session::start(self.clone(), None, Some(refresh_token), options)
    }

    pub(crate) async fn refresh_grant(
        &self,
        refresh_token: &RefreshToken,
    ) -> Result<Grant, AuthError> {
        let form = TokenForm {
            grant_type: "refresh_token",
            code: None,
            redirect_uri: None,
            refresh_token: Some(refresh_token.expose().to_owned()),
        };
        self.request_token(form).await
    }

    async fn request_token(&self, form: TokenForm) -> Result<Grant, AuthError> {
        let received = self
            .accounts
            .token(self.app.basic.clone(), form)
            .prepare()
            .map_err(AuthError::Concord)?
            .receive()
            .await
            .map_err(AuthError::Concord)?;
        if !received.status().is_success() {
            let (meta, document) = received
                .document::<Json, OAuthErrorObject>(TOKEN_ERROR_LIMIT)
                .await;
            let failure = Box::new(TokenFailure { meta, document });
            return Err(if failure.code() == Some("invalid_grant") {
                AuthError::InvalidGrant(failure)
            } else {
                AuthError::Refused(failure)
            });
        }
        let response = received.decode().await.map_err(AuthError::Concord)?;
        validate(response)
    }
}

/// The token document's contract: a Bearer token that is a valid header token, a positive
/// lifetime. An empty `refresh_token` counts as absent.
fn validate(response: TokenResponse) -> Result<Grant, AuthError> {
    if !response.token_type.eq_ignore_ascii_case("bearer") {
        return Err(AuthError::Protocol("the token type is not Bearer"));
    }
    if response.expires_in == 0 {
        return Err(AuthError::Protocol("the access token has no lifetime"));
    }
    let access = concord_auth::bearer(&response.access_token)
        .map_err(|_| AuthError::Protocol("the access token is not a valid bearer token"))?;
    let refresh = response
        .refresh_token
        .filter(|t| !t.is_empty())
        .map(RefreshToken);
    Ok(Grant {
        access,
        expires_in: Duration::from_secs(response.expires_in),
        refresh,
        scope: response.scope,
    })
}

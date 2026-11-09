//! OAuth 2.0 for QuickBooks Online: the app's credentials, the authorization-code exchange, and
//! one realm-bound [`Connection`] per connected company that refreshes its access token.
//!
//! Every token belongs to exactly one connection, and every connection to exactly one realm:
//! the access-token cache, the refresh token and its rejection state are per connection, never
//! shared or looked up across realms.

use std::fmt;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime};

use concord::Json;
use concord::reqwest::{self, StatusCode, header::HeaderValue};
use concord_auth::{RefreshError, Snapshot, Token, TokenCache};
use serde::{Deserialize, Serialize};

use crate::error::TokenFailure;
use crate::{
    AuthorizationError, AuthorizationKind, CallbackError, ERROR_BODY_LIMIT, Error, RealmId,
    TokenEndpoint, TokenEndpointConfig,
};

/// The OAuth scope of the Accounting API.
pub const ACCOUNTING_SCOPE: &str = "com.intuit.quickbooks.accounting";
/// The authorization endpoint (both environments, per the discovery documents).
const AUTHORIZE_URL: &str = "https://appcenter.intuit.com/connect/oauth2";
/// The token endpoint's origin (both environments).
const TOKEN_BASE: &str = "https://oauth.platform.intuit.com";
/// A cached access token is refreshed this long before its `expires_in` elapses.
const REFRESH_MARGIN: Duration = Duration::from_secs(60);

/// Which QuickBooks Online environment an app's keys and its companies belong to.
/// The connection's send policy selects this environment's host for either official HTTPS host,
/// including after [`QuickBooks::set_connection`](crate::QuickBooks::set_connection).
/// Loopback fixture origins are kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Environment {
    /// Development keys and sandbox companies: `https://sandbox-quickbooks.api.intuit.com`.
    Sandbox,
    /// Production keys and real companies: `https://quickbooks.api.intuit.com`.
    Production,
}

impl Environment {
    /// The Accounting API base URL.
    pub fn base_url(self) -> &'static str {
        match self {
            Self::Sandbox => "https://sandbox-quickbooks.api.intuit.com",
            Self::Production => "https://quickbooks.api.intuit.com",
        }
    }
}

fn secret_text(value: &str) -> bool {
    !value.is_empty() && !value.chars().any(char::is_control)
}

/// An OAuth refresh token. Secret: `Debug` withholds it and it has no `Display`.
#[derive(Clone, PartialEq, Eq, concord::Debug)]
pub struct RefreshToken(#[sensitive] String);

impl RefreshToken {
    /// Wraps a stored refresh token.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] when empty or containing control characters.
    pub fn new(value: impl Into<String>) -> Result<Self, Error> {
        let value = value.into();
        if secret_text(&value) {
            Ok(Self(value))
        } else {
            Err(Error::InvalidInput {
                what: "refresh token",
            })
        }
    }

    /// The token, for secure persistence.
    pub fn expose_secret(&self) -> &str {
        &self.0
    }
}

/// An OAuth authorization code from the redirect. Secret, and valid for one exchange only.
#[derive(Clone, PartialEq, Eq, concord::Debug)]
pub struct AuthorizationCode(#[sensitive] String);

impl AuthorizationCode {
    /// Wraps the `code` parameter.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] when empty or containing control characters.
    pub fn new(value: impl Into<String>) -> Result<Self, Error> {
        let value = value.into();
        if secret_text(&value) {
            Ok(Self(value))
        } else {
            Err(Error::InvalidInput {
                what: "authorization code",
            })
        }
    }
}

/// The parameters of the OAuth redirect to the app: `code`, `state` and `realmId`.
#[derive(Clone, Debug)]
pub struct AuthorizationCallback {
    code: AuthorizationCode,
    state: String,
    realm: RealmId,
}

impl AuthorizationCallback {
    /// The callback's parameters, already parsed.
    pub fn new(code: AuthorizationCode, state: impl Into<String>, realm: RealmId) -> Self {
        Self {
            code,
            state: state.into(),
            realm,
        }
    }

    /// Parses the redirect's query string (with or without the leading `?`).
    ///
    /// # Errors
    ///
    /// [`CallbackError::Denied`] for an `error` parameter, [`CallbackError::Missing`] for an
    /// absent, empty or repeated `code`, `state` or `realmId`, [`Error::InvalidInput`] for a
    /// `realmId` that is not a realm ID.
    pub fn from_query(query: &str) -> Result<Self, Error> {
        let url = reqwest::Url::parse(&format!(
            "https://callback.invalid/?{}",
            query.trim_start_matches('?')
        ))
        .map_err(|_| Error::InvalidInput {
            what: "OAuth callback query",
        })?;
        let pairs: Vec<(String, String)> = url.query_pairs().into_owned().collect();
        let one = |name: &'static str| -> Result<Option<String>, Error> {
            let mut values = pairs.iter().filter(|(k, _)| k == name).map(|(_, v)| v);
            match (values.next(), values.next()) {
                (None, _) => Ok(None),
                (Some(v), None) if !v.is_empty() => Ok(Some(v.clone())),
                _ => Err(Error::Callback(CallbackError::Missing(name))),
            }
        };
        if let Some(error) = one("error")? {
            return Err(Error::Callback(CallbackError::Denied(error)));
        }
        let required =
            |name: &'static str| one(name)?.ok_or(Error::Callback(CallbackError::Missing(name)));
        let code = AuthorizationCode::new(required("code")?)?;
        let state = required("state")?;
        let realm = RealmId::new(required("realmId")?)?;
        Ok(Self { code, state, realm })
    }

    /// The realm the user connected.
    pub fn realm(&self) -> &RealmId {
        &self.realm
    }

    /// The `state` the redirect carried.
    pub fn state(&self) -> &str {
        &self.state
    }

    fn check_state(&self, expected: &str) -> Result<(), Error> {
        if expected.is_empty() || self.state != expected {
            return Err(Error::Callback(CallbackError::StateMismatch));
        }
        Ok(())
    }
}

/// Token exchange and refresh send `authorization: Basic <base64(client_id:client_secret)>`,
/// constructed by `concord_auth::basic` from this app's credentials.
///
/// An app registered with Intuit: its client ID and secret (development keys for the sandbox,
/// production keys otherwise) and its redirect URI.
#[derive(Clone, concord::Debug)]
pub struct OAuthApp {
    environment: Environment,
    client_id: String,
    #[sensitive]
    credentials: HeaderValue,
    redirect_uri: String,
    token_base: String,
}

impl OAuthApp {
    /// The app's credentials.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] for an empty client ID or secret or a redirect URI that is not an
    /// `http`/`https` URL; [`Error::Credential`] when they cannot form a Basic credential.
    pub fn new(
        environment: Environment,
        client_id: &str,
        client_secret: &str,
        redirect_uri: &str,
    ) -> Result<Self, Error> {
        if client_id.is_empty() || client_secret.is_empty() {
            return Err(Error::InvalidInput {
                what: "OAuth client ID or secret",
            });
        }
        let credentials =
            concord_auth::basic(client_id, client_secret).map_err(Error::Credential)?;
        let redirect = reqwest::Url::parse(redirect_uri).map_err(|_| Error::InvalidInput {
            what: "redirect URI",
        })?;
        if !matches!(redirect.scheme(), "http" | "https") {
            return Err(Error::InvalidInput {
                what: "redirect URI",
            });
        }
        Ok(Self {
            environment,
            client_id: client_id.to_owned(),
            credentials,
            redirect_uri: redirect_uri.to_owned(),
            token_base: TOKEN_BASE.to_owned(),
        })
    }

    /// Points the token endpoint elsewhere (a local fixture); the path
    /// `/oauth2/v1/tokens/bearer` is kept.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] unless `url` is an `http`/`https` URL.
    pub fn with_token_base_url(mut self, url: &str) -> Result<Self, Error> {
        let parsed = reqwest::Url::parse(url).map_err(|_| Error::InvalidInput {
            what: "token endpoint URL",
        })?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(Error::InvalidInput {
                what: "token endpoint URL",
            });
        }
        url.clone_into(&mut self.token_base);
        Ok(self)
    }

    /// The app's environment.
    pub fn environment(&self) -> Environment {
        self.environment
    }

    /// The URL to send the user to: `client_id`, `response_type=code`, the Accounting scope,
    /// `redirect_uri` and your anti-forgery `state`, which the callback must return unchanged.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] for an empty `state`.
    pub fn authorization_url(&self, state: &str) -> Result<reqwest::Url, Error> {
        if state.is_empty() {
            return Err(Error::InvalidInput {
                what: "OAuth state",
            });
        }
        reqwest::Url::parse_with_params(
            AUTHORIZE_URL,
            [
                ("client_id", self.client_id.as_str()),
                ("response_type", "code"),
                ("scope", ACCOUNTING_SCOPE),
                ("redirect_uri", self.redirect_uri.as_str()),
                ("state", state),
            ],
        )
        .map_err(|_| Error::InvalidInput {
            what: "authorization URL",
        })
    }

    /// Exchanges the callback's code for the realm's first tokens: exactly one request, never
    /// repeated (Intuit warns that repeated exchanges may invalidate tokens). The access token
    /// is kept for the connection's first call.
    ///
    /// # Errors
    ///
    /// [`Error::Callback`] when `state` does not match (nothing sent); [`Error::Authorization`]
    /// when the token endpoint refuses or does not answer usably.
    pub async fn exchange_code(
        &self,
        http: &reqwest::Client,
        callback: AuthorizationCallback,
        expected_state: &str,
    ) -> Result<Connection, Error> {
        callback.check_state(expected_state)?;
        let grant = Grant::authorization_code(&callback.code, &self.redirect_uri);
        let issued = request_tokens(self, http, grant)
            .await
            .map_err(|failure| token_error(&callback.realm, Arc::new(failure)))?;
        let connection =
            Connection::new(self.clone(), callback.realm, issued.refresh_token.clone());
        connection.install(issued);
        Ok(connection)
    }

    /// Resumes a stored connection: the realm and **its** latest refresh token. The SDK cannot
    /// check that the token belongs to the realm; a token of another realm is refused by
    /// QuickBooks (401), never used for that other realm.
    pub fn connection(&self, realm: RealmId, refresh_token: RefreshToken) -> Connection {
        Connection::new(self.clone(), realm, refresh_token)
    }
}

/// The form body of a token request.
#[derive(Serialize, concord::Debug)]
pub(crate) struct Grant {
    grant_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[sensitive]
    code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    redirect_uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[sensitive]
    refresh_token: Option<String>,
}

impl Grant {
    fn authorization_code(code: &AuthorizationCode, redirect_uri: &str) -> Self {
        Self {
            grant_type: "authorization_code",
            code: Some(code.0.clone()),
            redirect_uri: Some(redirect_uri.to_owned()),
            refresh_token: None,
        }
    }

    fn refresh(token: &RefreshToken) -> Self {
        Self {
            grant_type: "refresh_token",
            code: None,
            redirect_uri: None,
            refresh_token: Some(token.0.clone()),
        }
    }
}

/// The token endpoint's success document.
#[derive(Deserialize, concord::Debug)]
#[sensitive]
pub(crate) struct TokenResponse {
    token_type: String,
    access_token: String,
    expires_in: u64,
    refresh_token: String,
    x_refresh_token_expires_in: Option<u64>,
}

/// An OAuth error document (RFC 6749 §5.2).
#[derive(Deserialize)]
struct OAuthError {
    error: String,
}

/// A validated token response.
struct Issued {
    access: HeaderValue,
    lifetime: Duration,
    refresh_token: RefreshToken,
    refresh_lifetime: Option<Duration>,
}

impl TokenResponse {
    fn issue(self) -> Result<Issued, TokenFailure> {
        // "Always store the latest refresh_token value": kept even when the rest is unusable.
        let Ok(refresh_token) = RefreshToken::new(self.refresh_token) else {
            return Err(TokenFailure::violation(
                "refresh_token is empty or invalid",
                None,
            ));
        };
        if !self.token_type.eq_ignore_ascii_case("bearer") {
            return Err(TokenFailure::violation(
                "token_type is not bearer",
                Some(refresh_token),
            ));
        }
        if self.expires_in == 0 {
            return Err(TokenFailure::violation(
                "expires_in is zero",
                Some(refresh_token),
            ));
        }
        let Ok(access) = concord_auth::bearer(&self.access_token) else {
            return Err(TokenFailure::violation(
                "access_token is not a bearer token",
                Some(refresh_token),
            ));
        };
        Ok(Issued {
            access,
            lifetime: Duration::from_secs(self.expires_in),
            refresh_token,
            refresh_lifetime: self.x_refresh_token_expires_in.map(Duration::from_secs),
        })
    }
}

/// One request to the token endpoint, classified. Never repeated here.
async fn request_tokens(
    app: &OAuthApp,
    http: &reqwest::Client,
    grant: Grant,
) -> Result<Issued, TokenFailure> {
    let config = TokenEndpointConfig {
        client_credentials: app.credentials.clone(),
    };
    let prepared = TokenEndpoint::new(http.clone(), config)
        .with_base_url(&app.token_base)
        .and_then(|endpoint| endpoint.bearer(grant).prepare())
        .map_err(|e| TokenFailure::local(AuthorizationKind::TokenServiceUnavailable, e))?;
    let received = prepared
        .receive()
        .await
        .map_err(|e| TokenFailure::local(AuthorizationKind::GrantUncertain, e))?;
    let status = received.status();
    if status.is_success() {
        return match received.decode().await {
            Ok(document) => document.issue(),
            Err(e) => Err(TokenFailure::local(AuthorizationKind::GrantUncertain, e)),
        };
    }
    let (_, document) = received
        .document::<Json, OAuthError>(ERROR_BODY_LIMIT)
        .await;
    let kind = if status == StatusCode::BAD_REQUEST || status == StatusCode::UNAUTHORIZED {
        AuthorizationKind::GrantRejected
    } else if status.is_server_error() {
        AuthorizationKind::GrantUncertain
    } else {
        AuthorizationKind::TokenServiceUnavailable
    };
    Err(TokenFailure {
        kind,
        status: Some(status),
        oauth_error: document.ok().map(|d| d.error),
        cause: None,
        violation: None,
        replacement: None,
    })
}

impl TokenFailure {
    fn local(kind: AuthorizationKind, cause: concord::Error) -> Self {
        Self {
            kind,
            status: cause.status(),
            oauth_error: None,
            cause: Some(cause),
            violation: None,
            replacement: None,
        }
    }

    fn violation(rule: &'static str, replacement: Option<RefreshToken>) -> Self {
        Self {
            kind: AuthorizationKind::GrantUncertain,
            status: None,
            oauth_error: None,
            cause: None,
            violation: Some(rule),
            replacement,
        }
    }
}

fn token_error(realm: &RealmId, failure: Arc<TokenFailure>) -> Error {
    Error::Authorization(Box::new(AuthorizationError::from_token(
        realm.clone(),
        failure,
    )))
}

/// A remembered `GrantRejected`: later calls fail the same way without asking again.
#[derive(Clone)]
struct Rejection {
    status: Option<StatusCode>,
    oauth_error: Option<String>,
}

/// The refresh side of a connection. Shared with the acquisition future, which never holds
/// the token cache itself (that would be a reference cycle).
struct Grants {
    refresh_token: RefreshToken,
    refresh_expires_at: Option<SystemTime>,
    rejected: Option<Rejection>,
    /// The access token of a code exchange, used once before any refresh.
    seed: Option<(HeaderValue, Instant)>,
    /// Bumped by every replacement, so a refresh started before it cannot overwrite it.
    epoch: u64,
}

impl Grants {
    fn store(&mut self, token: RefreshToken, lifetime: Option<Duration>) {
        self.refresh_token = token;
        self.refresh_expires_at = lifetime.and_then(|l| SystemTime::now().checked_add(l));
    }
}

struct Inner {
    app: OAuthApp,
    realm: RealmId,
    grants: Arc<Mutex<Grants>>,
    tokens: TokenCache<RealmId, TokenFailure>,
}

/// As the Accounting send policy, replaces `authorization` immediately before each attempt
/// with the current access-token snapshot (`Bearer <access_token>`); `prepare()` is token-free.
///
/// One connected company: its realm, its latest refresh token and its cached access token.
///
/// Clones share all three; the access token is refreshed once for every request waiting on it
/// (single flight). A connection never uses or changes another realm's tokens. Its `Display` is
/// the realm ID, which the [`QuickBooks`](crate::QuickBooks) declaration places in every path.
/// Its send policy also selects its environment's host when sending to either official HTTPS host;
/// replacing a client's connection switches realm, credentials and environment together.
/// Loopback fixture origins stay unchanged, and `prepare()` keeps the generated base snapshot.
#[derive(Clone)]
pub struct Connection(Arc<Inner>);

fn lock(grants: &Mutex<Grants>) -> MutexGuard<'_, Grants> {
    grants.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Connection {
    fn new(app: OAuthApp, realm: RealmId, refresh_token: RefreshToken) -> Self {
        Self(Arc::new(Inner {
            app,
            realm,
            grants: Arc::new(Mutex::new(Grants {
                refresh_token,
                refresh_expires_at: None,
                rejected: None,
                seed: None,
                epoch: 0,
            })),
            tokens: TokenCache::new(NonZeroUsize::MIN, REFRESH_MARGIN),
        }))
    }

    /// Installs freshly issued tokens, replacing whatever the connection held.
    fn install(&self, issued: Issued) {
        {
            let mut grants = lock(&self.0.grants);
            grants.epoch += 1;
            grants.store(issued.refresh_token, issued.refresh_lifetime);
            grants.rejected = None;
            grants.seed = Instant::now()
                .checked_add(issued.lifetime)
                .map(|expires| (issued.access, expires));
        }
        self.0.tokens.remove(&self.0.realm);
    }

    /// The realm every request of this connection goes to.
    pub fn realm(&self) -> &RealmId {
        &self.0.realm
    }

    /// The app's environment.
    pub fn environment(&self) -> Environment {
        self.0.app.environment
    }

    /// The latest refresh token, to persist after calls (QuickBooks may return a new one on any
    /// refresh, and only the latest is guaranteed to work).
    pub fn refresh_token(&self) -> RefreshToken {
        lock(&self.0.grants).refresh_token.clone()
    }

    /// When the latest refresh token expires, from the last token response's
    /// `x_refresh_token_expires_in` (`None` before the first one).
    pub fn refresh_token_expires_at(&self) -> Option<SystemTime> {
        lock(&self.0.grants).refresh_expires_at
    }

    /// `true` after the token endpoint rejected this connection's grant: every call fails with
    /// [`AuthorizationKind::GrantRejected`] without a request until the connection is
    /// reauthorized or given a newer refresh token.
    pub fn needs_reauthorization(&self) -> bool {
        lock(&self.0.grants).rejected.is_some()
    }

    /// Replaces the refresh token (one another process rotated, say) and forgets the cached
    /// access token. It must be **this realm's** token: the SDK cannot check that.
    pub fn replace_refresh_token(&self, token: RefreshToken) {
        {
            let mut grants = lock(&self.0.grants);
            grants.epoch += 1;
            grants.store(token, None);
            grants.rejected = None;
            grants.seed = None;
        }
        self.0.tokens.remove(&self.0.realm);
    }

    /// Recovers a rejected connection with a new authorization of the **same** realm: one code
    /// exchange, then the new tokens replace the old ones for every clone of this connection.
    ///
    /// # Errors
    ///
    /// [`CallbackError::StateMismatch`] or [`CallbackError::RealmMismatch`] (nothing sent, the
    /// connection unchanged); [`Error::Authorization`] when the exchange fails (unchanged too,
    /// except that a refresh token in a contract-breaking 2xx answer is stored).
    pub async fn reauthorize(
        &self,
        http: &reqwest::Client,
        callback: AuthorizationCallback,
        expected_state: &str,
    ) -> Result<(), Error> {
        callback.check_state(expected_state)?;
        if callback.realm != self.0.realm {
            return Err(Error::Callback(CallbackError::RealmMismatch));
        }
        let grant = Grant::authorization_code(&callback.code, &self.0.app.redirect_uri);
        match request_tokens(&self.0.app, http, grant).await {
            Ok(issued) => {
                self.install(issued);
                Ok(())
            }
            Err(mut failure) => {
                // A refresh token in an otherwise unusable answer is still the latest one.
                if let Some(replacement) = failure.replacement.take() {
                    self.replace_refresh_token(replacement);
                }
                Err(token_error(&self.0.realm, Arc::new(failure)))
            }
        }
    }

    /// The access token for the next request: cached, or acquired once for all waiters.
    pub(crate) async fn access_token(&self, http: &reqwest::Client) -> Result<Snapshot, Error> {
        let grants = Arc::clone(&self.0.grants);
        let app = self.0.app.clone();
        let http = http.clone();
        self.0
            .tokens
            .get_or_refresh(self.0.realm.clone(), move || acquire(grants, app, http))
            .await
            .map_err(|e| {
                let failure = match e {
                    RefreshError::Acquire(failure) => failure,
                    RefreshError::LifetimeOverflow => Arc::new(TokenFailure::violation(
                        "expires_in exceeds the clock",
                        None,
                    )),
                    RefreshError::Capacity => Arc::new(TokenFailure {
                        kind: AuthorizationKind::TokenServiceUnavailable,
                        status: None,
                        oauth_error: None,
                        cause: None,
                        violation: None,
                        replacement: None,
                    }),
                };
                token_error(&self.0.realm, failure)
            })
    }

    /// Forgets exactly the token a request was rejected with (never a newer one).
    pub(crate) fn discard(&self, used: &Snapshot) {
        self.0.tokens.invalidate(&self.0.realm, &used.generation());
    }
}

/// The acquisition behind the cache: the code exchange's token once, else one refresh.
async fn acquire(
    grants: Arc<Mutex<Grants>>,
    app: OAuthApp,
    http: reqwest::Client,
) -> Result<Token, TokenFailure> {
    let (grant, epoch) = {
        let mut state = lock(&grants);
        if let Some((header, expires)) = state.seed.take()
            && let Some(left) = expires.checked_duration_since(Instant::now())
        {
            return Ok(Token::new(header, left));
        }
        if let Some(rejection) = &state.rejected {
            return Err(TokenFailure {
                kind: AuthorizationKind::GrantRejected,
                status: rejection.status,
                oauth_error: rejection.oauth_error.clone(),
                cause: None,
                violation: None,
                replacement: None,
            });
        }
        (Grant::refresh(&state.refresh_token), state.epoch)
    };
    let outcome = request_tokens(&app, &http, grant).await;
    let mut state = lock(&grants);
    let current = state.epoch == epoch;
    match outcome {
        Ok(issued) => {
            if current {
                state.store(issued.refresh_token, issued.refresh_lifetime);
            }
            Ok(Token::new(issued.access, issued.lifetime))
        }
        Err(mut failure) => {
            if current {
                if let Some(replacement) = failure.replacement.take() {
                    state.store(replacement, None);
                }
                if failure.kind == AuthorizationKind::GrantRejected {
                    state.rejected = Some(Rejection {
                        status: failure.status,
                        oauth_error: failure.oauth_error.clone(),
                    });
                }
            }
            Err(failure)
        }
    }
}

impl fmt::Display for Connection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0.realm, f)
    }
}

impl fmt::Debug for Connection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Connection")
            .field("realm", &self.0.realm)
            .field("environment", &self.0.app.environment)
            .field("tokens", &"<redacted>")
            .finish()
    }
}

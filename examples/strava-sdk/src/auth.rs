//! OAuth rotation and the single owner of application retries.
use crate::{Error, OAuth, RateLimits, Response, Strava, StravaConfig, StravaOperation, finish};
use concord::{
    Endpoint, SendWith,
    reqwest::{self, header::AUTHORIZATION},
};
use concord_auth::{Token, TokenCache};
use concord_ratelimit::{Limiter, Mode, TokioClock};
use concord_retry::{Decision, Policy, Replay, Retry};
use serde::{Deserialize, Serialize};
use std::{
    num::NonZeroUsize,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex;

/// Application credentials. Debug withholds client ID and secret.
#[derive(Clone, concord::Debug)]
#[sensitive]
pub struct Credentials {
    client_id: u64,
    client_secret: String,
}
impl Credentials {
    /// Validate application credentials without performing I/O.
    pub fn new(client_id: u64, client_secret: String) -> Result<Self, Error> {
        if client_id == 0 || client_secret.is_empty() {
            return Err(Error::Invalid("empty OAuth credentials"));
        }
        Ok(Self {
            client_id,
            client_secret,
        })
    }
}

/// OAuth tokens. Persist the latest refresh token after each successful rotation.
#[derive(Clone, Deserialize, concord::Debug)]
#[sensitive]
pub struct Tokens {
    /// Expected to be Bearer.
    pub token_type: String,
    /// Sensitive access token; native access is deliberate.
    pub access_token: String,
    /// Sensitive rotating refresh token.
    pub refresh_token: String,
    /// Access-token expiry as Unix seconds.
    pub expires_at: u64,
    /// Provider-reported remaining lifetime.
    pub expires_in: u64,
    /// Granted scopes, when supplied by the provider.
    pub scope: Option<String>,
}

/// A single-use OAuth grant, sent in a URL-encoded body, never in the URL.
#[derive(Serialize, concord::Debug)]
#[sensitive]
pub struct TokenRequest {
    client_id: u64,
    client_secret: String,
    grant_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    refresh_token: Option<String>,
}
impl TokenRequest {
    /// Build an authorization-code exchange. The application validates callback state and scopes.
    pub fn authorization_code(credentials: &Credentials, code: String) -> Result<Self, Error> {
        if code.is_empty() {
            return Err(Error::Invalid("empty authorization code"));
        }
        Ok(Self {
            client_id: credentials.client_id,
            client_secret: credentials.client_secret.clone(),
            grant_type: "authorization_code",
            code: Some(code),
            refresh_token: None,
        })
    }
    fn refresh(credentials: &Credentials, refresh_token: String) -> Self {
        Self {
            client_id: credentials.client_id,
            client_secret: credentials.client_secret.clone(),
            grant_type: "refresh_token",
            code: None,
            refresh_token: Some(refresh_token),
        }
    }
}

struct Inner {
    http: reqwest::Client,
    origin: reqwest::Url,
    oauth: OAuth,
    credentials: Credentials,
    // Shared with the acquisition future the cache stores, which must not own `Inner`.
    tokens: Arc<Mutex<Tokens>>,
    cache: TokenCache<(), Error>,
    limiter: Limiter<u8>,
}
/// The send policy sets `authorization: Bearer <access_token>` from its current token snapshot
/// before each attempt; generated `prepare()` bypasses token acquisition and this edit.
///
/// A clone shares one athlete's tokens and this process's observed application quota.
/// Share a session among clones; independent sessions do not coordinate application-wide usage.
#[derive(Clone, concord::Debug)]
pub struct Session(#[sensitive] Arc<Inner>);

impl Session {
    /// Create a shared session using the SDK's strict transport. Retain a clone for token persistence.
    pub fn connect(
        origin: &str,
        oauth_origin: &str,
        credentials: Credentials,
        tokens: Tokens,
        timeout: Duration,
    ) -> Result<Self, Error> {
        Self::new(
            crate::transport(timeout)?,
            origin,
            oauth_origin,
            credentials,
            tokens,
        )
    }
    /// Build a generated client sharing this session and its strict transport.
    pub fn client(&self) -> Strava {
        Strava::new(
            self.0.http.clone(),
            StravaConfig {
                policy: self.clone(),
            },
        )
        .with_base_url(self.0.origin.as_str())
        .expect("validated origin")
    }
    pub(crate) fn new(
        http: reqwest::Client,
        origin: &str,
        oauth_origin: &str,
        credentials: Credentials,
        tokens: Tokens,
    ) -> Result<Self, Error> {
        let origin = validate_origin(origin)?;
        validate_origin(oauth_origin)?;
        if tokens.refresh_token.is_empty() {
            return Err(Error::Invalid("empty refresh token"));
        }
        Ok(Self(Arc::new(Inner {
            http: http.clone(),
            origin,
            oauth: OAuth::new(http).with_base_url(oauth_origin)?,
            credentials,
            tokens: Arc::new(Mutex::new(tokens)),
            cache: TokenCache::new(NonZeroUsize::new(1).unwrap(), Duration::from_secs(30)),
            limiter: Limiter::new(NonZeroUsize::new(4).unwrap(), TokioClock::default()),
        })))
    }
    /// Snapshot for durable caller-owned persistence; Debug redacts every token.
    /// In production, persist this after calls that may refresh, including failed API calls.
    pub async fn latest_tokens(&self) -> Tokens {
        self.0.tokens.lock().await.clone()
    }

    async fn token(&self) -> Result<concord_auth::Snapshot, Error> {
        // The cache keeps this future while it is in flight: capture its inputs, not `Inner`.
        let (tokens, oauth, credentials) = (
            self.0.tokens.clone(),
            self.0.oauth.clone(),
            self.0.credentials.clone(),
        );
        self.0
            .cache
            .get_or_refresh((), move || async move {
                let refresh_token = {
                    let stored = tokens.lock().await;
                    (stored.expires_at.saturating_sub(unix_now()?) <= 30)
                        .then(|| stored.refresh_token.clone())
                };
                // The lock is not held across the provider call: the cache keeps a cancelled
                // refresh paused, and readers of the stored tokens must not wait on it. The cache
                // already runs one acquisition at a time.
                if let Some(refresh_token) = refresh_token {
                    let next = oauth
                        .oauth()
                        .token(TokenRequest::refresh(&credentials, refresh_token))
                        .send()
                        .await?
                        .value;
                    // Retain the latest rotated refresh token even if the access-token document is unusable.
                    if next.refresh_token.is_empty() {
                        return Err(Error::Invalid("empty rotated refresh token"));
                    }
                    let mut stored = tokens.lock().await;
                    let scope = next.scope.clone().or_else(|| stored.scope.clone());
                    *stored = next;
                    stored.scope = scope;
                }
                let stored = tokens.lock().await;
                if !stored.token_type.eq_ignore_ascii_case("bearer") {
                    return Err(Error::Invalid("unsupported token type"));
                }
                let header = concord_auth::bearer(&stored.access_token)
                    .map_err(|_| Error::Invalid("invalid access token"))?;
                let lifetime = stored
                    .expires_in
                    .min(stored.expires_at.saturating_sub(unix_now()?));
                if lifetime <= 30 {
                    return Err(Error::Invalid("unusable token lifetime"));
                }
                Ok(Token::new(header, Duration::from_secs(lifetime)))
            })
            .await
            .map_err(Error::Auth)
    }
}
pub(crate) fn validate_origin(origin: &str) -> Result<reqwest::Url, Error> {
    let url = reqwest::Url::parse(origin).map_err(|_| Error::Invalid("invalid origin"))?;
    let local = matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"));
    if !(url.scheme() == "https" || (url.scheme() == "http" && local))
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
        || url.port() == Some(0)
    {
        return Err(Error::Invalid(
            "expected HTTPS origin or loopback fixture origin",
        ));
    }
    Ok(url)
}
fn unix_now() -> Result<u64, Error> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|_| Error::Invalid("clock before Unix epoch"))
}
/// Per-operation retry timing, quota and permission decisions.
struct Class {
    /// Delay before a read replay; a status poll keeps the one-second poll spacing.
    retry_delay: Duration,
    /// Charged to the non-upload windows too. The rate-limits page exempts only activity
    /// creation, uploads and media uploads; of this SDK's operations, only `uploads.create`.
    non_upload: bool,
    /// Permission that granted scopes, when known, must include before anything is sent.
    scope: Option<&'static str>,
}
impl Class {
    /// Default retry timing, quota and permission settings.
    const DEFAULT: Self = Self {
        retry_delay: Duration::from_millis(25),
        non_upload: true,
        scope: None,
    };

    /// No wildcard arm: an endpoint added to the declaration without a class here does not
    /// compile (E0004, naming its `StravaOperation` variant).
    fn of(operation: StravaOperation) -> Self {
        use StravaOperation as Op;
        match operation {
            Op::AthleteActivities | Op::ActivitiesGet => Self::DEFAULT,
            Op::UploadsStatus => Self {
                retry_delay: Duration::from_secs(1),
                ..Self::DEFAULT
            },
            Op::ActivitiesUpdate => Self {
                scope: Some("activity:write"),
                ..Self::DEFAULT
            },
            // Uploads also need `activity:write`; they keep their pre-update behavior (sent).
            Op::UploadsCreate => Self {
                non_upload: false,
                ..Self::DEFAULT
            },
        }
    }
}
/// Granted scopes as stored: space-delimited from the token exchange; commas are also accepted.
fn granted(scopes: &str, required: &str) -> bool {
    scopes
        .split(|c: char| c == ',' || c.is_ascii_whitespace())
        .any(|scope| scope == required)
}

impl<T: Send> SendWith<T> for Session {
    type Output = Result<Response<T>, Error>;
    async fn send<E: Endpoint<T> + Send>(&self, endpoint: E) -> Self::Output {
        let operation = StravaOperation::from_operation(E::operation())
            .ok_or(Error::Invalid("operation is not declared by this SDK"))?;
        let read = E::operation().is_read();
        let uncertain = !read;
        let Class {
            retry_delay,
            non_upload,
            scope,
        } = Class::of(operation);
        if let Some(required) = scope {
            let stored = self.0.tokens.lock().await;
            if stored
                .scope
                .as_deref()
                .is_some_and(|s| !granted(s, required))
            {
                return Err(Error::MissingScope(required));
            }
        }
        let (client, request, decoder) = endpoint
            .prepare()
            .map_err(|e| Error::from_call(e, uncertain))?
            .into_parts();
        if request.url().origin() != self.0.origin.origin() {
            return Err(Error::Invalid("credential destination changed"));
        }
        let template = if read { request.try_clone() } else { None };
        let mut first = Some(request);
        let mut policy = Policy::new(NonZeroUsize::new(if read { 2 } else { 1 }).unwrap());
        policy.initial_delay = retry_delay;
        policy.max_delay = retry_delay;
        let run = Retry::new(
            policy,
            if read {
                Replay::Permitted
            } else {
                Replay::Never
            },
        )
        .run_tokio(
            |_| {
                let request = first
                    .take()
                    .or_else(|| template.as_ref().and_then(reqwest::Request::try_clone));
                let client = client.clone();
                async move {
                    let mut request = request.ok_or(Error::Invalid("request is not replayable"))?;
                    let used = self.token().await?;
                    request
                        .headers_mut()
                        .insert(AUTHORIZATION, used.header().clone());
                    let keys: &[u8] = if non_upload { &[0, 1, 2, 3] } else { &[0, 1] };
                    self.0
                        .limiter
                        .acquire(keys, Mode::Refuse)
                        .await
                        .map_err(Error::Admission)?;
                    let received = decoder
                        .execute(&client, request)
                        .await
                        .map_err(|e| Error::from_call(e, uncertain))?;
                    if received.status() == reqwest::StatusCode::UNAUTHORIZED {
                        let mut stored = self.0.tokens.lock().await;
                        if self.0.cache.invalidate(&(), &used.generation()) {
                            stored.expires_at = 0;
                        }
                    }
                    RateLimits::parse(received.head().headers()).observe(
                        &self.0.limiter,
                        received.status().as_u16(),
                        non_upload,
                        received.head().headers(),
                    )?;
                    Ok(received)
                }
            },
            |result| match result {
                Ok(received) if read && matches!(received.status().as_u16(), 502..=504) => {
                    Decision::RetryAfter(None)
                }
                Err(Error::Concord(e))
                    if read && e.kind() == concord::ErrorKind::Transport && e.head().is_none() =>
                {
                    Decision::RetryAfter(None)
                }
                _ => Decision::Done,
            },
            |_, ceiling| ceiling,
        )
        .await;
        let received = run
            .outcome
            .ok_or(Error::Invalid("no application attempt completed"))??;
        finish(received, uncertain, run.report.attempts).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Abandoning every caller while a refresh is in flight must not keep the session (its
    /// tokens, credentials, HTTP client and cache) alive: the cache keeps the paused acquisition.
    #[tokio::test]
    async fn abandoned_refresh_does_not_keep_the_session_alive() {
        // Accepts the connection and never answers, so the refresh stays in flight.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap();
        let tokens = Tokens {
            token_type: "Bearer".into(),
            access_token: "access".into(),
            refresh_token: "refresh".into(),
            expires_at: 0, // expired: the first call refreshes
            expires_in: 0,
            scope: None,
        };
        let credentials = Credentials::new(1, "secret".into()).unwrap();
        let session = Session::new(http, &url, &url, credentials, tokens).unwrap();
        let state = Arc::downgrade(&session.0);
        let caller = tokio::spawn({
            let session = session.clone();
            async move { session.token().await.map(drop) }
        });
        let (_connection, _) = tokio::task::spawn_blocking(move || listener.accept())
            .await
            .unwrap()
            .unwrap();
        caller.abort();
        assert!(caller.await.unwrap_err().is_cancelled());
        drop(session);
        assert!(
            state.upgrade().is_none(),
            "the abandoned refresh keeps the session alive"
        );
    }

    /// A refresh cancelled mid-request stays paused in the cache; the stored tokens must stay
    /// readable meanwhile (and the update path, which reads the granted scope first, usable).
    #[tokio::test]
    async fn an_abandoned_refresh_does_not_block_token_readers() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap();
        let tokens = Tokens {
            token_type: "Bearer".into(),
            access_token: "access".into(),
            refresh_token: "refresh".into(),
            expires_at: 0,
            expires_in: 0,
            scope: Some("read".into()),
        };
        let credentials = Credentials::new(1, "secret".into()).unwrap();
        let session = Session::new(http, &url, &url, credentials, tokens).unwrap();
        let caller = tokio::spawn({
            let session = session.clone();
            async move { session.token().await.map(drop) }
        });
        let (_connection, _) = tokio::task::spawn_blocking(move || listener.accept())
            .await
            .unwrap()
            .unwrap();
        caller.abort();
        assert!(caller.await.unwrap_err().is_cancelled());
        let stored = tokio::time::timeout(Duration::from_secs(5), session.latest_tokens())
            .await
            .expect("the paused refresh holds the token lock");
        assert_eq!(stored.refresh_token, "refresh");
    }
}

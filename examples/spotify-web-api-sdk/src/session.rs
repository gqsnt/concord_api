//! One user's authorization: the access-token cache, the refresh token, and the send policy of
//! every Web API request.
//!
//! - **Single flight.** Concurrent requests that find no usable access token share one refresh
//!   ([`concord_auth::TokenCache`], one key per session).
//! - **The refresh token survives a refresh that does not rotate it.** It is replaced only by a
//!   non-empty `refresh_token` in a successful refresh response.
//! - **401** invalidates exactly the access token that request used, refreshes once and replays
//!   the request once (every operation of this SDK is a `GET`). A second 401 is final.
//! - **403 and 404** never refresh or invalidate anything.
//! - **429** is returned as [`ApiClass::RateLimited`](crate::ApiClass::RateLimited) with its
//!   `Retry-After`, or retried after that delay when [`SessionOptions::rate_limit_retries`] allows
//!   it and the delay is within [`SessionOptions::max_retry_wait`]. A 429 whose document says
//!   `QUOTA_EXCEEDED` is never retried.

use std::fmt;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime};

use concord::reqwest::{Method, StatusCode, header::AUTHORIZATION, header::HeaderValue};
use concord::{Endpoint, Json, Received, SendWith};
use concord_auth::{RefreshError, Snapshot, Token, TokenCache};
use concord_retry::{Decision, Policy, Replay, Retry, http::RetryAfterError};

use crate::auth::{Grant, OAuth, RefreshToken};
use crate::error::{ApiFailure, AuthError, ErrorEnvelope, classify};
use crate::{ApiClass, Error};

/// Web API error documents are read up to this many bytes.
pub const ERROR_BODY_LIMIT: usize = 64 * 1024;

type Clock = Arc<dyn Fn() -> Instant + Send + Sync>;

/// How a [`Session`] refreshes and repeats requests. Build it with [`SessionOptions::new`] and
/// the setters; every field has a default.
#[derive(Clone)]
pub struct SessionOptions {
    refresh_margin: Duration,
    rate_limit_retries: u8,
    max_retry_wait: Duration,
    deadline: Option<Duration>,
    clock: Option<Clock>,
}

impl Default for SessionOptions {
    fn default() -> Self {
        Self {
            refresh_margin: Duration::from_secs(60),
            rate_limit_retries: 0,
            max_retry_wait: Duration::from_secs(5),
            deadline: None,
            clock: None,
        }
    }
}

impl fmt::Debug for SessionOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SessionOptions")
            .field("refresh_margin", &self.refresh_margin)
            .field("rate_limit_retries", &self.rate_limit_retries)
            .field("max_retry_wait", &self.max_retry_wait)
            .field("deadline", &self.deadline)
            .field("clock", &self.clock.as_ref().map(|_| "<injected>"))
            .finish()
    }
}

impl SessionOptions {
    /// The defaults: refresh 60 s before expiry, no automatic 429 retry, a 5 s ceiling for a
    /// retried 429's `Retry-After`, no total deadline, the system monotonic clock.
    pub fn new() -> Self {
        Self::default()
    }

    /// Refresh an access token this long before the provider's `expires_in` ends. A token
    /// response whose lifetime is not longer than this margin is refused.
    #[must_use]
    pub fn refresh_margin(mut self, margin: Duration) -> Self {
        self.refresh_margin = margin;
        self
    }

    /// Retry a 429 (not `QUOTA_EXCEEDED`) up to `retries` times, waiting its `Retry-After`. A 429
    /// without a valid `Retry-After`, or with a delay above [`max_retry_wait`](Self::max_retry_wait),
    /// is returned at once.
    #[must_use]
    pub fn rate_limit_retries(mut self, retries: u8) -> Self {
        self.rate_limit_retries = retries;
        self
    }

    /// The longest `Retry-After` the session waits on its own.
    #[must_use]
    pub fn max_retry_wait(mut self, wait: Duration) -> Self {
        self.max_retry_wait = wait;
        self
    }

    /// One total budget per call, covering refreshes, attempts and waits.
    #[must_use]
    pub fn deadline(mut self, deadline: Duration) -> Self {
        self.deadline = Some(deadline);
        self
    }

    /// Replaces the monotonic clock that decides access-token expiry (deterministic tests).
    #[must_use]
    pub fn clock(mut self, now: impl Fn() -> Instant + Send + Sync + 'static) -> Self {
        self.clock = Some(Arc::new(now));
        self
    }
}

/// What the session knows about the refresh token.
enum RefreshState {
    Usable(RefreshToken),
    Absent,
    /// Answered `invalid_grant`: never sent again.
    Rejected,
}

struct GrantState {
    refresh: RefreshState,
    /// The access token of the grant the session started from, until its first use.
    seed: Option<(HeaderValue, Instant)>,
    scope: Option<String>,
}

/// What the token acquisition closure owns (never the cache itself).
struct Acquirer {
    oauth: OAuth,
    state: Mutex<GrantState>,
    now: Clock,
    margin: Duration,
}

impl Acquirer {
    fn state(&self) -> MutexGuard<'_, GrantState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    async fn acquire(&self) -> Result<Token, AuthError> {
        let refresh_token = {
            let mut state = self.state();
            if let Some((header, expires_at)) = state.seed.take() {
                let remaining = expires_at.saturating_duration_since((self.now)());
                if remaining > self.margin {
                    return Ok(Token::new(header, remaining));
                }
            }
            match &state.refresh {
                RefreshState::Usable(token) => token.clone(),
                RefreshState::Absent => return Err(AuthError::NoRefreshToken),
                RefreshState::Rejected => return Err(AuthError::ReauthorizationRequired),
            }
        };
        match self.oauth.refresh_grant(&refresh_token).await {
            Ok(grant) => {
                if grant.expires_in <= self.margin {
                    return Err(AuthError::Protocol(
                        "the access token lifetime is not longer than the refresh margin",
                    ));
                }
                let mut state = self.state();
                // Only a non-empty replacement rotates the stored token; and only if the token
                // that was sent is still the stored one (a concurrent `replace_grant` wins).
                if let RefreshState::Usable(current) = &state.refresh
                    && *current == refresh_token
                    && let Some(rotated) = grant.refresh
                {
                    state.refresh = RefreshState::Usable(rotated);
                }
                if grant.scope.is_some() {
                    state.scope = grant.scope;
                }
                Ok(Token::new(grant.access, grant.expires_in))
            }
            Err(error) => {
                if matches!(error, AuthError::InvalidGrant(_)) {
                    let mut state = self.state();
                    if matches!(&state.refresh, RefreshState::Usable(current) if *current == refresh_token)
                    {
                        state.refresh = RefreshState::Rejected;
                    }
                }
                Err(error)
            }
        }
    }
}

struct Inner {
    cache: TokenCache<(), AuthError>,
    acquirer: Arc<Acquirer>,
    rate_limit_retries: u8,
    max_retry_wait: Duration,
    deadline: Option<Duration>,
}

/// One user's authorization, and the send policy of [`crate::WebApi`]. Cheap to clone; clones
/// share the token cache and the refresh token.
#[derive(Clone)]
pub struct Session {
    inner: Arc<Inner>,
}

impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = self.inner.acquirer.state();
        let refresh = match state.refresh {
            RefreshState::Usable(_) => "usable",
            RefreshState::Absent => "absent",
            RefreshState::Rejected => "rejected",
        };
        f.debug_struct("Session")
            .field("refresh_token", &refresh)
            .field("scope", &state.scope)
            .finish_non_exhaustive()
    }
}

impl Session {
    pub(crate) fn start(
        oauth: OAuth,
        grant: Option<Grant>,
        refresh: Option<RefreshToken>,
        options: SessionOptions,
    ) -> Self {
        let now: Clock = options.clock.unwrap_or_else(|| Arc::new(Instant::now));
        let started = now();
        let (seed, refresh, scope) = match grant {
            Some(grant) => (
                started
                    .checked_add(grant.expires_in)
                    .map(|expires_at| (grant.access, expires_at)),
                grant.refresh.or(refresh),
                grant.scope,
            ),
            None => (None, refresh, None),
        };
        let clock = now.clone();
        Self {
            inner: Arc::new(Inner {
                cache: TokenCache::with_clock(
                    NonZeroUsize::MIN,
                    options.refresh_margin,
                    move || clock(),
                ),
                acquirer: Arc::new(Acquirer {
                    oauth,
                    state: Mutex::new(GrantState {
                        refresh: refresh.map_or(RefreshState::Absent, RefreshState::Usable),
                        seed,
                        scope,
                    }),
                    now,
                    margin: options.refresh_margin,
                }),
                rate_limit_retries: options.rate_limit_retries,
                max_retry_wait: options.max_retry_wait,
                deadline: options.deadline,
            }),
        }
    }

    /// The refresh token to persist: the latest one the provider issued, or `None` when the
    /// session has none or it was rejected (`invalid_grant`).
    pub fn refresh_token(&self) -> Option<RefreshToken> {
        match &self.inner.acquirer.state().refresh {
            RefreshState::Usable(token) => Some(token.clone()),
            RefreshState::Absent | RefreshState::Rejected => None,
        }
    }

    /// The granted scopes (space-separated) of the latest token response that listed them.
    pub fn scope(&self) -> Option<String> {
        self.inner.acquirer.state().scope.clone()
    }

    /// Installs a new grant after the user authorized again: its access token and, when it has
    /// one, its refresh token. The cached access token is dropped.
    pub fn replace_grant(&self, grant: Grant) {
        let now = (self.inner.acquirer.now)();
        {
            let mut state = self.inner.acquirer.state();
            state.seed = now
                .checked_add(grant.expires_in)
                .map(|expires_at| (grant.access, expires_at));
            if let Some(refresh) = grant.refresh {
                state.refresh = RefreshState::Usable(refresh);
            }
            if grant.scope.is_some() {
                state.scope = grant.scope;
            }
        }
        self.inner.cache.remove(&());
    }

    /// A usable access token: cached, the seed of the starting grant, or one shared refresh.
    async fn token(&self) -> Result<Snapshot, Error> {
        let acquirer = self.inner.acquirer.clone();
        self.inner
            .cache
            .get_or_refresh((), move || async move { acquirer.acquire().await })
            .await
            .map_err(|error| match error {
                RefreshError::Acquire(shared) => Error::Auth(shared),
                RefreshError::LifetimeOverflow => {
                    AuthError::Cache("the token lifetime exceeds the clock").into()
                }
                RefreshError::Capacity => AuthError::Cache("no capacity for an acquisition").into(),
            })
    }
}

/// One attempt's outcome, classified before the retry owner decides.
enum Attempt<T> {
    /// Any answer the policy does not repeat, unread.
    Answer(Received<T>),
    /// 401: the access token used was invalidated; the response is unread.
    Unauthorized(Received<T>),
    /// 429: its document was read to tell quota exhaustion from rate limiting.
    Throttled(Box<ApiFailure>),
}

impl<T: Send + 'static> SendWith<T> for Session {
    type Output = Result<T, Error>;

    async fn send<E: Endpoint<T> + Send>(&self, endpoint: E) -> Result<T, Error> {
        let (http, request, decoder) = endpoint.prepare()?.into_parts();
        // Every operation of this SDK is a GET: replaying it (after a refresh, after a 429's
        // delay) is safe. Anything else would be sent once.
        let replay = if request.method() == Method::GET {
            Replay::Permitted
        } else {
            Replay::Never
        };
        let retries = usize::from(self.inner.rate_limit_retries);
        let mut policy = Policy::new(NonZeroUsize::MIN.saturating_add(1 + retries));
        policy.timeout = self.inner.deadline;
        let max_wait = self.inner.max_retry_wait;
        let mut replayed_unauthorized = false;
        let mut throttled = 0usize;
        let start = Instant::now();
        let run = Retry::new(policy, replay)
            .run(
                |_attempt| {
                    let request = request.try_clone();
                    let http = &http;
                    async move {
                        let mut request = request
                            .ok_or(Error::Protocol("the request body cannot be replayed"))?;
                        let used = self.token().await?;
                        request
                            .headers_mut()
                            .insert(AUTHORIZATION, used.header().clone());
                        let received = decoder.execute(http, request).await?;
                        match received.status() {
                            StatusCode::UNAUTHORIZED => {
                                self.inner.cache.invalidate(&(), &used.generation());
                                Ok(Attempt::Unauthorized(received))
                            }
                            StatusCode::TOO_MANY_REQUESTS => {
                                Ok(Attempt::Throttled(failure(received).await))
                            }
                            _ => Ok(Attempt::Answer(received)),
                        }
                    }
                },
                |outcome: &Result<Attempt<T>, Error>| match outcome {
                    Ok(Attempt::Unauthorized(_)) if !replayed_unauthorized => {
                        replayed_unauthorized = true;
                        Decision::RetryAfter(None)
                    }
                    Ok(Attempt::Throttled(failure))
                        if failure.class == ApiClass::RateLimited && throttled < retries =>
                    {
                        match failure.retry_after {
                            Ok(Some(delay)) if delay <= max_wait => {
                                throttled += 1;
                                Decision::RetryAfter(Some(delay))
                            }
                            _ => Decision::Done,
                        }
                    }
                    _ => Decision::Done,
                },
                || start.elapsed(),
                tokio::time::sleep,
                |_, ceiling| ceiling,
            )
            .await;
        match run.outcome.ok_or(Error::Deadline)?? {
            Attempt::Answer(received) | Attempt::Unauthorized(received) => finish(received).await,
            Attempt::Throttled(failure) => Err(Error::Api(failure)),
        }
    }
}

fn retry_after<T>(received: &Received<T>) -> Result<Option<Duration>, RetryAfterError> {
    let head = received.head();
    let values = head.headers().get_all("retry-after");
    concord_retry::http::retry_after(values.iter().map(|v| v.as_bytes()), SystemTime::now())
}

/// Reads a non-2xx answer's error document (bounded) into an [`ApiFailure`].
async fn failure<T>(received: Received<T>) -> Box<ApiFailure> {
    let retry_after = retry_after(&received);
    let (meta, document) = received
        .document::<Json, ErrorEnvelope>(ERROR_BODY_LIMIT)
        .await;
    let document = document.map(|envelope| envelope.error);
    let class = classify(meta.status().as_u16(), document.as_ref().ok());
    Box::new(ApiFailure {
        class,
        meta,
        document,
        retry_after,
    })
}

/// The final answer: a 2xx decodes as declared (bounded by the declaration's `limit`); anything
/// else is the provider's failure.
async fn finish<T>(received: Received<T>) -> Result<T, Error> {
    if received.status().is_success() {
        Ok(received.decode().await?)
    } else {
        Err(Error::Api(failure(received).await))
    }
}

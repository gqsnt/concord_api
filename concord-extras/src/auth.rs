//! An access token shared by the calls to one origin: [`Renewed`], a middleware layer
//! (feature `auth`).
//!
//! `Renewed` adds the token to each request and renews it when it expires. It never
//! resends a request. After a `401` from the origin of its base, it returns the response
//! unchanged and discards the cached token only if it is still the one that the request
//! carried; a newer token stays.
//! No other status discards a token, except a response that a function set with
//! [`Renewed::rejected_when`] recognises as a rejection: an API that reports an expired
//! token inside a `200`. [`after_401`] adds a retry after that `401` to the
//! engine's retry policy, without replacing the policy. Use `not_applied_on(401)` only if
//! the API guarantees that a request rejected with `401` had no effect. Without it, a
//! POST not marked as safe to repeat is not retried. A stream body that can be sent only
//! once is never retried; a repeatable stream follows the usual retry rules.
//!
//! Clones of a `Renewed` share one cache, one renewal and one configuration: a setter
//! called on any clone changes every clone, so configure it fully, then clone. The origin of a request is checked before any renewal. If a call waiting for
//! the renewal is dropped, the renewal goes on for the other waiting calls; when the last
//! one is dropped, the renewal stops and the next call starts another. A failed renewal
//! also lets the next call try again. No task is started.
//!
//! The author supplies the token endpoint through a client without this middleware.
//! This example also enables the features `json` and `form`. Concord implements no OAuth
//! flow itself. With the separate feature `rotating`, `Rotating` coordinates an exchange
//! of tokens and their durable storage, through a renewal future that the application
//! polls. In both cases the application supplies the token protocol and the store.
//!
//! ```no_run
//! use concord::auth::{AccessToken, Renewed, after_401};
//! use concord::{Base, Json, Retry, Secret, Text};
//! use serde::Deserialize;
//! use std::time::Duration;
//!
//! #[derive(Deserialize)]
//! struct TokenReply { access_token: String, expires_in: u64 }
//! concord::api! {
//!     client Tokens(key: Secret) {
//!         base "https://identity.example.test";
//!         headers { "authorization" = key; }
//!         POST issue() -> TokenReply {
//!             path "token";
//!             body concord::body::form(&[("grant_type", "client_credentials")]);
//!             read Json;
//!         }
//!     }
//! }
//! concord::api! {
//!     client Documents {
//!         base "https://documents.example.test/v2";
//!         GET title() -> String { path "title"; read Text; }
//!     }
//! }
//! async fn title(application_key: Secret) -> Result<String, Box<dyn std::error::Error>> {
//!     let tokens = Tokens::new(application_key).with_timeout(Duration::from_secs(5));
//!     let base: Base = "https://documents.example.test/v2".parse()?;
//!     let renewed = Renewed::new(&base, move || {
//!         let tokens = tokens.clone();
//!         async move {
//!             let reply = tokens.issue().await?;
//!             AccessToken::expires_in(reply.access_token, Duration::from_secs(reply.expires_in))
//!         }
//!     }).early_by(Duration::from_secs(20)).renewal_timeout(Duration::from_secs(6));
//!     let documents = Documents::new().with_middleware(renewed)
//!         .with_retry(Retry::standard().when(after_401));
//!     Ok(documents.title().await?)
//! }
//! ```

use concord_core::http::header::{HeaderName, HeaderValue, InvalidHeaderValue, AUTHORIZATION};
use concord_core::reqwest::{Request, Response};
use concord_core::retry::Tried;
use concord_core::transport::{TransportError, TryInfo};
use concord_core::{Base, Error, Middleware, Next, Secret};
use futures_util::future::{BoxFuture, Shared, WeakShared};
use futures_util::FutureExt;
use std::error::Error as StdError;
use std::fmt;
use std::future::{poll_fn, Future};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard};
use std::task::{Poll, Waker};
use std::time::Duration;
use tokio::time::Instant;

mod rejection;
#[cfg(feature = "rotating")]
#[cfg_attr(docsrs, doc(cfg(feature = "rotating")))]
mod rotating;
pub use rejection::Seen;
#[cfg(feature = "rotating")]
#[cfg_attr(docsrs, doc(cfg(feature = "rotating")))]
#[doc(inline)]
pub use rotating::{
    CredentialStore, Exchange, Rotating, RotationDriver, RotationError, Stored, TokenPair,
};

/// An access token with its expiry.
///
/// The expiry is private, and a copy of the token is taken when it is installed in a
/// cache. `Debug` hides the value and the lifetime. The remaining lifetime follows the
/// same internal timer as renewal, including virtual time. Saving the expiry as
/// wall-clock time is the author's choice: `SystemTime::now().checked_add(token.remaining())`.
///
/// Clones share the value, a `Secret`: [`Secret::replace`] on the [`value`](Self::value)
/// of one clone changes every clone. The expiry is copied.
///
/// ```no_run
/// let token = concord::auth::AccessToken::expires_in("token", std::time::Duration::from_secs(300))?;
/// let saved_value = token.value().clone();
/// let saved_expiry = std::time::SystemTime::now().checked_add(token.remaining());
/// # Ok::<(), concord::auth::RenewalError>(())
/// ```
#[derive(Clone)]
pub struct AccessToken {
    value: Secret,
    expires_at: Instant,
}

impl AccessToken {
    /// Creates a token that expires `lifetime` from now.
    ///
    /// # Errors
    ///
    /// Returns `Expired` if `lifetime` is zero, and `TimeOverflow` if the expiry cannot
    /// be represented.
    ///
    /// ```no_run
    /// let token = concord::auth::AccessToken::expires_in("token", std::time::Duration::from_secs(300))?;
    /// # Ok::<(), concord::auth::RenewalError>(())
    /// ```
    #[must_use = "use the returned token or handle the error"]
    pub fn expires_in(value: impl Into<Secret>, lifetime: Duration) -> Result<Self, RenewalError> {
        if lifetime.is_zero() {
            return Err(RenewalError::Expired);
        }
        let expires_at = Instant::now()
            .checked_add(lifetime)
            .ok_or(RenewalError::TimeOverflow)?;
        Ok(Self {
            value: value.into(),
            expires_at,
        })
    }

    /// Returns the secret value, without a scheme such as `Bearer`.
    ///
    /// ```no_run
    /// fn save(token: &concord::auth::AccessToken) -> concord::Secret { token.value().clone() }
    /// ```
    pub fn value(&self) -> &Secret {
        &self.value
    }

    /// Returns the remaining lifetime, or zero once the token has expired.
    ///
    /// ```no_run
    /// fn expired(token: &concord::auth::AccessToken) -> bool { token.remaining().is_zero() }
    /// ```
    pub fn remaining(&self) -> Duration {
        self.expires_at.saturating_duration_since(Instant::now())
    }
}

impl fmt::Debug for AccessToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AccessToken(..)")
    }
}

/// Why a token could not be renewed or added to a request; `source()` keeps the cause.
///
/// `Failed` and `NotSent` share the author's error with every waiting call. `InvalidHeader` and
/// `TimedOut` keep their causes too; the errors of the settings and of expiry have no
/// lower-level cause. A panic while calling or polling the renewal, formatting the
/// header or running `on_renewed` becomes `Panicked` for every waiting call. The panic
/// payload is discarded; the author's panic hook still runs. With panic=abort, a panic
/// aborts the process. After a panic, the next call can start a new renewal.
///
/// Concord's own errors never print a secret; what the author puts in an error of
/// their own is their responsibility. The `Display` and `Debug` of this type omit
/// causes and panic payloads, but `source()` exposes the kept cause on purpose.
///
/// A call to the API returns this error directly as the source of its transport error:
///
/// ```no_run
/// use concord::auth::RenewalError;
/// use std::error::Error as _;
/// fn inspect<P: 'static>(err: &concord::Error<P>) {
///     let renewal = RenewalError::find(err);
///     if let Some(renewal) = renewal {
///         let token_endpoint = renewal.source().and_then(|s| s.downcast_ref::<concord::Error<()>>());
///     }
/// }
/// ```
#[non_exhaustive]
#[derive(Clone)]
pub enum RenewalError {
    /// The renewal, or the installation of the token, failed; the shared cause is kept.
    Failed(Arc<dyn StdError + Send + Sync>),
    /// The shared renewal took longer than `renewal_timeout`; the timer's error is kept.
    TimedOut(Arc<dyn StdError + Send + Sync>),
    /// The author's code panicked; the panic payload, which may hold values, is discarded.
    Panicked,
    /// The token could not be formatted as the chosen header.
    InvalidHeader(Arc<InvalidHeaderValue>),
    /// The initial token has expired, or a renewed token does not outlast `early_by`.
    Expired,
    /// The request's origin differs from the origin of the credential's base.
    WrongOrigin,
    /// The renewal timeout is zero or the configuration is inconsistent.
    InvalidPolicy,
    /// A deadline cannot be represented.
    TimeOverflow,
    /// The renewal failed before its request left: the token it would have sent is still
    /// unused. Only the author's code says so, with [`not_sent`](Self::not_sent) or
    /// [`from_call`](Self::from_call); the shared cause is kept.
    NotSent(Arc<dyn StdError + Send + Sync>),
}

impl RenewalError {
    /// Finds a `RenewalError` in this error or its chain of sources.
    ///
    /// Returns `None` when there is none, also when the chain is cyclic.
    ///
    /// ```no_run
    /// use concord::auth::{RenewalError, Renewed};
    /// use std::error::Error as _;
    /// concord::api! {
    ///     client Documents {
    ///         base "https://documents.example.test";
    ///         GET title() -> String { path "title"; read concord::Text; }
    ///     }
    /// }
    /// async fn title(auth: Renewed) -> Result<String, concord::Error> {
    ///     let result = Documents::new().with_middleware(auth).title().await;
    ///     if let Err(error) = &result {
    ///         if let Some(renewal) = RenewalError::find(error) {
    ///             let cause = renewal.source();
    ///         }
    ///     }
    ///     result
    /// }
    /// ```
    pub fn find<'a>(error: &'a (dyn StdError + 'static)) -> Option<&'a Self> {
        let mut next = Some(error);
        let mut fast = Some(error);
        while let Some(error) = next {
            if let Some(renewal) = error.downcast_ref::<Self>() {
                return Some(renewal);
            }
            next = error.source();
            for _ in 0..2 {
                fast = fast.and_then(|error| error.source());
                // A renewal error can itself belong to the cycle.
                if let Some(renewal) = fast.and_then(|error| error.downcast_ref::<Self>()) {
                    return Some(renewal);
                }
            }
            if let (Some(next), Some(fast)) = (next, fast) {
                if std::ptr::eq(next, fast) {
                    return None;
                }
            }
        }
        None
    }

    /// Wraps an error of the author's as `Failed`: a shared cause, never printed.
    ///
    /// ```no_run
    /// let error = concord::auth::RenewalError::failed(std::io::Error::other("renewal failed"));
    /// ```
    pub fn failed(cause: impl StdError + Send + Sync + 'static) -> Self {
        Self::Failed(Arc::new(cause))
    }

    /// Wraps an error of the author's as `NotSent`: the renewal failed, and no byte of
    /// the request that carries the refresh token left the process. The cause is shared
    /// and never printed.
    ///
    /// `Rotating` then saves the pair it had before the exchange again, without a usable
    /// expiry, and the next call exchanges the same refresh token; no owner sends that
    /// access token again. `Renewed` treats `NotSent` as `Failed`.
    ///
    /// **When the exchange is a Concord call,** do not classify its error yourself: use
    /// [`from_call`](Self::from_call), which gives `NotSent` exactly when the call made one
    /// attempt and Concord states that nothing of it was sent, a connect timeout of
    /// `concord::Http` included.
    ///
    /// **When you classify an error yourself,** use `not_sent` only when you can establish
    /// that no byte of the request that carries the refresh token was written, and that
    /// none can still be written: a check of your own that fails before the call to the
    /// token endpoint, or a refusal that you know happened before the request was handed
    /// to the network. Otherwise use [`failed`](Self::failed): a timeout whose moment you do
    /// not know, a connection that failed or broke when you cannot tell whether the request
    /// was written, a failure while the body of the request or of the response was sent or
    /// read, and any response, of any status. The server may have consumed the refresh
    /// token in each of these cases, and a provider that sees a consumed refresh token used
    /// again usually revokes every token of the account. When in doubt, use `failed`: the
    /// account then requires a new authorization, which is the safe side.
    ///
    /// For an exchange made with `reqwest` directly: with native `reqwest` 0.13,
    /// `e.is_connect() && !e.is_timeout()` is enough to say `not_sent` for one buffered
    /// request through `reqwest`'s own connector, with no connection layer of your own,
    /// no redirect followed, no retry (`reqwest::retry::never()`) and no TLS early data:
    /// a connection that could not be made fails before the request is written. A form
    /// body needs the `form` feature of `reqwest`. A Concord call's `Connect` class proves
    /// nothing of the kind.
    ///
    /// ```no_run
    /// use concord::auth::{Exchange, RenewalError, TokenPair};
    /// async fn exchange(exchange: Exchange, offline: bool) -> Result<TokenPair, RenewalError> {
    ///     if offline {
    ///         // Checked before the call: the refresh token was never sent.
    ///         return Err(RenewalError::not_sent(std::io::Error::other("offline")));
    ///     }
    ///     # let _ = exchange;
    ///     Err(RenewalError::failed(std::io::Error::other("token endpoint required")))
    /// }
    /// ```
    pub fn not_sent(cause: impl StdError + Send + Sync + 'static) -> Self {
        Self::NotSent(Arc::new(cause))
    }

    /// Wraps the error of the one call to the token endpoint, the call that carries the
    /// refresh token: `NotSent` when Concord states that nothing of it was sent, and
    /// `Failed` otherwise.
    ///
    /// It is `NotSent` when the call failed on its first and only attempt
    /// (`error.tries()` is 1) and [`error.request_not_sent()`](Error::request_not_sent) is
    /// `true`, which covers two cases:
    ///
    /// - the request was blocked before it was sent (`error.class()` is
    ///   `Some(Class::Blocked)`), as by a `concord::limit::Limit` that refuses to wait, or a
    ///   middleware layer or transport that refuses the request; a middleware layer or a
    ///   transport returns `blocked()` only when no request left during its call;
    /// - a failure that states that nothing was written: `concord::Http`, the default
    ///   transport, states it for a failure that it recognizes as one of its connector's
    ///   (a refused connection, a host name that was not resolved, a failed TLS handshake
    ///   or proxy, its own connect timeout); a transport or middleware of your own states it
    ///   with `TransportError::not_sent`, also after it opened a connection when no byte of
    ///   the request was written or can still be written.
    ///
    /// Every other error is `Failed`: with `Http`, a connection that failed after it was
    /// made, even one reset before the server read anything, because the request may
    /// already have been written; a `Connect` error from a `reqwest::Client` used as the transport; a timeout
    /// of the call or of the response; a response, of any status; an error after more than
    /// one attempt, or on a page after the first; and an attempt during which the chain also
    /// sent the request elsewhere, or dropped a send before it returned. A Build error is
    /// `Failed` too, because a request that was sent once can fail to be built again for a
    /// retry.
    ///
    /// Use it in place of `?` on that call, not on a call made after it:
    ///
    /// ```no_run
    /// # use concord::auth::{Exchange, RenewalError, TokenPair};
    /// # use concord::Secret;
    /// # #[derive(serde::Deserialize)] struct Reply { access_token: String, refresh_token: String, expires_in: u64 }
    /// concord::api! {
    ///     client Tokens {
    ///         base "https://identity.example.test";
    ///         POST refresh(refresh_token: Secret) -> Reply {
    ///             path "token";
    ///             body concord::form { "grant_type" = "refresh_token"; refresh_token; }
    ///             read concord::Json;
    ///         }
    ///     }
    /// }
    /// async fn exchange(tokens: Tokens, exchange: Exchange) -> Result<TokenPair, RenewalError> {
    ///     let reply = tokens.refresh(exchange.refresh()).await.map_err(RenewalError::from_call)?;
    ///     Ok(TokenPair::expires_in(reply.access_token, reply.refresh_token,
    ///         std::time::Duration::from_secs(reply.expires_in)))
    /// }
    /// ```
    pub fn from_call<P: Send + Sync + 'static>(error: Error<P>) -> Self {
        if error.tries() == 1 && error.request_not_sent() {
            Self::not_sent(error)
        } else {
            Self::failed(error)
        }
    }
}

impl<P: Send + Sync + 'static> From<Error<P>> for RenewalError {
    fn from(error: Error<P>) -> Self {
        Self::failed(error)
    }
}

impl fmt::Debug for RenewalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Failed(_) => "RenewalError::Failed",
            Self::TimedOut(_) => "RenewalError::TimedOut",
            Self::Panicked => "RenewalError::Panicked",
            Self::InvalidHeader(_) => "RenewalError::InvalidHeader",
            Self::Expired => "RenewalError::Expired",
            Self::WrongOrigin => "RenewalError::WrongOrigin",
            Self::InvalidPolicy => "RenewalError::InvalidPolicy",
            Self::TimeOverflow => "RenewalError::TimeOverflow",
            Self::NotSent(_) => "RenewalError::NotSent",
        })
    }
}

impl fmt::Display for RenewalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Failed(_) => "the credential could not be renewed or installed",
            Self::TimedOut(_) => "the credential renewal exceeded renewal_timeout",
            Self::Panicked => "a credential callback panicked",
            Self::InvalidHeader(_) => "the credential could not be formatted as a header",
            Self::Expired => "the credential has expired or does not outlast early_by",
            Self::WrongOrigin => "the request leaves the credential's origin",
            Self::InvalidPolicy => "the credential renewal policy is invalid",
            Self::TimeOverflow => "the credential deadline cannot be represented",
            Self::NotSent(_) => "the credential could not be renewed, and its request was not sent",
        })
    }
}

impl StdError for RenewalError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Failed(cause) | Self::TimedOut(cause) | Self::NotSent(cause) => {
                Some(cause.as_ref())
            }
            Self::InvalidHeader(cause) => Some(cause.as_ref()),
            _ => None,
        }
    }
}

/// A middleware layer that adds an access token to each request and renews it when it expires.
///
/// It serves one account. The scheme, host and effective port of each request must
/// match those of the base; a path prefix in the base is accepted and neither narrows
/// nor widens that rule. The cache keeps one current token and a weak handle on the
/// renewal in progress. The size of tokens, and what the author's functions capture,
/// are under the author's control. Replacing or discarding the current token leaves no
/// older token in the cache. The renewal must use a client without this middleware: a
/// renewal that directly waits for itself fails with `InvalidPolicy` instead of waiting
/// on its own future.
///
/// Clones share one cache, one renewal and one configuration: a setter called on any
/// clone, `with_token` included, changes every clone. Configure it fully, then clone.
///
/// ```no_run
/// use concord::auth::{AccessToken, Renewed};
/// let base: concord::Base = "https://api.example.test/v2".parse()?;
/// let auth = Renewed::new(&base, || async {
///     AccessToken::expires_in("token", std::time::Duration::from_secs(300))
/// });
/// let shared = auth.clone();
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone)]
#[must_use = "configure the credential and attach it as middleware"]
pub struct Renewed {
    inner: Arc<Inner>,
}

impl Renewed {
    /// A credential for the origin of `base`, renewed by the function `renew`.
    ///
    /// Defaults: no initial token, no early margin, a renewal timeout of 30 seconds, no
    /// `on_renewed` function, and an `Authorization: Bearer` header. A panic while calling
    /// or polling `renew` fails every waiting call with `RenewalError::Panicked`, discards
    /// the payload, and lets the next call renew again.
    ///
    /// ```no_run
    /// # use concord::auth::{Renewed, AccessToken};
    /// let base: concord::Base = "https://api.example.test".parse()?;
    /// let auth = Renewed::new(&base, || async {
    ///     AccessToken::expires_in("token", std::time::Duration::from_secs(300))
    /// });
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use = "attach the credential with with_middleware"]
    pub fn new<F, Fut>(base: &Base, renew: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<AccessToken, RenewalError>> + Send + 'static,
    {
        Self {
            inner: Arc::new(Inner {
                // Base has already validated the URL; retain a defensive failure path.
                origin: concord_core::reqwest::Url::parse(&base.to_string()).ok(),
                state: Mutex::new(State {
                    policy: Policy {
                        margin: Duration::ZERO,
                        timeout: Duration::from_secs(30),
                        header_name: AUTHORIZATION,
                        format: Arc::new(|value| {
                            HeaderValue::from_str(&format!("Bearer {}", &*value.expose()))
                        }),
                        notify: None,
                        rejects: None,
                    },
                    renew: Arc::new(move || renew().boxed()),
                    current: None,
                    flight: None,
                    polling: None,
                }),
            }),
        }
    }

    /// Renews the token when its remaining lifetime is at most `margin`: zero by default.
    ///
    /// An initial token (from `with_token`) within the margin is accepted and renewed on
    /// the first call. A token returned by renewal must outlast the margin, or the
    /// renewal fails with `RenewalError::Expired`.
    ///
    /// ```no_run
    /// fn configure(auth: concord::auth::Renewed) -> concord::auth::Renewed {
    ///     auth.early_by(std::time::Duration::from_secs(20))
    /// }
    /// ```
    #[must_use = "attach the credential with with_middleware"]
    pub fn early_by(self, margin: Duration) -> Self {
        self.inner.lock().policy.margin = margin;
        self
    }

    /// Sets the longest time the shared renewal may take while calls wait for it: 30 seconds by default.
    ///
    /// Zero fails with `InvalidPolicy` before any renewal.
    ///
    /// ```no_run
    /// fn configure(auth: concord::auth::Renewed) -> concord::auth::Renewed {
    ///     auth.renewal_timeout(std::time::Duration::from_secs(5))
    /// }
    /// ```
    #[must_use = "attach the credential with with_middleware"]
    pub fn renewal_timeout(self, timeout: Duration) -> Self {
        self.inner.lock().policy.timeout = timeout;
        self
    }

    /// Sets an initial token, without calling `on_renewed`; a copy of it is taken.
    ///
    /// A token within the early margin is accepted, whatever the order of the setters,
    /// and the first call renews it. The header is formatted and the settings are checked
    /// when the token is used.
    ///
    /// # Errors
    ///
    /// Returns `RenewalError::Expired` if the token has already expired; that is the only error.
    ///
    /// ```no_run
    /// fn seed(auth: concord::auth::Renewed, token: concord::auth::AccessToken)
    ///     -> Result<concord::auth::Renewed, concord::auth::RenewalError> { auth.with_token(token) }
    /// ```
    #[must_use = "use the returned credential or handle the error"]
    pub fn with_token(self, first: AccessToken) -> Result<Self, RenewalError> {
        if first.remaining().is_zero() {
            return Err(RenewalError::Expired);
        }
        let seeded = Cached::Seeded {
            token: first.snapshot(),
            generation: Arc::new(()),
        };
        let retired = self.inner.lock().current.replace(seeded);
        drop(retired);
        Ok(self)
    }

    /// Sets a function called once for each renewed token, after it is checked and before
    /// calls can use it.
    ///
    /// The function must return promptly; saving the token durably is up to the author.
    /// A panic fails the waiting calls with `RenewalError::Panicked` and the token is
    /// not used; the next call can renew again.
    ///
    /// ```no_run
    /// fn configure(auth: concord::auth::Renewed) -> concord::auth::Renewed {
    ///     auth.on_renewed(|token| { let expiry = std::time::SystemTime::now().checked_add(token.remaining()); })
    /// }
    /// ```
    #[must_use = "attach the credential with with_middleware"]
    pub fn on_renewed(self, notify: impl Fn(&AccessToken) + Send + Sync + 'static) -> Self {
        let notify: Arc<Notify> = Arc::new(notify);
        let retired = self.inner.lock().policy.notify.replace(notify);
        drop(retired);
        self
    }

    /// Sets the header that carries the token, and its whole value.
    ///
    /// By default it is `Authorization: Bearer <token>`. The header value is always
    /// marked sensitive. If formatting fails, the standard error is kept as the source
    /// of the renewal error.
    ///
    /// ```no_run
    /// fn configure(auth: concord::auth::Renewed) -> concord::auth::Renewed {
    ///     auth.header(concord::http::HeaderName::from_static("x-api-key"), |token| {
    ///         concord::http::HeaderValue::from_str(&token.expose())
    ///     })
    /// }
    /// ```
    #[must_use = "attach the credential with with_middleware"]
    pub fn header(
        self,
        name: HeaderName,
        format: impl Fn(&Secret) -> Result<HeaderValue, InvalidHeaderValue> + Send + Sync + 'static,
    ) -> Self {
        let format: Arc<Format> = Arc::new(format);
        let retired = {
            let mut state = self.inner.lock();
            state.policy.header_name = name;
            std::mem::replace(&mut state.policy.format, format)
        };
        drop(retired);
        self
    }

    /// Sets a function that recognises a response that rejects the token although its
    /// status is not `401`: an API that reports a revoked or expired token inside a `200`.
    ///
    /// The function receives the status, the headers and the whole body of a response, as
    /// a [`Seen`], once the body has been read to its end, and only when the response's
    /// final URL, as the transport reports it, is at the origin of the credential's base,
    /// and the body is complete and holds at most 64 KiB. A body that ends with an error is
    /// never complete. One that ends cleanly is complete by this rule. A response that
    /// never carries a body (a `1xx`, `204`, `205` or `304`, a response to a `HEAD` this
    /// layer sent, a successful response to a `CONNECT` this layer sent) is complete when
    /// it gave no byte, whatever length it announces, and the function then sees its
    /// headers; it is never shown if it gave a byte. Any other response is complete when
    /// it gave exactly the length that the reader expects (the `Content-Length`, or else
    /// the exact size hint), or, with neither, all it had: no byte against a positive
    /// length announced is a cut body. When it returns `true`,
    /// the token that the request carried is rejected as after a `401`: only if it is
    /// still the current token, so an answer that arrives after a renewal never discards
    /// the newer token. Nothing is renewed at once: each later attempt that needs the
    /// token, of any call, joins the renewal in flight or starts one, and the retry
    /// setting of each call bounds its attempts. The response is returned
    /// unchanged. `rejected_when` itself schedules no retry, and `after_401` retries a `401`
    /// only; a retry rule of the call that applies to the response can send the call again,
    /// and that attempt then carries the renewed token. A `401` always rejects the token,
    /// and the function is not called for it. A response from another origin, `401`
    /// included, rejects nothing.
    ///
    /// When the function runs depends on how the body is read. For a value or a problem
    /// that the reader decodes, as the body ends, before it is decoded, so before the call
    /// returns. For a stream of records (`JsonLines`, `Sse`), when the body signals its
    /// end, which can come before the reader has yielded its last records, or only on a
    /// later poll after the last bytes: dropping the stream before the body has signalled
    /// its end skips the function. For `send_raw()`, when the caller
    /// has read the body to its end; never if it does not. A call given to `transfer::at`
    /// is read like any call. A body that is not read to its end is never shown: a dropped
    /// call, a reader that stops early, an endpoint that does not read its body.
    ///
    /// The response is attributed to the token that this credential placed. A layer that
    /// runs after it and removes or replaces the `Authorization` header owns that change.
    ///
    /// Watching costs something, also when the function never matches: every response
    /// through the credential from its origin is observed, and up to 64 KiB of its body is
    /// copied into one buffer, dropped once the function has seen it, or as soon as the body
    /// is longer. Frames, trailers, the size hint and the end of the body pass through
    /// unchanged. Without this function, a response is not touched.
    ///
    /// The function must return promptly. A panic in it rejects nothing, and its payload is
    /// dropped, but Rust's panic hook still prints the panic message: never put text of the
    /// body in a panic message. Nothing bounds the renewals across calls when a server
    /// rejects every token; each renewal is bounded by `renewal_timeout`. Configuring a
    /// clone changes every clone.
    ///
    /// ```no_run
    /// #[derive(serde::Deserialize)]
    /// struct Envelope { ok: bool, error: Option<String> }
    /// fn configure(auth: concord::auth::Renewed) -> concord::auth::Renewed {
    ///     auth.rejected_when(|seen| {
    ///         seen.status() == 200
    ///             && serde_json::from_slice::<Envelope>(seen.body()).is_ok_and(|reply| {
    ///                 !reply.ok && matches!(reply.error.as_deref(), Some("token_expired" | "invalid_auth"))
    ///             })
    ///     })
    /// }
    /// ```
    #[must_use = "attach the credential with with_middleware"]
    pub fn rejected_when(
        self,
        rejects: impl Fn(&Seen<'_>) -> bool + Send + Sync + 'static,
    ) -> Self {
        let rejects: Arc<rejection::Rejects> = Arc::new(rejects);
        let retired = self.inner.lock().policy.rejects.replace(rejects);
        drop(retired);
        self
    }
}

impl fmt::Debug for Renewed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Renewed(..)")
    }
}

impl Middleware for Renewed {
    async fn handle(
        &self,
        mut request: Request,
        info: TryInfo,
        next: Next<'_>,
    ) -> Result<Response, TransportError> {
        if !self
            .inner
            .origin
            .as_ref()
            .is_some_and(|base| base.origin() == request.url().origin())
        {
            return Err(blocked(RenewalError::WrongOrigin));
        }
        let Version {
            header_name,
            header,
            generation,
            ..
        } = self.token().await.map_err(blocked)?;
        request
            .headers_mut()
            .try_insert(header_name, header)
            .map_err(|cause| blocked(RenewalError::failed(cause)))?;
        let method = request.method().clone();
        let response = next.send(request, info).await?;
        // A response from another origin, as a transport that followed a redirect
        // reports it, rejects nothing.
        if !rejection::same_origin(self.inner.origin.as_ref(), &response) {
            return Ok(response);
        }
        if response.status() == 401 {
            self.inner.discard(&generation);
            return Ok(response);
        }
        let rejects = self.inner.lock().policy.rejects.clone();
        Ok(match rejects {
            Some(rejects) => {
                let inner = Arc::downgrade(&self.inner);
                let generation = Arc::downgrade(&generation);
                rejection::watch(response, &method, rejects, move || {
                    if let (Some(inner), Some(generation)) = (inner.upgrade(), generation.upgrade())
                    {
                        inner.discard(&generation);
                    }
                })
            }
            None => response,
        })
    }
}

/// A retry rule to add to a policy: it retries a `401` received on the first attempt, and nothing else.
///
/// The policy's number of attempts, its checks that a request is safe to repeat and its
/// random wait still apply. This rule sends nothing itself and does not mark a POST as
/// safe to repeat.
///
/// ```no_run
/// use concord::{Retry, auth::after_401};
/// let normal = Retry::<()>::standard().when(after_401);
/// let recovery_only = Retry::<()>::none().tries(2).when(after_401);
/// // Only when this API promises that 401 means the operation was not applied:
/// let post_recovery = recovery_only.not_applied_on(401);
/// ```
pub fn after_401<P>(tried: &Tried<'_, P>) -> Option<Duration> {
    (tried.number() == 1 && tried.status().is_some_and(|status| status == 401))
        .then_some(Duration::ZERO)
}

// Retain one generation and one weak flight handle, with no call-indexed map.
// Only waiters strongly own the shared future.
type Format = dyn Fn(&Secret) -> Result<HeaderValue, InvalidHeaderValue> + Send + Sync;
type Notify = dyn Fn(&AccessToken) + Send + Sync;
type Renew = dyn Fn() -> BoxFuture<'static, Result<AccessToken, RenewalError>> + Send + Sync;
type FlightFuture = BoxFuture<'static, Result<Version, RenewalError>>;

#[derive(Clone)]
struct Policy {
    margin: Duration,
    timeout: Duration,
    header_name: HeaderName,
    format: Arc<Format>,
    notify: Option<Arc<Notify>>,
    rejects: Option<Arc<rejection::Rejects>>,
}

#[derive(Clone)]
struct Version {
    token: AccessToken,
    header_name: HeaderName,
    header: HeaderValue,
    format: Arc<Format>,
    // Identity avoids a wrapping generation counter and stale-401 collisions.
    generation: Arc<()>,
}

enum Cached {
    Seeded {
        token: AccessToken,
        generation: Arc<()>,
    },
    Ready(Version),
}

impl Cached {
    fn token(&self) -> &AccessToken {
        match self {
            Self::Seeded { token, .. } => token,
            Self::Ready(version) => &version.token,
        }
    }

    fn generation(&self) -> &Arc<()> {
        match self {
            Self::Seeded { generation, .. } => generation,
            Self::Ready(version) => &version.generation,
        }
    }
}

struct State {
    policy: Policy,
    renew: Arc<Renew>,
    current: Option<Cached>,
    flight: Option<WeakShared<FlightFuture>>,
    // Detect a direct self-wait before Shared registers its own driver as a waiter.
    polling: Option<Waker>,
}

struct Inner {
    origin: Option<concord_core::reqwest::Url>,
    state: Mutex<State>,
}

impl Inner {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    // Discard the cached token only if it is still the one that the request carried.
    fn discard(&self, generation: &Arc<()>) {
        let retired = {
            let mut state = self.lock();
            if state
                .current
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current.generation(), generation))
            {
                state.current.take()
            } else {
                None
            }
        };
        drop(retired);
    }
}

impl AccessToken {
    fn snapshot(&self) -> Self {
        Self {
            value: Secret::new(self.value.expose().to_string()),
            expires_at: self.expires_at,
        }
    }
}

fn blocked(error: RenewalError) -> TransportError {
    TransportError::blocked().with_source(error)
}

fn prepare(
    policy: &Policy,
    token: AccessToken,
    generation: Arc<()>,
) -> Result<Version, RenewalError> {
    catch_unwind(AssertUnwindSafe(|| {
        if token.remaining() <= policy.margin {
            return Err(RenewalError::Expired);
        }
        let token = token.snapshot();
        // The formatter may retain a Secret clone; it cannot mutate the cache.
        let mut header = (policy.format)(&token.snapshot().value)
            .map_err(|cause| RenewalError::InvalidHeader(Arc::new(cause)))?;
        header.set_sensitive(true);
        if token.remaining() <= policy.margin {
            return Err(RenewalError::Expired);
        }
        Ok(Version {
            token,
            header_name: policy.header_name.clone(),
            header,
            format: policy.format.clone(),
            generation,
        })
    }))
    .unwrap_or(Err(RenewalError::Panicked))
}

impl Renewed {
    async fn token(&self) -> Result<Version, RenewalError> {
        loop {
            let (policy, cached, flight, retired) = {
                let mut state = self.inner.lock();
                let policy = state.policy.clone();
                if policy.timeout.is_zero() {
                    return Err(RenewalError::InvalidPolicy);
                }
                let retired = if state
                    .current
                    .as_ref()
                    .is_some_and(|current| current.token().remaining() <= policy.margin)
                {
                    state.current.take()
                } else {
                    None
                };
                if let Some(current) = &state.current {
                    if let Cached::Ready(version) = current {
                        if Arc::ptr_eq(&version.format, &policy.format) {
                            return Ok(version.clone());
                        }
                    }
                    let cached = (current.token().clone(), current.generation().clone());
                    (policy, Some(cached), None, retired)
                } else {
                    let flight = state
                        .flight
                        .as_ref()
                        .and_then(WeakShared::upgrade)
                        .unwrap_or_else(|| {
                            let flight = self.start(policy.clone(), state.renew.clone());
                            state.flight = flight.downgrade();
                            flight
                        });
                    (policy, None, Some(flight), retired)
                }
            };
            drop(retired);
            if let Some((token, generation)) = cached {
                // Prepare seeds and revalidate header changes without notifying or renewing.
                let version = prepare(&policy, token, generation)?;
                let retired = {
                    let mut state = self.inner.lock();
                    if state.current.as_ref().is_some_and(|current| {
                        Arc::ptr_eq(current.generation(), &version.generation)
                    }) {
                        state.current.replace(Cached::Ready(version.clone()))
                    } else {
                        None
                    }
                };
                drop(retired);
                // Retirement and local drops can run author code. Reacquire
                // only after those drops, so lifetime and cache state are fresh.
                continue;
            }
            match flight {
                Some(mut flight) => {
                    poll_fn(|cx| {
                        if self
                            .inner
                            .lock()
                            .polling
                            .as_ref()
                            .is_some_and(|driver| driver.will_wake(cx.waker()))
                        {
                            return Poll::Ready(Err(RenewalError::InvalidPolicy));
                        }
                        Pin::new(&mut flight).poll(cx)
                    })
                    .await?;
                }
                None => return Err(RenewalError::InvalidPolicy),
            }
            // A shared result can have waited unpolled past expiry or rejection.
            // Reacquire from the current cache and policy before using the token.
        }
    }

    fn start(&self, policy: Policy, renew: Arc<Renew>) -> Shared<FlightFuture> {
        let inner = self.inner.clone();
        async move {
            let Some(deadline) = Instant::now().checked_add(policy.timeout) else {
                inner.lock().flight = None;
                return Err(RenewalError::TimeOverflow);
            };
            let mut work = Box::pin(
                AssertUnwindSafe(async {
                    let token = renew().await?;
                    let version = prepare(&policy, token, Arc::new(()))?;
                    if let Some(notify) = &policy.notify {
                        let notification = version.token.snapshot();
                        if Instant::now() >= deadline {
                            std::future::pending::<()>().await;
                        }
                        notify(&notification);
                    }
                    // A synchronous notification can consume the remaining lifetime.
                    if version.token.remaining() <= policy.margin {
                        return Err(RenewalError::Expired);
                    }
                    Ok(version)
                })
                .catch_unwind(),
            );
            let work = poll_fn(|cx| {
                // Timeout polls its work first. Leave it pending once the
                // deadline is due so that its timer supplies the elapsed cause.
                if Instant::now() >= deadline {
                    return Poll::Pending;
                }
                let driver = cx.waker().clone();
                let retired = inner.lock().polling.replace(driver);
                drop(retired);
                let result = work.as_mut().poll(cx);
                let retired = inner.lock().polling.take();
                drop(retired);
                if Instant::now() >= deadline {
                    Poll::Pending
                } else {
                    result
                }
            });
            let result = match tokio::time::timeout_at(deadline, work).await {
                Ok(Ok(result)) => result,
                Ok(Err(_)) => Err(RenewalError::Panicked),
                Err(cause) => Err(RenewalError::TimedOut(Arc::new(cause))),
            };
            let (late, retired) = {
                let mut state = inner.lock();
                let late = Instant::now() >= deadline && result.is_ok();
                let retired = if let Ok(version) = &result {
                    if late {
                        None
                    } else {
                        state.current.replace(Cached::Ready(version.clone()))
                    }
                } else {
                    None
                };
                state.flight = None;
                (late, retired)
            };
            drop(retired);
            if late {
                Err(elapsed(deadline).await)
            } else {
                result
            }
        }
        .boxed()
        .shared()
    }
}

async fn elapsed(deadline: Instant) -> RenewalError {
    match tokio::time::timeout_at(deadline, std::future::pending::<std::convert::Infallible>())
        .await
    {
        Err(cause) => RenewalError::TimedOut(Arc::new(cause)),
        Ok(never) => match never {},
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test(start_paused = true)]
    async fn c11_3_renewed_cache_retains_only_the_current_generation() {
        let base: Base = "https://data.test".parse().unwrap();
        let renewals = Arc::new(AtomicUsize::new(0));
        let auth = Renewed::new(&base, {
            let renewals = renewals.clone();
            move || {
                renewals.fetch_add(1, Ordering::SeqCst);
                async { AccessToken::expires_in("renewed-secret", Duration::from_secs(100)) }
            }
        })
        .with_token(AccessToken::expires_in("seed-secret", Duration::from_secs(100)).unwrap())
        .unwrap();
        let seeded = {
            let state = auth.inner.lock();
            assert!(matches!(state.current, Some(Cached::Seeded { .. })));
            Arc::downgrade(state.current.as_ref().unwrap().generation())
        };
        let token = auth.token().await.unwrap();
        assert!(Arc::ptr_eq(&seeded.upgrade().unwrap(), &token.generation));
        drop(token);
        let auth = auth
            .with_token(
                AccessToken::expires_in("replacement-secret", Duration::from_secs(100)).unwrap(),
            )
            .unwrap();
        assert!(seeded.upgrade().is_none());
        let mut previous = {
            let state = auth.inner.lock();
            Arc::downgrade(state.current.as_ref().unwrap().generation())
        };
        for _ in 0..20 {
            tokio::time::sleep(Duration::from_secs(100)).await;
            let token = auth.token().await.unwrap();
            assert!(previous.upgrade().is_none());
            {
                let state = auth.inner.lock();
                let Some(Cached::Ready(current)) = &state.current else {
                    panic!("a completed renewal must be the current token");
                };
                assert!(Arc::ptr_eq(&current.generation, &token.generation));
                assert!(state.flight.is_none());
                assert!(state.polling.is_none());
            }
            previous = Arc::downgrade(&token.generation);
            drop(token);
            assert!(previous.upgrade().is_some());
        }
        assert_eq!(renewals.load(Ordering::SeqCst), 20);
        drop(auth);
        assert!(previous.upgrade().is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn c11_3_renewed_cache_does_not_own_a_dropped_renewal_future() {
        let base: Base = "https://data.test".parse().unwrap();
        let renewals = Arc::new(AtomicUsize::new(0));
        let auth = Renewed::new(&base, {
            let renewals = renewals.clone();
            move || {
                renewals.fetch_add(1, Ordering::SeqCst);
                async {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    AccessToken::expires_in("renewed-secret", Duration::from_secs(100))
                }
            }
        });
        let mut first = Box::pin(auth.token());
        let mut second = Box::pin(auth.token());
        assert!(first.as_mut().now_or_never().is_none());
        assert!(second.as_mut().now_or_never().is_none());
        let flight = auth.inner.lock().flight.clone().unwrap();
        assert_eq!(renewals.load(Ordering::SeqCst), 1);
        drop(first);
        assert!(flight.upgrade().is_some());
        drop(second);
        assert!(flight.upgrade().is_none());
        {
            let state = auth.inner.lock();
            assert!(state.current.is_none());
            assert!(state.flight.as_ref().unwrap().upgrade().is_none());
            assert!(state.polling.is_none());
        }
        let token = auth.token().await.unwrap();
        assert_eq!(renewals.load(Ordering::SeqCst), 2);
        let state = auth.inner.lock();
        assert!(matches!(state.current, Some(Cached::Ready(_))));
        assert!(Arc::ptr_eq(
            state.current.as_ref().unwrap().generation(),
            &token.generation
        ));
        assert!(state.flight.is_none());
        assert!(flight.upgrade().is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn c11_3_retired_entries_drop_after_unlock() {
        use concord_core::transport::Transport;
        use std::sync::{OnceLock, Weak};

        struct Owner {
            inner: Weak<Inner>,
            unlocked: Arc<Mutex<Vec<bool>>>,
        }
        impl AsRef<[u8]> for Owner {
            fn as_ref(&self) -> &[u8] {
                b"credential"
            }
        }
        impl Drop for Owner {
            fn drop(&mut self) {
                let inner = self.inner.upgrade().unwrap();
                let unlocked = inner.state.try_lock().is_ok();
                self.unlocked.lock().unwrap().push(unlocked);
                // Detect the deadlock without hanging the test process. Only
                // reenter when the mutex is free; the broken code records false.
                if unlocked {
                    assert!(Renewed { inner }.token().now_or_never().unwrap().is_ok());
                }
            }
        }
        struct StripHeader(concord_testkit::Script);
        impl Transport for StripHeader {
            async fn send(
                &self,
                mut request: Request,
                info: TryInfo,
            ) -> Result<Response, TransportError> {
                drop(request.headers_mut().remove(AUTHORIZATION));
                self.0.send(request, info).await
            }
        }
        let unlocked = Arc::new(Mutex::new(Vec::new()));
        for mode in 0..4 {
            let holder = Arc::new(OnceLock::<Weak<Inner>>::new());
            let formats = Arc::new(AtomicUsize::new(0));
            let base: Base = "https://data.test".parse().unwrap();
            let auth = Renewed::new(&base, || async {
                AccessToken::expires_in("renewed", Duration::from_secs(100))
            })
            .header(AUTHORIZATION, {
                let holder = holder.clone();
                let formats = formats.clone();
                let unlocked = unlocked.clone();
                move |_| {
                    if formats.fetch_add(1, Ordering::SeqCst) == 0 {
                        HeaderValue::from_maybe_shared(concord_core::bytes::Bytes::from_owner(
                            Owner {
                                inner: holder.get().unwrap().clone(),
                                unlocked: unlocked.clone(),
                            },
                        ))
                    } else {
                        Ok(HeaderValue::from_static("replacement"))
                    }
                }
            })
            .with_token(AccessToken::expires_in("seed", Duration::from_secs(2)).unwrap())
            .unwrap();
            holder.set(Arc::downgrade(&auth.inner)).unwrap();
            drop(auth.token().await.unwrap());
            match mode {
                0 => {
                    let script = concord_testkit::Script::new();
                    script.answer(401);
                    let transport = StripHeader(script.clone());
                    let request = Request::new(
                        concord_core::http::Method::GET,
                        "https://data.test/".parse().unwrap(),
                    );
                    let response = auth
                        .handle(
                            request,
                            TryInfo::new("auth.renewal", 1),
                            Next::to(&transport),
                        )
                        .await
                        .unwrap();
                    assert_eq!(response.status(), 401);
                    let requests = script.requests();
                    assert_eq!(requests.len(), 1);
                    assert_eq!(requests[0].info().number(), 1);
                }
                1 => {
                    tokio::time::advance(Duration::from_secs(3)).await;
                    drop(auth.token().await.unwrap());
                }
                2 => {
                    drop(
                        auth.clone()
                            .with_token(
                                AccessToken::expires_in("replacement", Duration::from_secs(100))
                                    .unwrap(),
                            )
                            .unwrap(),
                    );
                }
                _ => {
                    let auth = auth.clone().header(AUTHORIZATION, |_| {
                        Ok(HeaderValue::from_static("replacement"))
                    });
                    drop(auth.token().await.unwrap());
                }
            }
        }
        assert_eq!(
            *unlocked.lock().unwrap(),
            [true; 4],
            "a retired header owner ran under the state lock"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn c11_3_callback_captures_drop_after_unlock() {
        struct Probe {
            inner: std::sync::Weak<Inner>,
            unlocked: Arc<Mutex<Vec<bool>>>,
        }
        impl Drop for Probe {
            fn drop(&mut self) {
                let inner = self.inner.upgrade().unwrap();
                self.unlocked
                    .lock()
                    .unwrap()
                    .push(inner.state.try_lock().is_ok());
            }
        }
        let base: Base = "https://data.test".parse().unwrap();
        let auth = Renewed::new(&base, || async {
            AccessToken::expires_in("token", Duration::from_secs(100))
        });
        let unlocked = Arc::new(Mutex::new(Vec::new()));
        let probe = Probe {
            inner: Arc::downgrade(&auth.inner),
            unlocked: unlocked.clone(),
        };
        let auth = auth.on_renewed(move |_| {
            let _ = &probe;
        });
        let auth = auth.on_renewed(|_| {});
        let probe = Probe {
            inner: Arc::downgrade(&auth.inner),
            unlocked: unlocked.clone(),
        };
        let auth = auth.header(AUTHORIZATION, move |_| {
            let _ = &probe;
            Ok(HeaderValue::from_static("token"))
        });
        let _auth = auth.header(AUTHORIZATION, |_| Ok(HeaderValue::from_static("token")));
        assert_eq!(
            *unlocked.lock().unwrap(),
            [true; 2],
            "replaced callback captures ran under the state lock"
        );
    }
}

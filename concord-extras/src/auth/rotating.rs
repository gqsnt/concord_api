//! A refresh token that is replaced after every exchange.

use super::RenewalError;
use concord_core::http::header::{HeaderName, HeaderValue, InvalidHeaderValue, AUTHORIZATION};
use concord_core::reqwest::{Request, Response};
use concord_core::transport::{TransportError, TryInfo};
use concord_core::{Base, Middleware, Next, Secret};
use futures_util::FutureExt;
use std::error::Error as StdError;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, SystemTime};
use tokio::time::Instant;

mod engine;

/// An access token and a refresh token, with an optional expiry of the access token.
///
/// A pair cannot be changed. A copy of each secret is taken, even when it is given as a
/// mutable alias of a `Secret`, and the methods return independent secrets. An expiry
/// of `None` means the access token cannot be used; a valid replacement refresh token
/// is still saved before `Expired` is reported.
///
/// A clone is independent: a pair cannot be changed, and its methods return new secrets.
///
/// ```no_run
/// let pair = concord::auth::TokenPair::expires_in("access", "refresh", std::time::Duration::from_secs(300));
/// let saved_expiry = pair.expires_at();
/// ```
#[derive(Clone)]
#[cfg(feature = "rotating")]
#[cfg_attr(docsrs, doc(cfg(feature = "rotating")))]
pub struct TokenPair {
    access: Arc<str>,
    refresh: Arc<str>,
    expiry: Expiry,
}

#[derive(Clone)]
enum Expiry {
    Absolute(Option<SystemTime>),
    Relative {
        wall: Option<SystemTime>,
        deadline: Option<Instant>,
    },
}

impl TokenPair {
    /// Restores a stored pair; with a past or absent expiry, `Rotating` renews the pair
    /// before it uses the access token.
    ///
    /// ```no_run
    /// let pair = concord::auth::TokenPair::new("access", "refresh", None);
    /// ```
    #[must_use]
    pub fn new(
        access: impl Into<Secret>,
        refresh: impl Into<Secret>,
        expires_at: Option<SystemTime>,
    ) -> Self {
        Self::with_expiry(access.into(), refresh.into(), Expiry::Absolute(expires_at))
    }

    /// Creates the result of an exchange: a pair whose access token expires `lifetime` from now.
    ///
    /// It never fails, even on a zero lifetime or a time overflow. The lifetime is
    /// measured on the renewal timer (including virtual time in tests); it starts here
    /// and includes the time spent saving. An expiry that cannot be represented becomes
    /// `None`, and the refresh token can still be saved safely.
    ///
    /// ```no_run
    /// let pair = concord::auth::TokenPair::expires_in("access", "refresh", std::time::Duration::MAX);
    /// assert!(pair.expires_at().is_none());
    /// ```
    #[must_use]
    pub fn expires_in(
        access: impl Into<Secret>,
        refresh: impl Into<Secret>,
        lifetime: Duration,
    ) -> Self {
        let wall = SystemTime::now().checked_add(lifetime);
        let deadline = wall.and_then(|_| Instant::now().checked_add(lifetime));
        let wall = deadline.and(wall);
        Self::with_expiry(
            access.into(),
            refresh.into(),
            Expiry::Relative { wall, deadline },
        )
    }

    fn with_expiry(access: Secret, refresh: Secret, expiry: Expiry) -> Self {
        Self {
            access: Arc::from(&*access.expose()),
            refresh: Arc::from(&*refresh.expose()),
            expiry,
        }
    }

    /// Returns an independent copy of the access token, without a scheme such as `Bearer`.
    ///
    /// ```no_run
    /// # let pair = concord::auth::TokenPair::new("access", "refresh", None);
    /// let access = pair.access();
    /// ```
    pub fn access(&self) -> Secret {
        Secret::new(self.access.to_string())
    }

    /// Returns an independent copy of the refresh token.
    ///
    /// ```no_run
    /// # let pair = concord::auth::TokenPair::new("access", "refresh", None);
    /// let refresh = pair.refresh();
    /// ```
    pub fn refresh(&self) -> Secret {
        Secret::new(self.refresh.to_string())
    }

    /// Returns the wall-clock expiry to save, or `None` when there is none or it cannot be
    /// represented. A past expiry is returned as it is.
    ///
    /// ```no_run
    /// # let pair = concord::auth::TokenPair::new("access", "refresh", None);
    /// let expiry: Option<std::time::SystemTime> = pair.expires_at();
    /// ```
    pub fn expires_at(&self) -> Option<SystemTime> {
        match self.expiry {
            Expiry::Absolute(wall) | Expiry::Relative { wall, .. } => wall,
        }
    }

    // Called with the account lock held when mapping persistent wall time.
    fn deadline(&self) -> Option<Instant> {
        match self.expiry {
            Expiry::Relative { deadline, .. } => deadline,
            Expiry::Absolute(wall) => wall.and_then(|wall| {
                let left = wall.duration_since(SystemTime::now()).unwrap_or_default();
                Instant::now().checked_add(left)
            }),
        }
    }

    fn same_record(&self, other: &Self) -> bool {
        self.access == other.access
            && self.refresh == other.refresh
            && self.expires_at() == other.expires_at()
    }
}

impl fmt::Debug for TokenPair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TokenPair(..)")
    }
}

/// The whole durable state of one account.
///
/// This enum is exhaustive on purpose: every store must save every state. Adding
/// a state requires a major version, so an old store cannot fail unexpectedly
/// just after the server has issued a replacement token.
///
/// ```no_run
/// fn marker(record: concord::auth::Stored) -> bool {
///     match record { concord::auth::Stored::Ready(_) => false, concord::auth::Stored::Refreshing => true }
/// }
/// ```
#[derive(Clone)]
#[cfg(feature = "rotating")]
#[cfg_attr(docsrs, doc(cfg(feature = "rotating")))]
pub enum Stored {
    /// A saved pair, whose access token may already be unusable.
    Ready(TokenPair),
    /// The intent to exchange, saved before the exchange is sent: its outcome may be
    /// unknown, so no known refresh token is safe to send again.
    Refreshing,
}

impl fmt::Debug for Stored {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Ready(_) => "Stored::Ready",
            Self::Refreshing => "Stored::Refreshing",
        })
    }
}

/// Atomic, durable storage for the record of one account, which it alone owns.
///
/// A success means that the record has been replaced durably. After `save` returns,
/// **including with an error**, that operation has no further effect on storage: its
/// outcome may be unknown, but it must be final. Saving the same record again must be
/// safe. A timed-out HTTP PUT does not in general have this property.
///
/// Concord runs the operations one at a time and never applies a timeout to them. A
/// panic in the store stops this instance. After a panic or a cancellation, the
/// application must make sure that no earlier operation is still running before
/// another owner uses the account: no late write may race it. One `Rotating` and its
/// clones own the account; other processes need a lease owned by the application,
/// because this trait provides no locking across processes. The methods must yield
/// promptly, bound their own allocations and errors, and must not wait on a call to
/// the API that needs this credential.
///
/// The [whole example](Rotating) supplies a local store whose saves are final in this way.
#[cfg(feature = "rotating")]
#[cfg_attr(docsrs, doc(cfg(feature = "rotating")))]
pub trait CredentialStore: Send + Sync + 'static {
    /// Loads the last complete record; a storage error is kept as the cause.
    ///
    /// ```no_run
    /// async fn load(store: &impl concord::auth::CredentialStore)
    ///     -> Result<concord::auth::Stored, concord::auth::RenewalError> { store.load().await }
    /// ```
    fn load(&self) -> impl Future<Output = Result<Stored, RenewalError>> + Send;
    /// Replaces the whole record atomically; a success means that it is durable.
    ///
    /// ```no_run
    /// async fn mark(store: &impl concord::auth::CredentialStore)
    ///     -> Result<(), concord::auth::RenewalError> { store.save(concord::auth::Stored::Refreshing).await }
    /// ```
    fn save(&self, record: Stored) -> impl Future<Output = Result<(), RenewalError>> + Send;
}

/// What one exchange receives: its own copy of the refresh token to send.
///
/// More inputs may be added later. Use a separate client for the token endpoint, with no
/// retry policy and a transport that does not resend: `concord::Http`, the default,
/// resends nothing on its own; a `reqwest::Client` of your own needs
/// `.retry(reqwest::retry::never())`. Set a timeout on that client's request yourself: the
/// driver never cancels the exchange.
///
/// ```no_run
/// fn token(exchange: &concord::auth::Exchange) -> concord::Secret { exchange.refresh().clone() }
/// ```
#[cfg(feature = "rotating")]
#[cfg_attr(docsrs, doc(cfg(feature = "rotating")))]
pub struct Exchange {
    refresh: Secret,
}
impl Exchange {
    /// Returns this exchange's copy of the refresh token; replacing it cannot change the
    /// state that the credential keeps.
    ///
    /// ```no_run
    /// fn token(exchange: &concord::auth::Exchange) -> concord::Secret { exchange.refresh().clone() }
    /// ```
    pub fn refresh(&self) -> &Secret {
        &self.refresh
    }
}
impl fmt::Debug for Exchange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Exchange(..)")
    }
}

/// Why a `Rotating` credential could not provide an access token; the cause is kept.
///
/// `Display` prints one sentence for each variant, and `Debug` prints its name;
/// neither prints values, causes or panic payloads. `source()` exposes the kept cause
/// on purpose; the application's own errors remain the application's responsibility.
/// The renewal errors shared with `Renewed` have the same text and cause. A caught
/// panic discards its payload, but the application's panic hook still runs; with
/// panic=abort the process aborts.
///
/// ```no_run
/// fn inspect(error: &concord::Error) {
///     if let Some(rotation) = concord::auth::RotationError::find(error) {
///         let cause = std::error::Error::source(rotation);
///     }
/// }
/// ```
#[non_exhaustive]
#[derive(Clone)]
#[cfg(feature = "rotating")]
#[cfg_attr(docsrs, doc(cfg(feature = "rotating")))]
pub enum RotationError {
    /// Loading failed; the store's error is kept.
    Load(RenewalError),
    /// Saving failed; the final outcome may be unknown, so the record is loaded again.
    Save(RenewalError),
    /// A failure of the exchange, a wait that timed out, a panic, or an error of the
    /// header, expiry, origin, settings or time overflow.
    Renewal(RenewalError),
    /// The refresh token is empty, or an exchange returned the token it consumed.
    InvalidRefreshToken,
    /// No known refresh token is safe to use and no replacement is known: for example the
    /// store holds a `Refreshing` marker or an empty refresh token, or an exchange failed
    /// or returned an invalid one. An exchange that failed with `RenewalError::NotSent`
    /// leads here only if saving the previous pair again failed, or if a panic destroyed
    /// the exchange's future, which the caller then sees as `RenewalError::Panicked`. The
    /// application must
    /// recover the account with the provider.
    NeedsReauthorization,
    /// The driver was not polled while this call waited.
    NotPolled,
    /// The driver was dropped, or a storage panic permanently stopped the account.
    Stopped,
    /// The driver could not save a replacement durably, and returns it so that the
    /// application can save it.
    ///
    /// After a panic in the store, make sure that no earlier store operation is still
    /// running before saving the pair by other means.
    #[non_exhaustive]
    Unsaved {
        /// The exact replacement, even when its access token or expiry is unusable.
        pair: TokenPair,
        /// The failure that ended the last attempt to recover it, kept as this error's
        /// source: from loading, saving or renewal, a panic included.
        cause: RenewalError,
    },
}
impl RotationError {
    /// Finds a `RotationError` in this error or its chain of sources, also when the
    /// chain is a cycle that contains it.
    ///
    /// ```no_run
    /// fn inspect(error: &concord::Error) { let rotation = concord::auth::RotationError::find(error); }
    /// ```
    pub fn find<'a>(error: &'a (dyn StdError + 'static)) -> Option<&'a Self> {
        let mut slow = Some(error);
        let mut fast = Some(error);
        while let Some(error) = slow {
            if let Some(found) = error.downcast_ref::<Self>() {
                return Some(found);
            }
            slow = error.source();
            for _ in 0..2 {
                fast = fast.and_then(StdError::source);
                if let Some(found) = fast.and_then(|error| error.downcast_ref::<Self>()) {
                    return Some(found);
                }
            }
            if let (Some(slow), Some(fast)) = (slow, fast) {
                if std::ptr::eq(slow, fast) {
                    return None;
                }
            }
        }
        None
    }
}
impl fmt::Debug for RotationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Load(_) => "RotationError::Load",
            Self::Save(_) => "RotationError::Save",
            Self::Renewal(_) => "RotationError::Renewal",
            Self::InvalidRefreshToken => "RotationError::InvalidRefreshToken",
            Self::NeedsReauthorization => "RotationError::NeedsReauthorization",
            Self::NotPolled => "RotationError::NotPolled",
            Self::Stopped => "RotationError::Stopped",
            Self::Unsaved { .. } => "RotationError::Unsaved",
        })
    }
}
impl fmt::Display for RotationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Load(_) => "the credential could not be loaded",
            Self::Save(_) => "the credential could not be saved",
            Self::Renewal(_) => "the credential could not be renewed or installed",
            Self::InvalidRefreshToken => "the refresh token is empty or was not replaced",
            Self::NeedsReauthorization => "the account requires reauthorization",
            Self::NotPolled => "the rotation driver was not polled during renewal_timeout",
            Self::Stopped => "the rotation driver has stopped",
            Self::Unsaved { .. } => "the replacement credential must be saved by the application",
        })
    }
}
impl StdError for RotationError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Load(error)
            | Self::Save(error)
            | Self::Renewal(error)
            | Self::Unsaved { cause: error, .. } => Some(error),
            _ => None,
        }
    }
}

/// The future that runs the exchanges and saves of one rotating account.
///
/// The application polls this future or spawns it; Concord starts no task or thread.
/// It is `Send` and `Unpin`, but neither `Sync` nor `Clone`. As long as it is polled,
/// dropping every call to the API, or their timing out, does not cancel a renewal that
/// has begun. A failed exchange yields no replacement; its error goes to the waiting
/// calls. After `RenewalError::NotSent`, the save of the previous pair is part of the
/// renewal, and is not dropped either. A valid replacement is saved, or kept pending after a failed save until the
/// next call that needs a token, or the drop of the last owner, retries the save; a
/// pair that cannot be saved at the end is handed back in [`RotationError::Unsaved`].
/// A renewal begins when saving its `Refreshing` marker begins; dropping the last
/// waiting call during the first load leaves no renewal begun. Store operations never
/// time out: the first load is dropped when its last waiting call is dropped, but a
/// save is never dropped while the driver is polled. The driver never applies a
/// timeout to the exchange either. Dropping the driver cancels its work and stops the
/// credential; no asynchronous work runs in `Drop`.
///
/// When the last owner of the credential is dropped, the driver finishes the work
/// already begun, makes one more attempt at a pending save, then returns success or
/// the failure that remains. If that attempt fails, it returns `Unsaved` with the pair.
/// A panic in the store stops the credential with `Stopped`; a waiting call whose
/// deadline has passed still reports its timeout. The driver returns `Unsaved` if it
/// holds a replacement, and `Stopped` otherwise. Ordinary failures go to the waiting
/// calls and leave the driver ready for the next calls that need a token. A store or
/// an exchange that hangs keeps the driver from finishing; calls to the API still have
/// their own deadlines. After a call starts waiting, the driver must be polled before
/// that call's deadline, or the call receives `NotPolled`, even if the driver is
/// polled later.
///
/// The promise that a pair is saved or returned begins when the author's exchange
/// future yields a pair whose refresh token is not empty and differs from the one sent;
/// any other pair is discarded with `InvalidRefreshToken`. Concord cannot protect a
/// pair that the author's code loses before yielding it. Concord guards the destruction of the author's values once; a
/// panic payload that panics again when destroyed is beyond what it can contain.
/// Concord's own formatting never prints a token. An error of the author's store, and
/// anything printed by walking its sources, remain the author's responsibility.
///
/// Poll it within a Tokio runtime. Finishing a renewal wakes every waiting call, so
/// that work grows with the number of waiting calls. Callbacks and destructors must
/// return promptly. The [whole example](Rotating) runs the driver in a task owned by
/// the application.
///
/// Saving an unsaved pair is up to the application. After a panic in the store, first
/// make sure that earlier operations can have no later effect; the recovery store
/// below must already meet that requirement. If its own save fails, this example
/// returns the original error, so that its caller still has the pair.
///
/// ```no_run
/// use concord::auth::{CredentialStore, RotationDriver, RotationError, Stored};
/// async fn finish(driver: RotationDriver, recovery_store: &impl CredentialStore)
///     -> Result<(), RotationError>
/// {
///     let result = driver.await;
///     if let Err(RotationError::Unsaved { pair, .. }) = &result {
///         // The application saves the exact pair, even if access is unusable.
///         if recovery_store.save(Stored::Ready(pair.clone())).await.is_ok() {
///             return Ok(());
///         }
///     }
///     result
/// }
/// ```
#[must_use = "poll or explicitly spawn the rotation driver alongside resource calls"]
#[cfg(feature = "rotating")]
#[cfg_attr(docsrs, doc(cfg(feature = "rotating")))]
pub struct RotationDriver {
    inner: Option<Arc<engine::Inner>>,
    work: Option<futures_util::future::BoxFuture<'static, Result<(), RotationError>>>,
}
impl Future for RotationDriver {
    type Output = Result<(), RotationError>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let result =
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.poll_work(cx))) {
                Ok(result) => result,
                Err(_) => Poll::Ready(Err(self
                    .inner
                    .as_ref()
                    .and_then(|inner| inner.hand_back(RenewalError::Panicked))
                    .unwrap_or(RotationError::Stopped))),
            };
        match result {
            Poll::Pending => Poll::Pending,
            Poll::Ready(result) => {
                // The output is safe here before any remaining author captures
                // are destroyed. A single cleanup panic cannot lose the pair;
                // a panic payload that itself panics is outside this guarantee.
                let clean = self.cleanup();
                Poll::Ready(
                    if clean || matches!(result, Err(RotationError::Unsaved { .. })) {
                        result
                    } else {
                        Err(RotationError::Renewal(RenewalError::Panicked))
                    },
                )
            }
        }
    }
}
impl RotationDriver {
    fn poll_work(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), RotationError>> {
        let Some(inner) = self.inner.as_ref() else {
            return Poll::Ready(Err(RotationError::Stopped));
        };
        let previous = {
            let mut state = inner.lock();
            let _ = state.progress.first.set(Instant::now());
            std::mem::replace(&mut state.progress, Arc::new(engine::Progress::default()))
        };
        drop(previous);
        let waker = cx.waker().clone();
        let driver_waker = Arc::new(cx.waker().clone());
        let retired = {
            let mut state = inner.lock();
            (
                state.polling.replace(waker),
                state.driver_waker.replace(driver_waker),
            )
        };
        drop(retired);
        let result = match self.work.as_mut() {
            Some(work) => work.as_mut().poll(cx),
            None => Poll::Ready(Err(RotationError::Stopped)),
        };
        let retired = inner.lock().polling.take();
        drop(retired);
        result
    }

    fn cleanup(&mut self) -> bool {
        let work = self.work.take();
        let work_clean =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(work))).is_ok();
        let inner_clean = self.inner.take().is_none_or(|inner| {
            let stopped =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| inner.stop())).is_ok();
            let dropped =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(inner))).is_ok();
            stopped && dropped
        });
        work_clean && inner_clean
    }
}
impl Drop for RotationDriver {
    fn drop(&mut self) {
        self.cleanup();
    }
}
impl fmt::Debug for RotationDriver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RotationDriver(..)")
    }
}

/// A middleware layer that renews the access token with a refresh token that can be used only once.
///
/// Keep the `RotationDriver` that [`Rotating::new`] returns polled, apart from the
/// calls to the API. The origin is checked before the store is accessed: a path prefix
/// in the base neither narrows nor widens the comparison of scheme, host and effective
/// port.
///
/// The durable `Refreshing` marker is saved before the exchange. A replacement is saved
/// before either token is used, even if the access token has no usable expiry or
/// cannot be formatted as a header. If the final save fails, the pair is kept: the next
/// call that needs a token loads the store again and retries only the save, never
/// exchanging the old token again. Even a reload that shows the same pair needs another
/// successful save, because seeing a record is no proof that it is durable. An empty
/// stored refresh token is refused before a marker is saved; after an exchange returns
/// an empty or unchanged refresh token, the marker stays and the next call receives
/// `NeedsReauthorization`. After an unusable access token was saved (`Expired`, or a
/// header that cannot be built), the next call renews with the saved refresh token and
/// never sends that access token. When the outcome of an exchange is uncertain, the
/// account must be authorized again. An exchange that fails with
/// [`RenewalError::NotSent`] states that its request never left: the pair from before the
/// exchange is saved again over the marker, without a usable expiry (its `expires_at()`
/// is `None`), because its access token was being replaced. Once that save succeeds, the
/// pair is current again; the waiting calls fail with that error without being sent, and
/// the next call, in this instance or in any owner that loads the store later, exchanges
/// the same refresh token before it sends an access token. If that save fails, this
/// instance stays blocked, as after any failed exchange, and its next call receives
/// `NeedsReauthorization`; the store holds either the marker or the restored pair,
/// and an owner that loads it later requires a new authorization, or exchanges. If a
/// panic destroys the exchange's future, nothing is saved again, the waiting calls
/// receive `RenewalError::Panicked`, and the marker stays. No
/// store can commit the server's issuance and the local save atomically: a crash between
/// the two can lose a token.
///
/// A `401` is returned unchanged and rejects only the access token that the request
/// carried, when its final URL is at the origin of the base; a response from another
/// origin rejects nothing. Add `after_401` to the engine's retry policy; this middleware never resends
/// a request. An API that rejects a token inside another status, such as a `200`, is
/// described once with [`rejected_when`](Rotating::rejected_when). A call that resumes waiting checks the expiry, the settings and the state
/// of the driver again. Relative lifetimes include the time spent saving; retrying a
/// pending save never restarts them.
///
/// The state kept is one account record, one pending replacement, one renewal in
/// progress, and one notification per waiting call; no history and no queue of
/// requests. The size of tokens, what callbacks capture, the buffers of the store and
/// the number of calls in progress are the application's to bound. All callbacks,
/// notifications and drops of the author's values happen outside the state lock. No
/// task is started.
///
/// Clones share one account, one renewal and one configuration: `header`, `early_by`
/// or `renewal_timeout` called on any clone changes every clone. Configure it fully,
/// then clone.
#[doc = include_str!("rotating/example.md")]
#[derive(Clone)]
#[must_use = "configure the credential and attach it as middleware"]
#[cfg(feature = "rotating")]
#[cfg_attr(docsrs, doc(cfg(feature = "rotating")))]
pub struct Rotating {
    inner: Arc<engine::Inner>,
    owners: Option<Arc<()>>,
}
impl Rotating {
    /// Creates a credential and its one driver, without loading the store or sending a request.
    ///
    /// Defaults: no early margin, a wait of 30 seconds for each call, and a sensitive
    /// `Authorization: Bearer` header. The store must already contain an initial grant.
    ///
    /// ```no_run
    /// # use concord::auth::{CredentialStore, Exchange, RenewalError, Rotating, TokenPair};
    /// fn connect(store: impl CredentialStore, base: &concord::Base) {
    ///     let (credential, driver) = Rotating::new(base, store, |_: Exchange| async {
    ///         Err::<TokenPair, _>(RenewalError::failed(std::io::Error::other("exchange required")))
    ///     });
    /// }
    /// ```
    #[must_use = "attach the credential and poll its driver"]
    pub fn new<S, F, Fut>(base: &Base, store: S, exchange: F) -> (Self, RotationDriver)
    where
        S: CredentialStore,
        F: Fn(Exchange) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<TokenPair, RenewalError>> + Send + 'static,
    {
        let owners = Arc::new(());
        let inner = engine::Inner::new(base, store, Arc::new(move |input| exchange(input).boxed()));
        let work = engine::run(inner.clone(), Arc::downgrade(&owners)).boxed();
        (
            Self {
                inner: inner.clone(),
                owners: Some(owners),
            },
            RotationDriver {
                inner: Some(inner),
                work: Some(work),
            },
        )
    }

    /// Renews the access token when at most `margin` of its lifetime is left: zero by default.
    ///
    /// Configuring a clone changes the margin shared by every clone, as with `Renewed`.
    ///
    /// ```no_run
    /// fn configure(auth: concord::auth::Rotating) -> concord::auth::Rotating {
    ///     auth.early_by(std::time::Duration::from_secs(20))
    /// }
    /// ```
    #[must_use = "attach the configured credential as middleware"]
    pub fn early_by(self, margin: Duration) -> Self {
        self.inner.lock().policy.margin = margin;
        self
    }

    /// Sets how long each call waits for a token, not how long the exchange may take:
    /// 30 seconds by default.
    ///
    /// Configuring a clone changes the wait shared by every clone. Zero is refused before
    /// the store is accessed. A call whose driver was not polled fails with `NotPolled`;
    /// any other wait that expires fails with `Renewal(RenewalError::TimedOut)`. Unlike
    /// `Renewed::renewal_timeout`, this never cancels an exchange: cancelling a
    /// single-use exchange already sent could lose its replacement. Set a timeout on
    /// the token client's request yourself. Renewal and saving go on after this wait.
    ///
    /// ```no_run
    /// fn configure(auth: concord::auth::Rotating) -> concord::auth::Rotating {
    ///     auth.renewal_timeout(std::time::Duration::from_secs(5))
    /// }
    /// ```
    #[must_use = "attach the configured credential as middleware"]
    pub fn renewal_timeout(self, timeout: Duration) -> Self {
        self.inner.lock().policy.timeout = timeout;
        self
    }

    /// Sets the header that carries the access token, and its whole value, always
    /// marked sensitive.
    ///
    /// By default it is `Authorization: Bearer <token>`. An error formatting or inserting
    /// the header keeps its cause through `RenewalError`. Configuring a clone changes the
    /// header shared by every clone, as with `Renewed`.
    ///
    /// ```no_run
    /// fn configure(auth: concord::auth::Rotating) -> concord::auth::Rotating {
    ///     auth.header(concord::http::HeaderName::from_static("x-token"), |token| {
    ///         concord::http::HeaderValue::from_str(&token.expose())
    ///     })
    /// }
    /// ```
    #[must_use = "attach the configured credential as middleware"]
    pub fn header(
        self,
        name: HeaderName,
        format: impl Fn(&Secret) -> Result<HeaderValue, InvalidHeaderValue> + Send + Sync + 'static,
    ) -> Self {
        let format: Arc<engine::Format> = Arc::new(format);
        let retired = {
            let mut state = self.inner.lock();
            (
                std::mem::replace(&mut state.policy.name, name),
                std::mem::replace(&mut state.policy.format, format),
            )
        };
        drop(retired);
        self
    }

    /// Sets a function that recognises a response that rejects the access token although
    /// its status is not `401`: an API that reports a revoked or expired token inside a
    /// `200`.
    ///
    /// It works as [`Renewed::rejected_when`](super::Renewed::rejected_when): the same
    /// [`Seen`](super::Seen), shown at the same moments, for a response from the origin of
    /// the credential's base whose whole body, of at most 64 KiB and complete by the same
    /// rule, has been read. When it returns `true`, the access token that the request
    /// carried is rejected as after a `401`: only if it is still the current token, so an
    /// answer that arrives after a renewal never discards the newer pair. Nothing is
    /// exchanged at once: each later attempt that needs the token, of any call, joins the
    /// exchange in flight or starts one, and the retry setting of each call bounds its
    /// attempts. The response is returned unchanged. `rejected_when` itself
    /// schedules no retry, and `after_401` retries a `401` only; a retry rule of the call
    /// that applies to the response can send the call again, and that attempt then waits
    /// for the exchange and carries the new token. A `401` always rejects the token, and
    /// the function is not called for it.
    ///
    /// Watching costs something, also when the function never matches: every response
    /// through the credential from its origin is observed, and up to 64 KiB of its body is
    /// copied into one buffer, dropped once the function has seen it, or as soon as the body
    /// is longer. Frames, trailers, the size hint and the end of the body pass through
    /// unchanged; without this function, a response is not touched. The function must
    /// return promptly; a panic in it rejects nothing, but Rust's panic hook still prints
    /// the panic message: never put text of the body in a panic message. Nothing bounds the
    /// exchanges across calls when a server rejects every token; each call waits for one
    /// at most `renewal_timeout`. Configuring a clone changes every clone.
    ///
    /// ```no_run
    /// #[derive(serde::Deserialize)]
    /// struct Envelope { ok: bool, error: Option<String> }
    /// fn configure(auth: concord::auth::Rotating) -> concord::auth::Rotating {
    ///     auth.rejected_when(|seen| {
    ///         seen.status() == 200
    ///             && serde_json::from_slice::<Envelope>(seen.body()).is_ok_and(|reply| {
    ///                 !reply.ok && matches!(reply.error.as_deref(), Some("token_expired" | "invalid_auth"))
    ///             })
    ///     })
    /// }
    /// ```
    #[must_use = "attach the configured credential as middleware"]
    pub fn rejected_when(
        self,
        rejects: impl Fn(&super::Seen<'_>) -> bool + Send + Sync + 'static,
    ) -> Self {
        let rejects: Arc<super::rejection::Rejects> = Arc::new(rejects);
        let retired = self.inner.lock().policy.rejects.replace(rejects);
        drop(retired);
        self
    }
}
impl Drop for Rotating {
    fn drop(&mut self) {
        drop(self.owners.take());
        self.inner.demand.notify_one();
    }
}
impl fmt::Debug for Rotating {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Rotating(..)")
    }
}
impl Middleware for Rotating {
    async fn handle(
        &self,
        mut request: Request,
        info: TryInfo,
        next: Next<'_>,
    ) -> Result<Response, TransportError> {
        let blocked = |error| TransportError::blocked().with_source(error);
        if !self
            .inner
            .origin
            .as_ref()
            .is_some_and(|base| base.origin() == request.url().origin())
        {
            return Err(blocked(RotationError::Renewal(RenewalError::WrongOrigin)));
        }
        let mut wait = self.inner.wait().map_err(blocked)?;
        let access = loop {
            let access = self.inner.access(&mut wait).await.map_err(blocked)?;
            let displaced = request
                .headers_mut()
                .try_insert(access.name.clone(), access.header.clone())
                .map_err(|cause| blocked(RotationError::Renewal(RenewalError::failed(cause))))?;
            // Replacing a header can destroy an author's byte owner. Its Drop may
            // change policy or advance time, so dispatch needs another check.
            drop(displaced);
            if self.inner.is_current(&access, &wait) {
                break access;
            }
            // PANIC-SAFETY: HeaderMap::remove looks up a name; it does not index a sequence.
            drop(request.headers_mut().remove(&access.name));
            drop(access);
        };
        let method = request.method().clone();
        let response = next.send(request, info).await?;
        // A response from another origin, as a transport that followed a redirect
        // reports it, rejects nothing.
        if !super::rejection::same_origin(self.inner.origin.as_ref(), &response) {
            return Ok(response);
        }
        if response.status() == 401 {
            self.inner.reject(&access.generation);
            return Ok(response);
        }
        let rejects = self.inner.lock().policy.rejects.clone();
        Ok(match rejects {
            Some(rejects) => {
                let inner = Arc::downgrade(&self.inner);
                let generation = Arc::downgrade(&access.generation);
                super::rejection::watch(response, &method, rejects, move || {
                    if let (Some(inner), Some(generation)) = (inner.upgrade(), generation.upgrade())
                    {
                        inner.reject(&generation);
                    }
                })
            }
            None => response,
        })
    }
}

fn default_format(token: &Secret) -> Result<HeaderValue, InvalidHeaderValue> {
    HeaderValue::from_str(&format!("Bearer {}", &*token.expose()))
}

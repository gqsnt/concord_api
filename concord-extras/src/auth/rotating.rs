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

/// An immutable access/refresh pair and an optional access expiry.
///
/// Secrets are snapshotted, including when supplied as mutable `Secret` aliases.
/// Accessors return independent secrets. `None` means access cannot be used; a
/// valid replacement refresh token is still saved before reporting `Expired`.
///
/// ```no_run
/// let pair = concord::auth::TokenPair::expires_in("access", "refresh", std::time::Duration::from_secs(300));
/// let saved_expiry = pair.expires_at();
/// ```
#[derive(Clone)]
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
    /// Restores a stored pair; past and absent expiries require renewal before access.
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

    /// Creates an exchange result without failing on zero lifetime or time overflow.
    ///
    /// The lifetime uses the renewal timer (including virtual time in tests),
    /// starts here and includes time spent saving. An unrepresentable
    /// expiry becomes `None`; the refresh token can still be saved safely.
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

    /// Returns an independent access-token snapshot, without a header scheme.
    ///
    /// ```no_run
    /// # let pair = concord::auth::TokenPair::new("access", "refresh", None);
    /// let access = pair.access();
    /// ```
    pub fn access(&self) -> Secret {
        Secret::new(self.access.to_string())
    }

    /// Returns an independent refresh-token snapshot.
    ///
    /// ```no_run
    /// # let pair = concord::auth::TokenPair::new("access", "refresh", None);
    /// let refresh = pair.refresh();
    /// ```
    pub fn refresh(&self) -> Secret {
        Secret::new(self.refresh.to_string())
    }

    /// Returns the wall expiry to persist, or `None` for unusable access.
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

/// The complete durable state of one account.
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
pub enum Stored {
    /// A saved pair, whose access token may already be unusable.
    Ready(TokenPair),
    /// Durable intent; there is no known refresh token safe to submit again.
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

/// Atomic, durable storage for one exclusively owned account record.
///
/// Success acknowledges durable replacement. After `save` returns, **including
/// an error**, that operation has no further effects on storage: its outcome can
/// be unknown, but must be final. Saving the same record again must be safe. A
/// timed-out HTTP PUT does not in general have this property.
///
/// Operations are serialized and never timed out by Concord. A store panic stops
/// this instance; after a panic or cancellation the application must establish
/// quiescence before another owner uses the account. No late write may race it.
/// One `Rotating` and its clones own the account; other processes require an
/// application-owned lease. This interface supplies no cross-process locking.
/// Callbacks must yield promptly, bound their own allocations and errors, and
/// must not wait on a resource call that needs this credential.
///
/// The [whole example](Rotating) supplies a local store with this finality property.
pub trait CredentialStore: Send + Sync + 'static {
    /// Loads the last complete record, preserving storage errors as causes.
    ///
    /// ```no_run
    /// async fn load(store: &impl concord::auth::CredentialStore)
    ///     -> Result<concord::auth::Stored, concord::auth::RenewalError> { store.load().await }
    /// ```
    fn load(&self) -> impl Future<Output = Result<Stored, RenewalError>> + Send;
    /// Atomically replaces the whole record and acknowledges durability.
    ///
    /// ```no_run
    /// async fn mark(store: &impl concord::auth::CredentialStore)
    ///     -> Result<(), concord::auth::RenewalError> { store.save(concord::auth::Stored::Refreshing).await }
    /// ```
    fn save(&self, record: Stored) -> impl Future<Output = Result<(), RenewalError>> + Send;
}

/// The isolated refresh token supplied to one exchange, with room for more inputs.
///
/// Use a separate token client with retries disabled, including in its transport.
/// Bound that client's request explicitly: the driver never cancels the exchange.
///
/// ```no_run
/// fn token(exchange: &concord::auth::Exchange) -> concord::Secret { exchange.refresh().clone() }
/// ```
pub struct Exchange {
    refresh: Secret,
}
impl Exchange {
    /// Borrows the exchange's snapshot; replacing it cannot change retained state.
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

/// A rotation failure with its underlying cause retained.
///
/// `Display` prints one sentence for its variant; `Debug` prints its name.
/// Neither prints values, causes or panic payloads.
/// `source()` deliberately exposes the retained cause; application errors remain
/// the application's responsibility. Shared renewal conditions have the same
/// spelling and cause as for `Renewed`. A caught panic discards its payload,
/// but the application's panic hook still runs; with panic=abort it aborts.
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
pub enum RotationError {
    /// Loading failed, preserving the store's error.
    Load(RenewalError),
    /// Saving failed; the final outcome may be unknown and is reloaded.
    Save(RenewalError),
    /// Exchange, wait timeout, panic, header, expiry, origin, policy, or overflow failure.
    Renewal(RenewalError),
    /// The refresh token is empty, or an exchange returned the token it consumed.
    InvalidRefreshToken,
    /// A durable intent has no known replacement; provider recovery is required.
    NeedsReauthorization,
    /// The driver was not polled during this call's wait.
    NotPolled,
    /// The driver was dropped, or a storage panic permanently stopped the account.
    Stopped,
    /// The driver could not durably save a replacement and returns it for recovery.
    /// Establish store quiescence after a panic before saving it by other means.
    #[non_exhaustive]
    Unsaved {
        /// The exact replacement, including an unusable access token or expiry.
        pair: TokenPair,
        /// The final recovery failure, retained as this error's source.
        cause: RenewalError,
    },
}
impl RotationError {
    /// Finds this error in a source chain, including a cycle containing this error.
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

/// The future that owns one rotating account's exchanges and saves.
///
/// The application polls or spawns this future; Concord starts no task or thread.
/// It is `Send` and `Unpin`, not `Sync` or `Clone`. While it remains polled, a
/// begun renewal finishes with its replacement saved or handed back in
/// [`RotationError::Unsaved`], even when every resource call is dropped or times
/// out. Renewal begins when saving its marker begins; dropping the last waiting
/// call during the initial load leaves no begun renewal. Store operations are
/// never timed out: the initial load is dropped when its last waiting call is
/// dropped, but a save is never dropped while the driver remains polled. The
/// exchange is never timed out by the driver. Dropping the driver cancels its
/// work and stops the credential; no asynchronous work runs from `Drop`.
///
/// When the last credential owner goes away, it drains begun work, makes one
/// further recovery attempt for a pending save, then returns success or the
/// unresolved failure. If that attempt fails, it returns `Unsaved` with the pair.
/// A store panic stops the credential with `Stopped`; an existing waiter whose
/// deadline has passed still reports its timeout. The driver returns `Unsaved`
/// if it holds a replacement, otherwise `Stopped`.
/// Ordinary failures go to waiting calls and leave the driver available for new
/// demand. A hung store or exchange prevents draining; resource waits still have
/// their own deadlines. The first driver poll after a call begins waiting must
/// precede its deadline, or that call receives `NotPolled`, even if polled late.
///
/// The saved-or-returned promise begins when the author's exchange future
/// yields the pair. Concord cannot protect a pair the author's code loses before
/// yielding it. Concord guards destruction of author-owned values once; a panic
/// payload that panics again when destroyed is outside what it can contain.
/// Concord's own formatting never prints a token. An author's store error and
/// anything printed by traversing its sources remain the author's responsibility.
///
/// Poll within a Tokio runtime. Completing a renewal wakes every waiting call;
/// that work grows with the number of waiters. Callbacks and destructors must
/// return promptly. The [whole example](Rotating) shows an application-owned task.
///
/// The application owns recovery of an unsaved pair. After a store panic, first
/// establish that previous operations can have no later effects; the recovery
/// store below must already meet that requirement. If its own save fails, this
/// example returns the original error, keeping the pair available to its caller.
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

/// Middleware that renews access with a single-use refresh token.
///
/// Configure it fully, then clone; clones share one account and one renewal.
/// As with `Renewed`, configuring any clone changes the shared header, early
/// margin and wait allowance for every clone of that account.
/// Keep the returned `RotationDriver` polled independently of resource calls.
/// The origin is checked before store access: a base path prefix neither narrows
/// nor widens the scheme, host and effective-port comparison.
///
/// The durable `Refreshing` marker precedes exchange. A replacement is saved
/// before either token is used, even if access has no usable expiry or cannot
/// form a header. A failed final save retains the pair; the next demand reloads
/// the store and retries only saving, never exchanging the old token again.
/// Even a reload showing the same pair requires another successful save: visibility
/// is not proof of durability. An empty stored refresh token is refused before
/// saving a marker; after an exchange returns an empty or unchanged refresh
/// token, the marker remains and the next call receives `NeedsReauthorization`.
/// After unusable access was saved (`Expired` or a header that cannot be built),
/// the next call renews with the saved refresh token and never sends that access.
/// Uncertain exchange outcomes require reauthorization. No store can atomically
/// commit remote issuance and local saving: a crash in that gap can lose a token.
///
/// A 401 is returned unchanged and rejects only its access generation. Compose
/// `after_401` with the engine's retry policy; this middleware never resends a
/// resource request. A resumed waiter rechecks expiry, policy and driver state.
/// Relative lifetimes include save time; pending-save retries never restart them.
///
/// Retained state is one account record, one pending replacement, one workflow,
/// and one notification per waiting call; no history or request queue is kept.
/// Token sizes, callback captures, storage buffers and live call counts belong
/// to the application. All callbacks, notifications and author-owned drops occur
/// outside the state lock. No task is started.
#[doc = include_str!("rotating/example.md")]
#[derive(Clone)]
#[must_use = "configure the credential and attach it as middleware"]
pub struct Rotating {
    inner: Arc<engine::Inner>,
    owners: Option<Arc<()>>,
}
impl Rotating {
    /// Creates a credential and its unique driver without loading or sending.
    /// Defaults: zero early margin, a 30-second caller wait, sensitive Bearer header.
    /// The store must already contain an initial grant.
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

    /// Renews access with at most this much lifetime left; default: zero.
    /// Configuring a clone changes the margin shared by every clone, as in `Renewed`.
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

    /// Bounds how long each call waits, not the exchange; default: 30 seconds.
    /// Configuring a clone changes the allowance shared by every clone.
    /// Zero is refused before storage. An unpolled driver produces `NotPolled`;
    /// another expired wait produces `Renewal(RenewalError::TimedOut)`.
    /// Unlike `Renewed::renewal_timeout`, this never cancels an exchange: losing
    /// a sent single-use exchange could lose its replacement. Bound the token
    /// client's request explicitly. Renewal and saving continue after this wait.
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

    /// Selects a header and its whole value, always marking it sensitive.
    /// Formatting or insertion errors retain their causes through `RenewalError`.
    /// Configuring a clone changes the header shared by every clone, as in `Renewed`.
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
        let response = next.send(request, info).await?;
        if response.status() == 401 {
            let mut state = self.inner.lock();
            if let Some(current) = &mut state.current {
                if Arc::ptr_eq(&current.generation, &access.generation) {
                    current.rejected = true;
                }
            }
        }
        Ok(response)
    }
}

fn default_format(token: &Secret) -> Result<HeaderValue, InvalidHeaderValue> {
    HeaderValue::from_str(&format!("Bearer {}", &*token.expose()))
}

use std::{
    collections::HashMap,
    fmt,
    future::Future,
    hash::Hash,
    num::NonZeroUsize,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant},
};

use futures_util::{
    FutureExt,
    future::{BoxFuture, Shared},
};
use http::HeaderValue;

/// One acquisition result: the credential and its untrusted lifetime.
/// The cache measures the lifetime from the first poll of the acquisition, before its I/O.
#[derive(Clone)]
pub struct Token {
    header: HeaderValue,
    lifetime: Duration,
}

impl Token {
    /// Accepts a validated HTTP header and makes it sensitive, even if its caller forgot to.
    /// Use [`crate::basic`], [`crate::bearer`] or [`crate::authorization`] for credential syntax.
    /// Lifetime validation happens in [`TokenCache::get_or_refresh`] with the injected clock.
    pub fn new(mut header: HeaderValue, lifetime: Duration) -> Self {
        header.set_sensitive(true);
        Self { header, lifetime }
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Token(<redacted>)")
    }
}

/// An opaque credential identity. Equality is meaningful across clones, keys and caches.
/// It cannot wrap or be reused while a stale snapshot still exists.
#[derive(Clone)]
pub struct Generation(Arc<()>);

impl PartialEq for Generation {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for Generation {}

impl fmt::Debug for Generation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Generation(<opaque>)")
    }
}

/// A sensitive credential paired with the generation to invalidate if its request gets a 401.
/// A snapshot is immutable; obtaining it does not grant permission to replay a request.
#[derive(Clone)]
pub struct Snapshot {
    header: HeaderValue,
    generation: Generation,
    refresh_at: Instant,
}

impl Snapshot {
    /// The sensitive header to apply immediately before an application attempt.
    pub fn header(&self) -> &HeaderValue {
        &self.header
    }

    /// The identity of exactly this credential acquisition.
    pub fn generation(&self) -> Generation {
        self.generation.clone()
    }
}

impl fmt::Debug for Snapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Snapshot(<redacted>)")
    }
}

/// A failed acquisition or a local refusal. Formatting never delegates to the provider error.
pub enum RefreshError<E> {
    /// Every caller waiting on this acquisition receives the same error allocation.
    Acquire(Arc<E>),
    /// The provider lifetime cannot be represented by the monotonic clock.
    LifetimeOverflow,
    /// All capacity slots are acquiring tokens; no active acquisition was evicted.
    Capacity,
}

impl<E> Clone for RefreshError<E> {
    fn clone(&self) -> Self {
        match self {
            Self::Acquire(error) => Self::Acquire(error.clone()),
            Self::LifetimeOverflow => Self::LifetimeOverflow,
            Self::Capacity => Self::Capacity,
        }
    }
}

impl<E> fmt::Debug for RefreshError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Acquire(_) => "RefreshError::Acquire(<redacted>)",
            Self::LifetimeOverflow => "RefreshError::LifetimeOverflow",
            Self::Capacity => "RefreshError::Capacity",
        })
    }
}

impl<E> fmt::Display for RefreshError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Acquire(_) => "credential acquisition failed",
            Self::LifetimeOverflow => "credential lifetime exceeds clock range",
            Self::Capacity => "credential cache capacity is occupied by acquisitions",
        })
    }
}

// No source(): even an upstream error's formatting may reveal credentials. The explicit Acquire
// field lets the SDK inspect/classify the error without making ordinary error chains leak it.
impl<E> std::error::Error for RefreshError<E> {}

type Acquisition<E> = Shared<BoxFuture<'static, Result<Snapshot, RefreshError<E>>>>;

enum Entry<E> {
    Ready(Snapshot),
    Refreshing(Generation, Acquisition<E>),
}

/// A bounded token cache for **SDK-selected keys**, with one owned acquisition per key/generation.
///
/// The key (or the cache instance's immutable context) must distinguish destination/audience,
/// tenant, principal and scopes/access. No scope, realm trust or replay policy is inferred here.
/// `E` preserves the SDK's acquisition error; it need not implement `Clone`, `Debug` or `Error`.
///
/// The cache owns shared futures: cancellation of a caller does not cancel the acquisition while
/// the entry remains. Other waiters, or the next caller, drive it to completion. No tasks are
/// spawned. A provider future must itself be nonblocking and obey its own timeout policy.
/// Acquisition closures/futures must not panic: a panic poisons the shared future; use `remove`
/// to discard such an entry before reuse. Cancellation does not poison it.
/// Do not capture this cache in its own acquisition closure (that would create an Arc cycle).
/// Keys must have nonpanicking, stable `Hash`/`Eq` implementations.
pub struct TokenCache<K, E> {
    entries: Mutex<HashMap<K, Entry<E>>>,
    capacity: usize,
    margin: Duration,
    now: Arc<dyn Fn() -> Instant + Send + Sync>,
}

impl<K, E> fmt::Debug for TokenCache<K, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Even keys, the clock closure and upstream errors can contain secrets.
        f.write_str("TokenCache(<redacted>)")
    }
}

impl<K: Eq + Hash + Clone, E: Send + Sync + 'static> TokenCache<K, E> {
    /// Uses the platform monotonic clock and a bounded number of keys.
    /// At capacity an arbitrary ready entry is evicted; in-flight entries are never evicted.
    pub fn new(capacity: NonZeroUsize, refresh_margin: Duration) -> Self {
        Self::with_clock(capacity, refresh_margin, Instant::now)
    }

    /// Injects a monotonic clock, principally for deterministic expiry tests.
    /// The clock must be nonblocking, nonpanicking and return comparable instants.
    pub fn with_clock(
        capacity: NonZeroUsize,
        refresh_margin: Duration,
        now: impl Fn() -> Instant + Send + Sync + 'static,
    ) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            capacity: capacity.get(),
            margin: refresh_margin,
            now: Arc::new(now),
        }
    }

    fn entries(&self) -> MutexGuard<'_, HashMap<K, Entry<E>>> {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Reads a credential still outside its refresh margin, without starting acquisition.
    /// Expired entries remain bounded and can be replaced by acquisition or explicit removal.
    pub fn get(&self, key: &K) -> Option<Snapshot> {
        let now = (self.now)();
        match self.entries().get(key) {
            Some(Entry::Ready(token)) if now < token.refresh_at => Some(token.clone()),
            _ => None,
        }
    }

    /// Invalidates only the generation that was actually used by a failed request.
    /// A delayed 401 cannot invalidate a newer token or its in-flight refresh.
    pub fn invalidate(&self, key: &K, generation: &Generation) -> bool {
        let mut entries = self.entries();
        if matches!(entries.get(key), Some(Entry::Ready(token)) if &token.generation == generation)
        {
            entries.remove(key);
            true
        } else {
            false
        }
    }

    /// Explicitly forgets a key, including an abandoned acquisition, freeing its capacity slot.
    /// Existing waiters still finish their shared acquisition but cannot repopulate this entry.
    /// A subsequent call may start a new acquisition while those old waiters finish.
    pub fn remove(&self, key: &K) -> bool {
        let removed = { self.entries().remove(key) };
        // User future/capture destructors must also run outside the map lock.
        removed.is_some()
    }

    /// Returns a cached token or joins/starts one shared acquisition.
    ///
    /// Only the winning caller's closure is invoked, outside the mutex. The future is owned
    /// (`Send + 'static`) so cancelling the starter cannot strand other waiters. A failed flight
    /// is removed; its existing waiters share the failure, and the next call can try again.
    ///
    /// Lifetime arithmetic is checked **before** subtracting the margin. Zero lifetime or a
    /// margin >= lifetime returns the acquisition to current waiters but never reuses it in a
    /// later call. Time spent acquiring consumes lifetime. No internal refresh or retry loop runs.
    pub async fn get_or_refresh<F, Fut>(
        &self,
        key: K,
        acquire: F,
    ) -> Result<Snapshot, RefreshError<E>>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<Token, E>> + Send + 'static,
    {
        let now = (self.now)();
        let (generation, flight) = {
            let mut entries = self.entries();
            match entries.get(&key) {
                Some(Entry::Ready(token)) if now < token.refresh_at => return Ok(token.clone()),
                Some(Entry::Refreshing(generation, flight)) => (generation.clone(), flight.clone()),
                _ => {
                    if !entries.contains_key(&key) && entries.len() == self.capacity {
                        let victim = entries.iter().find_map(|(key, entry)| {
                            matches!(entry, Entry::Ready(_)).then(|| key.clone())
                        });
                        if let Some(victim) = victim {
                            entries.remove(&victim);
                        } else {
                            return Err(RefreshError::Capacity);
                        }
                    }
                    let generation = Generation(Arc::new(()));
                    let identity = generation.clone();
                    let clock = self.now.clone();
                    let margin = self.margin;
                    let flight = async move {
                        let started = clock();
                        let token = acquire()
                            .await
                            .map_err(|e| RefreshError::Acquire(Arc::new(e)))?;
                        // Validate the full provider lifetime, not a possibly smaller margin-adjusted one.
                        started
                            .checked_add(token.lifetime)
                            .ok_or(RefreshError::LifetimeOverflow)?;
                        let refresh_at = started
                            .checked_add(token.lifetime.saturating_sub(margin))
                            .ok_or(RefreshError::LifetimeOverflow)?;
                        Ok(Snapshot {
                            header: token.header,
                            generation: identity,
                            refresh_at,
                        })
                    }
                    .boxed()
                    .shared();
                    entries.insert(
                        key.clone(),
                        Entry::Refreshing(generation.clone(), flight.clone()),
                    );
                    (generation, flight)
                }
            }
        };
        let result = flight.await;
        let mut entries = self.entries();
        // An explicitly removed/evicted entry or another generation must never be resurrected.
        if matches!(entries.get(&key), Some(Entry::Refreshing(current, _)) if current == &generation)
        {
            match &result {
                Ok(token) => {
                    entries.insert(key, Entry::Ready(token.clone()));
                }
                Err(_) => {
                    entries.remove(&key);
                }
            }
        }
        result
    }
}

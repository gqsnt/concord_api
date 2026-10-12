//! Deterministic shared-persistent limiter conformance fixture (RATE-05).
//!
//! [`InMemorySharedStore`] simulates one shared persistent limiter backend
//! (one storage namespace). Every [`InMemorySharedLimiter`] obtained from it
//! simulates one process or binary version attached to that backend. The
//! fixture demonstrates the exact [`Enforcement::SharedPersistent`] contract:
//!
//! * Inside `acquire`, before any quota state is read or written, every policy
//!   schema of the plan is atomically compared with, or registered in, the
//!   store's schema registry under the adapter's namespace and algorithm
//!   version.
//! * An incompatible active schema under the same policy ID (a changed
//!   encoding ID, label, dimension order, window schema or algorithm version)
//!   fails closed with [`RateLimitErrorKind::SchemaConflict`]; it is never
//!   treated as a fresh empty bucket.
//! * State written by a pre-v3 peer that registered no schema is not assumed
//!   compatible: it fails closed until an operator installs an explicit
//!   schema lock ([`InMemorySharedStore::install_schema_lock`]).
//! * Registration storage is finite (4,096 identities by default), counting
//!   both registered schemas and legacy continuity markers. Converting a
//!   legacy marker to its explicit schema lock reuses that identity's slot.
//!   No active contract is evicted; only an explicit operator retirement
//!   ([`InMemorySharedStore::retire_schema`]) removes one, which is a reviewed
//!   state reset.
//! * Admission is atomic and conjunctive: either every window of every policy
//!   is debited and every new schema registered, or nothing changes.
//!
//! The quota algorithm (version 1) is a deterministic fixed window per bucket,
//! anchored at the bucket's first use and measured with Tokio's clock, that
//! rejects an exhausted plan with [`RateLimitErrorKind::Acquire`] instead of
//! waiting. It exists to test schema continuity and admission atomicity; it
//! is not a distributed limiter implementation and performs no I/O.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use concord_core::contracts::{BoxFuture, RateLimitErrorKind};
use concord_core::rate_limit::{
    DEFAULT_RATE_LIMIT_KEY_BYTES, DEFAULT_RATE_SCHEMA_REGISTRATIONS, Enforcement,
    LimiterCapability, RateLimitError, RateLimitId, RateLimitPermit, RateLimitPlan,
    RateLimitRequest, RateLimiter, RatePolicySchema,
};

/// Default finite number of quota buckets held by one in-memory store.
pub const DEFAULT_SHARED_QUOTA_ENTRIES: usize = 4_096;

/// The quota algorithm version implemented by this fixture.
pub const FIXED_WINDOW_ALGORITHM_VERSION: u32 = 1;

/// A simulated shared persistent limiter backend.
///
/// Cloning shares the same backend. All state is process memory guarded by
/// one mutex; operations are synchronous, bounded and never hold the lock
/// across an await point. A poisoned lock makes every later operation fail
/// closed with [`RateLimitErrorKind::Internal`].
#[derive(Clone)]
pub struct InMemorySharedStore {
    inner: Arc<StoreInner>,
}

struct StoreInner {
    namespace: &'static str,
    registration_capacity: usize,
    quota_capacity: usize,
    state: Mutex<StoreState>,
    failed: AtomicBool,
}

#[derive(Default)]
struct StoreState {
    registrations: BTreeMap<RateLimitId, Registration>,
    legacy: BTreeSet<RateLimitId>,
    buckets: BTreeMap<BucketKey, WindowState>,
    admissions: u64,
}

struct Registration {
    algorithm_version: u32,
    schema: RatePolicySchema,
}

#[derive(Clone, Eq, Ord, PartialEq, PartialOrd)]
struct BucketKey {
    id: RateLimitId,
    parts: Vec<(Box<str>, Box<[u8]>)>,
    duration: Duration,
    capacity: u32,
}

struct WindowState {
    start: tokio::time::Instant,
    used: u64,
}

impl InMemorySharedStore {
    /// Creates a store with the default finite capacities.
    ///
    /// # Errors
    ///
    /// Returns [`RateLimitErrorKind::InvalidConfiguration`] for an invalid
    /// namespace (see [`LimiterCapability::validate`]).
    pub fn new(namespace: &'static str) -> Result<Self, RateLimitError> {
        Self::with_capacities(
            namespace,
            DEFAULT_RATE_SCHEMA_REGISTRATIONS,
            DEFAULT_SHARED_QUOTA_ENTRIES,
        )
    }

    /// Creates a store with explicit positive registration and quota-bucket
    /// capacities.
    ///
    /// # Errors
    ///
    /// Returns [`RateLimitErrorKind::InvalidConfiguration`] for an invalid
    /// namespace or a zero capacity.
    pub fn with_capacities(
        namespace: &'static str,
        registration_capacity: usize,
        quota_capacity: usize,
    ) -> Result<Self, RateLimitError> {
        LimiterCapability::new(
            Enforcement::SharedPersistent,
            namespace,
            FIXED_WINDOW_ALGORITHM_VERSION,
        )
        .validate()?;
        if registration_capacity == 0 || quota_capacity == 0 {
            return Err(invalid_configuration());
        }
        Ok(Self {
            inner: Arc::new(StoreInner {
                namespace,
                registration_capacity,
                quota_capacity,
                state: Mutex::new(StoreState::default()),
                failed: AtomicBool::new(false),
            }),
        })
    }

    /// Attaches one adapter instance (one simulated process) that interprets
    /// state with [`FIXED_WINDOW_ALGORITHM_VERSION`].
    #[must_use]
    pub fn limiter(&self) -> InMemorySharedLimiter {
        self.limiter_with_algorithm(FIXED_WINDOW_ALGORITHM_VERSION)
    }

    /// Attaches one adapter instance that claims a specific algorithm
    /// version, for example to simulate a differently versioned peer.
    #[must_use]
    pub fn limiter_with_algorithm(&self, algorithm_version: u32) -> InMemorySharedLimiter {
        InMemorySharedLimiter {
            store: self.clone(),
            algorithm_version,
        }
    }

    /// Returns the registered schema for one policy ID, if any.
    ///
    /// # Errors
    ///
    /// Returns [`RateLimitErrorKind::Internal`] after lock poisoning.
    pub fn registered_schema(
        &self,
        id: &str,
    ) -> Result<Option<(u32, RatePolicySchema)>, RateLimitError> {
        let state = self.lock()?;
        Ok(state.registrations.iter().find_map(|(key, registration)| {
            (key.as_str() == id)
                .then(|| (registration.algorithm_version, registration.schema.clone()))
        }))
    }

    /// Returns the number of active schema registrations.
    ///
    /// # Errors
    ///
    /// Returns [`RateLimitErrorKind::Internal`] after lock poisoning.
    pub fn registration_count(&self) -> Result<usize, RateLimitError> {
        Ok(self.lock()?.registrations.len())
    }

    /// Returns the number of retained quota buckets.
    ///
    /// # Errors
    ///
    /// Returns [`RateLimitErrorKind::Internal`] after lock poisoning.
    pub fn bucket_count(&self) -> Result<usize, RateLimitError> {
        Ok(self.lock()?.buckets.len())
    }

    /// Returns the number of committed admissions.
    ///
    /// # Errors
    ///
    /// Returns [`RateLimitErrorKind::Internal`] after lock poisoning.
    pub fn admission_count(&self) -> Result<u64, RateLimitError> {
        Ok(self.lock()?.admissions)
    }

    /// Simulates quota state for `id` written by a pre-v3 peer that performed
    /// no schema registration.
    ///
    /// Until an operator installs an explicit schema lock, adapters fail
    /// closed for this ID instead of assuming a unilateral handshake proves
    /// compatibility with the old binary.
    ///
    /// # Errors
    ///
    /// Returns [`RateLimitErrorKind::InvalidConfiguration`] for an invalid ID,
    /// [`RateLimitErrorKind::SchemaConflict`] when a schema is already
    /// registered, and [`RateLimitErrorKind::Internal`] after lock poisoning.
    pub fn mark_legacy_state(&self, id: &str) -> Result<(), RateLimitError> {
        let id = RateLimitId::new(id)?;
        let mut state = self.lock()?;
        if state.registrations.contains_key(&id) {
            return Err(schema_conflict());
        }
        if state.legacy.contains(&id) {
            return Ok(());
        }
        let occupied = state
            .registrations
            .len()
            .checked_add(state.legacy.len())
            .ok_or_else(internal_error)?;
        if occupied >= self.inner.registration_capacity {
            return Err(state_capacity());
        }
        state.legacy.insert(id);
        Ok(())
    }

    /// Installs an explicit operator schema lock, the coordinated migration
    /// protocol for legacy state or a planned registration.
    ///
    /// An identical existing registration is accepted unchanged.
    ///
    /// # Errors
    ///
    /// Returns [`RateLimitErrorKind::SchemaConflict`] when a different schema
    /// or algorithm is already registered under the ID,
    /// [`RateLimitErrorKind::StateCapacity`] when registration storage is
    /// full, and [`RateLimitErrorKind::Internal`] after lock poisoning.
    pub fn install_schema_lock(
        &self,
        algorithm_version: u32,
        schema: RatePolicySchema,
    ) -> Result<(), RateLimitError> {
        let mut state = self.lock()?;
        let id = schema.id().clone();
        if let Some(existing) = state.registrations.get(&id) {
            return if existing.algorithm_version == algorithm_version && existing.schema == schema {
                Ok(())
            } else {
                Err(schema_conflict())
            };
        }
        // Converting an already retained legacy identity occupies the same
        // slot; a new identity must fit beside every active schema and marker.
        let occupied = state
            .registrations
            .len()
            .checked_add(state.legacy.len())
            .ok_or_else(internal_error)?;
        if !state.legacy.contains(&id) && occupied >= self.inner.registration_capacity {
            return Err(state_capacity());
        }
        state.legacy.remove(&id);
        state.registrations.insert(
            id,
            Registration {
                algorithm_version,
                schema,
            },
        );
        Ok(())
    }

    /// Retires one policy ID: removes its registration, any legacy marker and
    /// all of its quota buckets. This is an explicit, reviewed state reset,
    /// never performed automatically.
    ///
    /// Returns whether anything was removed.
    ///
    /// # Errors
    ///
    /// Returns [`RateLimitErrorKind::Internal`] after lock poisoning.
    pub fn retire_schema(&self, id: &str) -> Result<bool, RateLimitError> {
        let mut state = self.lock()?;
        let registered = state
            .registrations
            .keys()
            .find(|key| key.as_str() == id)
            .cloned();
        let legacy = state.legacy.iter().find(|key| key.as_str() == id).cloned();
        let mut removed = false;
        if let Some(key) = registered {
            state.registrations.remove(&key);
            removed = true;
        }
        if let Some(key) = legacy {
            state.legacy.remove(&key);
            removed = true;
        }
        let before = state.buckets.len();
        state.buckets.retain(|key, _| key.id.as_str() != id);
        Ok(removed || state.buckets.len() != before)
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, StoreState>, RateLimitError> {
        if self.inner.failed.load(Ordering::Acquire) {
            return Err(internal_error());
        }
        self.inner.state.lock().map_err(|_| {
            self.inner.failed.store(true, Ordering::Release);
            internal_error()
        })
    }
}

/// One adapter instance attached to an [`InMemorySharedStore`].
///
/// Its capability is [`Enforcement::SharedPersistent`] with the store's
/// namespace and the instance's algorithm version. `acquire` performs all of
/// its work atomically on its first poll and never waits, so a future dropped
/// before that poll leaves no state and one dropped afterwards has either
/// committed completely or changed nothing. Response feedback is ignored.
#[derive(Clone)]
pub struct InMemorySharedLimiter {
    store: InMemorySharedStore,
    algorithm_version: u32,
}

impl InMemorySharedLimiter {
    fn admit(&self, request: &RateLimitRequest<'_>) -> Result<(), RateLimitError> {
        if request.capability != self.capability() {
            // Core must present the capability it snapshotted from this
            // adapter; anything else is a misconfigured runtime.
            return Err(invalid_configuration());
        }
        let plan = request.plan;
        if plan
            .policies()
            .iter()
            .any(|policy| policy.key().total_bytes() > DEFAULT_RATE_LIMIT_KEY_BYTES)
        {
            return Err(RateLimitError::new(RateLimitErrorKind::InvalidKey));
        }
        let now = tokio::time::Instant::now();
        let mut state = self.store.lock()?;
        let pending = self.check_schemas(&state, plan)?;
        let registrations = state
            .registrations
            .len()
            .checked_add(state.legacy.len())
            .and_then(|occupied| occupied.checked_add(pending.len()))
            .ok_or_else(internal_error)?;
        if registrations > self.store.inner.registration_capacity {
            return Err(state_capacity());
        }
        let updates = self.plan_quota(&mut state, plan, now)?;
        let admissions = state.admissions.checked_add(1).ok_or_else(internal_error)?;
        // Every check has passed: commit registrations, debits and the
        // admission count together.
        for schema in pending {
            state.registrations.insert(
                schema.id().clone(),
                Registration {
                    algorithm_version: self.algorithm_version,
                    schema,
                },
            );
        }
        for (key, window) in updates {
            state.buckets.insert(key, window);
        }
        state.admissions = admissions;
        Ok(())
    }

    /// Compares every policy schema with the registry without mutating it and
    /// returns the schemas that still need registration.
    fn check_schemas(
        &self,
        state: &StoreState,
        plan: &RateLimitPlan,
    ) -> Result<Vec<RatePolicySchema>, RateLimitError> {
        let mut pending: Vec<RatePolicySchema> = Vec::new();
        for policy in plan.policies() {
            let schema = policy.schema();
            match state.registrations.get(schema.id()) {
                Some(registration) => {
                    if registration.algorithm_version != self.algorithm_version
                        || registration.schema != *schema
                    {
                        return Err(schema_conflict());
                    }
                }
                None => {
                    if state.legacy.contains(schema.id()) {
                        return Err(schema_conflict());
                    }
                    if let Some(earlier) =
                        pending.iter().find(|earlier| earlier.id() == schema.id())
                    {
                        if earlier != schema {
                            return Err(schema_conflict());
                        }
                    } else {
                        pending.push(schema.clone());
                    }
                }
            }
        }
        Ok(pending)
    }

    /// Computes the complete post-admission window state without mutating
    /// buckets, pruning only fully elapsed buckets outside this plan when
    /// capacity requires it.
    fn plan_quota(
        &self,
        state: &mut StoreState,
        plan: &RateLimitPlan,
        now: tokio::time::Instant,
    ) -> Result<Vec<(BucketKey, WindowState)>, RateLimitError> {
        let mut costs = BTreeMap::<BucketKey, u64>::new();
        for policy in plan.policies() {
            let parts = policy
                .key()
                .parts()
                .iter()
                .map(|part| (Box::from(part.name()), Box::from(part.value())))
                .collect::<Vec<_>>();
            for window in policy.windows() {
                let key = BucketKey {
                    id: policy.id().clone(),
                    parts: parts.clone(),
                    duration: window.duration(),
                    capacity: window.capacity().get(),
                };
                let cost = costs.entry(key).or_default();
                *cost = cost
                    .checked_add(u64::from(policy.cost().get()))
                    .ok_or_else(internal_error)?;
            }
        }
        let missing = costs
            .keys()
            .filter(|key| !state.buckets.contains_key(*key))
            .count();
        let needed = state
            .buckets
            .len()
            .checked_add(missing)
            .ok_or_else(internal_error)?;
        if needed > self.store.inner.quota_capacity {
            // A fully elapsed window is indistinguishable from a fresh bucket,
            // so pruning it changes no admission outcome; active windows are
            // never evicted.
            state.buckets.retain(|key, window| {
                costs.contains_key(key) || !window_elapsed(window, key.duration, now)
            });
            let needed = state
                .buckets
                .len()
                .checked_add(missing)
                .ok_or_else(internal_error)?;
            if needed > self.store.inner.quota_capacity {
                return Err(state_capacity());
            }
        }
        let mut updates = Vec::with_capacity(costs.len());
        for (key, cost) in costs {
            let (start, used) = match state.buckets.get(&key) {
                Some(window) if !window_elapsed(window, key.duration, now) => {
                    (window.start, window.used)
                }
                _ => (now, 0),
            };
            let used = used.checked_add(cost).ok_or_else(internal_error)?;
            if used > u64::from(key.capacity) {
                return Err(RateLimitError::with_safe_message(
                    RateLimitErrorKind::Acquire,
                    "shared fixture quota exhausted",
                ));
            }
            updates.push((key, WindowState { start, used }));
        }
        Ok(updates)
    }
}

impl RateLimiter for InMemorySharedLimiter {
    fn capability(&self) -> LimiterCapability {
        LimiterCapability::new(
            Enforcement::SharedPersistent,
            self.store.inner.namespace,
            self.algorithm_version,
        )
    }

    fn acquire<'a>(
        &'a self,
        request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        // All work happens on the first poll, so a future dropped unpolled
        // leaves no registration or debit.
        Box::pin(async move { self.admit(&request).map(|()| RateLimitPermit::granted()) })
    }
}

fn window_elapsed(window: &WindowState, duration: Duration, now: tokio::time::Instant) -> bool {
    now.saturating_duration_since(window.start) >= duration
}

fn invalid_configuration() -> RateLimitError {
    RateLimitError::new(RateLimitErrorKind::InvalidConfiguration)
}

fn schema_conflict() -> RateLimitError {
    RateLimitError::new(RateLimitErrorKind::SchemaConflict)
}

fn state_capacity() -> RateLimitError {
    RateLimitError::new(RateLimitErrorKind::StateCapacity)
}

fn internal_error() -> RateLimitError {
    RateLimitError::new(RateLimitErrorKind::Internal)
}

#[cfg(test)]
mod tests;

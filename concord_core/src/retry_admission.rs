use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use url::{Host, Url};

pub(crate) const INITIAL_CREDITS: u8 = 5;
pub(crate) const MAX_CREDITS: u8 = 10;
pub(crate) const RETRY_COST: u8 = 5;
pub(crate) const DEFAULT_MAX_ENTRIES: usize = 4096;
pub(crate) const DEFAULT_IDLE_TTL: Duration = Duration::from_secs(15 * 60);

#[derive(Clone, Eq, Hash, PartialEq)]
pub(crate) struct OriginKey {
    scheme: String,
    host: String,
    port: u16,
}

impl OriginKey {
    pub(crate) fn from_url(url: &Url) -> Result<Self, ()> {
        let scheme = url.scheme().to_ascii_lowercase();
        if !matches!(scheme.as_str(), "http" | "https") {
            return Err(());
        }
        let host = match url.host() {
            Some(Host::Domain(host)) => format!("domain:{}", host.to_ascii_lowercase()),
            Some(Host::Ipv4(host)) => format!("ipv4:{host}"),
            Some(Host::Ipv6(host)) => format!("ipv6:[{host}]"),
            None => return Err(()),
        };
        let port = url.port_or_known_default().ok_or(())?;
        Ok(Self { scheme, host, port })
    }
}

impl fmt::Debug for OriginKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OriginKey")
            .field("scheme", &"<redacted>")
            .field("host", &"<redacted>")
            .field("port", &self.port)
            .finish()
    }
}

trait AdmissionClock: Send + Sync {
    fn now(&self) -> Instant;
}

struct MonotonicClock;

impl AdmissionClock for MonotonicClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

struct Entry {
    credits: u8,
    last_activity: Instant,
    active_requests: usize,
    active_permits: usize,
}

struct RegistryState {
    entries: HashMap<OriginKey, Entry>,
}

struct RegistryInner {
    state: Mutex<RegistryState>,
    max_entries: usize,
    idle_ttl: Duration,
    clock: Arc<dyn AdmissionClock>,
}

/// Advanced retry-admission registry configuration.
///
/// Normal clients use the process-wide default registry. Constructing a
/// private registry is intended for deterministic tests and specialized
/// embedding scenarios.
#[derive(Clone)]
pub struct RetryAdmissionRegistry {
    inner: Arc<RegistryInner>,
}

impl RetryAdmissionRegistry {
    pub(crate) fn global() -> Self {
        static GLOBAL: OnceLock<RetryAdmissionRegistry> = OnceLock::new();
        GLOBAL
            .get_or_init(|| Self::new(DEFAULT_MAX_ENTRIES, DEFAULT_IDLE_TTL))
            .clone()
    }

    pub fn new(max_entries: usize, idle_ttl: Duration) -> Self {
        Self::with_clock(max_entries, idle_ttl, Arc::new(MonotonicClock))
    }

    fn with_clock(max_entries: usize, idle_ttl: Duration, clock: Arc<dyn AdmissionClock>) -> Self {
        Self {
            inner: Arc::new(RegistryInner {
                state: Mutex::new(RegistryState {
                    entries: HashMap::new(),
                }),
                max_entries,
                idle_ttl,
                clock,
            }),
        }
    }

    fn lock_state(&self) -> Option<std::sync::MutexGuard<'_, RegistryState>> {
        // A poisoned registry is not trustworthy enough to recover credits or
        // entry activity. Fail closed for admission and leave original sends
        // untracked instead of panicking or resetting the reserve.
        self.inner.state.lock().ok()
    }

    #[cfg(test)]
    pub(crate) fn active_requests_for(&self, key: &OriginKey) -> usize {
        self.lock_state()
            .and_then(|state| state.entries.get(key).map(|entry| entry.active_requests))
            .unwrap_or_default()
    }

    pub(crate) fn track(&self, key: OriginKey) -> OriginHandle {
        let now = self.inner.clock.now();
        let Some(mut state) = self.lock_state() else {
            return OriginHandle {
                registry: self.clone(),
                key,
                tracked: false,
            };
        };
        cleanup_expired(&mut state, now, self.inner.idle_ttl);
        let tracked = if let Some(entry) = state.entries.get_mut(&key) {
            entry.active_requests += 1;
            entry.last_activity = now;
            true
        } else if state.entries.len() < self.inner.max_entries {
            state.entries.insert(
                key.clone(),
                Entry {
                    credits: INITIAL_CREDITS,
                    last_activity: now,
                    active_requests: 1,
                    active_permits: 0,
                },
            );
            true
        } else {
            false
        };
        OriginHandle {
            registry: self.clone(),
            key,
            tracked,
        }
    }

    fn reserve(&self, key: &OriginKey) -> Option<AdmissionPermit> {
        let now = self.inner.clock.now();
        let mut state = self.lock_state()?;
        let entry = state.entries.get_mut(key)?;
        if entry.credits < RETRY_COST {
            return None;
        }
        entry.credits -= RETRY_COST;
        entry.active_permits += 1;
        entry.last_activity = now;
        Some(AdmissionPermit {
            registry: self.clone(),
            key: key.clone(),
            committed: false,
        })
    }

    fn finish_permit(&self, key: &OriginKey, committed: bool) {
        let now = self.inner.clock.now();
        let Some(mut state) = self.lock_state() else {
            return;
        };
        let Some(entry) = state.entries.get_mut(key) else {
            return;
        };
        entry.active_permits = entry.active_permits.saturating_sub(1);
        if !committed {
            entry.credits = entry.credits.saturating_add(RETRY_COST).min(MAX_CREDITS);
        }
        entry.last_activity = now;
    }

    fn deposit_original(&self, key: &OriginKey) {
        let now = self.inner.clock.now();
        let Some(mut state) = self.lock_state() else {
            return;
        };
        if let Some(entry) = state.entries.get_mut(key) {
            entry.credits = entry.credits.saturating_add(1).min(MAX_CREDITS);
            entry.last_activity = now;
        }
    }
}

pub(crate) struct OriginHandle {
    registry: RetryAdmissionRegistry,
    key: OriginKey,
    tracked: bool,
}

impl OriginHandle {
    pub(crate) fn reserve(&self) -> Option<AdmissionPermit> {
        self.tracked.then(|| self.registry.reserve(&self.key))?
    }

    pub(crate) fn deposit_original(&self) {
        if self.tracked {
            self.registry.deposit_original(&self.key);
        }
    }
}

impl Drop for OriginHandle {
    fn drop(&mut self) {
        if !self.tracked {
            return;
        }
        let now = self.registry.inner.clock.now();
        let Some(mut state) = self.registry.lock_state() else {
            return;
        };
        if let Some(entry) = state.entries.get_mut(&self.key) {
            entry.active_requests = entry.active_requests.saturating_sub(1);
            entry.last_activity = now;
        }
    }
}

pub(crate) struct AdmissionPermit {
    registry: RetryAdmissionRegistry,
    key: OriginKey,
    committed: bool,
}

impl AdmissionPermit {
    pub(crate) fn commit(&mut self) {
        self.committed = true;
    }
}

impl Drop for AdmissionPermit {
    fn drop(&mut self) {
        self.registry.finish_permit(&self.key, self.committed);
    }
}

fn cleanup_expired(state: &mut RegistryState, now: Instant, idle_ttl: Duration) {
    state.entries.retain(|_, entry| {
        entry.active_requests != 0
            || entry.active_permits != 0
            || now.duration_since(entry.last_activity) < idle_ttl
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct TestClock {
        now: Mutex<Instant>,
    }

    impl TestClock {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                now: Mutex::new(Instant::now()),
            })
        }

        fn advance(&self, duration: Duration) {
            *self.now.lock().unwrap() += duration;
        }
    }

    impl AdmissionClock for TestClock {
        fn now(&self) -> Instant {
            *self.now.lock().unwrap()
        }
    }

    fn key(value: &str) -> OriginKey {
        OriginKey::from_url(&value.parse().unwrap()).unwrap()
    }

    fn can_track(registry: &RetryAdmissionRegistry, origin: &str) -> bool {
        registry.track(key(origin)).tracked
    }

    fn credits(registry: &RetryAdmissionRegistry, key: &OriginKey) -> u8 {
        registry
            .inner
            .state
            .lock()
            .unwrap()
            .entries
            .get(key)
            .unwrap()
            .credits
    }

    #[test]
    fn exact_integer_credit_arithmetic() {
        let clock = TestClock::new();
        let registry = RetryAdmissionRegistry::with_clock(4, Duration::from_secs(60), clock);
        let origin = key("https://example.com:443/path");
        let handle = registry.track(origin.clone());
        assert_eq!(credits(&registry, &origin), 5);
        handle.deposit_original();
        assert_eq!(credits(&registry, &origin), 6);
        let mut permit = handle.reserve().unwrap();
        assert_eq!(credits(&registry, &origin), 1);
        permit.commit();
        drop(permit);
        handle.deposit_original();
        handle.deposit_original();
        handle.deposit_original();
        handle.deposit_original();
        assert_eq!(credits(&registry, &origin), 5);
        for _ in 0..10 {
            handle.deposit_original();
        }
        assert_eq!(credits(&registry, &origin), 10);
    }

    #[test]
    fn uncommitted_permit_refunds_and_committed_permit_does_not() {
        let registry = RetryAdmissionRegistry::new(4, Duration::from_secs(60));
        let origin = key("http://example.com");
        let handle = registry.track(origin.clone());
        handle.deposit_original();
        let permit = handle.reserve().unwrap();
        assert_eq!(credits(&registry, &origin), 1);
        drop(permit);
        assert_eq!(credits(&registry, &origin), 6);
        let mut permit = handle.reserve().unwrap();
        permit.commit();
        drop(permit);
        assert_eq!(credits(&registry, &origin), 1);
    }

    #[test]
    fn origin_normalization_uses_scheme_host_and_effective_port() {
        assert_eq!(
            key("https://EXAMPLE.com/path?a=1"),
            key("HTTPS://example.com:443/other")
        );
        assert_ne!(key("https://example.com:8443"), key("https://example.com"));
        assert_ne!(key("http://example.com"), key("https://example.com"));
        assert_ne!(key("https://127.0.0.1"), key("https://[::1]"));
    }

    #[test]
    fn cleanup_is_opportunistic_and_preserves_active_entries() {
        let clock = TestClock::new();
        let registry = RetryAdmissionRegistry::with_clock(1, Duration::from_secs(5), clock.clone());
        let origin = key("https://one.example");
        let handle = registry.track(origin.clone());
        clock.advance(Duration::from_secs(10));
        let second = registry.track(key("https://two.example"));
        assert!(!second.tracked);
        drop(handle);
        let first = registry.track(origin);
        assert!(first.tracked);
    }

    #[test]
    fn cleanup_preserves_an_entry_holding_a_live_permit() {
        let clock = TestClock::new();
        let registry = RetryAdmissionRegistry::with_clock(1, Duration::from_secs(5), clock.clone());
        let origin = key("https://permit.example");
        let handle = registry.track(origin);
        let permit = handle.reserve().expect("initial reserve is available");
        drop(handle);
        clock.advance(Duration::from_secs(10));

        let second = registry.track(key("https://two.example"));
        assert!(!second.tracked);
        drop(permit);
        clock.advance(Duration::from_secs(10));

        let third = registry.track(key("https://three.example"));
        assert!(third.tracked);
    }

    #[test]
    fn origin_handle_lease_survives_until_drop() {
        let clock = TestClock::new();
        let registry = RetryAdmissionRegistry::with_clock(1, Duration::ZERO, clock);
        let origin = key("https://held.example");
        let other = "https://other.example";

        let handle = registry.track(origin);
        assert!(!can_track(&registry, other));
        drop(handle);
        assert!(can_track(&registry, other));
    }

    #[test]
    fn origin_handle_lifecycle_does_not_deposit_again() {
        let registry = RetryAdmissionRegistry::new(1, Duration::from_secs(60));
        let origin = key("https://single-deposit.example");
        let handle = registry.track(origin.clone());
        handle.deposit_original();
        assert_eq!(credits(&registry, &origin), 6);

        drop(handle);
        assert_eq!(credits(&registry, &origin), 6);
    }

    #[test]
    fn concurrent_reservation_spends_the_last_five_credits_once() {
        let registry = RetryAdmissionRegistry::new(1, Duration::from_secs(60));
        let handle = Arc::new(registry.track(key("https://concurrent.example")));
        let left = {
            let handle = handle.clone();
            std::thread::spawn(move || handle.reserve())
        };
        let right = {
            let handle = handle.clone();
            std::thread::spawn(move || handle.reserve())
        };

        let left = left.join().unwrap();
        let right = right.join().unwrap();
        assert_eq!(u8::from(left.is_some()) + u8::from(right.is_some()), 1);
    }

    #[test]
    fn full_registry_allows_untracked_original_but_not_reservation() {
        let registry = RetryAdmissionRegistry::new(1, Duration::from_secs(60));
        let first = registry.track(key("https://one.example"));
        let second = registry.track(key("https://two.example"));
        assert!(!second.tracked);
        assert!(second.reserve().is_none());
        assert!(first.reserve().is_some());
    }

    #[test]
    fn constants_are_integer_exact_and_bounded() {
        static ASSERT: AtomicU64 = AtomicU64::new(0);
        ASSERT.store(
            u64::from(INITIAL_CREDITS) + u64::from(MAX_CREDITS) + u64::from(RETRY_COST),
            Ordering::Relaxed,
        );
        assert_eq!(ASSERT.load(Ordering::Relaxed), 20);
    }

    #[test]
    fn poisoned_registry_fails_closed_without_panicking() {
        let registry = RetryAdmissionRegistry::new(1, Duration::from_secs(60));
        let poisoned = registry.clone();
        let result = std::thread::spawn(move || {
            let _guard = poisoned.inner.state.lock().unwrap();
            panic!("poison test");
        })
        .join();
        assert!(result.is_err());

        let handle = registry.track(key("https://poisoned.example"));
        assert!(!handle.tracked);
        assert!(handle.reserve().is_none());
        handle.deposit_original();
        drop(handle);
    }
}

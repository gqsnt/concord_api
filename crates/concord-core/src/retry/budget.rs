use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::Mutex;
use std::time::Duration;

use tokio::time::Instant;
use url::{Host, Url};

const WINDOW_BUCKETS: usize = 10;
const RATIO_SCALE: u64 = 10_000_000_000_000_000;

#[derive(Clone, Copy)]
pub(super) struct FixedRatio {
    units: u64,
}

impl FixedRatio {
    pub(super) fn from_f64(value: f64) -> Option<Self> {
        if !value.is_finite() || !(0.0..=1_000.0).contains(&value) {
            return None;
        }
        let scaled = value * RATIO_SCALE as f64;
        if !scaled.is_finite() || !(0.0..=10_000_000_000_000_000_000.0).contains(&scaled) {
            return None;
        }
        let rounded = scaled.round() as u64;
        Some(Self {
            units: if value > 0.0 { rounded.max(1) } else { 0 },
        })
    }

    pub(super) fn allowed_retries(self, initial_dispatches: u64) -> Option<u128> {
        if self.units == 0 || initial_dispatches == 0 {
            return Some(0);
        }
        let product = u128::from(initial_dispatches).checked_mul(u128::from(self.units))?;
        product
            .checked_add(u128::from(RATIO_SCALE - 1))
            .map(|rounded| rounded / u128::from(RATIO_SCALE))
    }

    #[cfg(test)]
    pub(super) const fn units(self) -> u64 {
        self.units
    }
}

pub(super) struct RetryBudget {
    ratio: FixedRatio,
    scope_capacity: usize,
    scope_ttl: Duration,
    state: Mutex<BudgetState>,
}

impl RetryBudget {
    pub(super) fn new(ratio: FixedRatio, scope_capacity: usize, scope_ttl: Duration) -> Self {
        Self {
            ratio,
            scope_capacity,
            scope_ttl,
            state: Mutex::new(BudgetState::default()),
        }
    }

    pub(super) fn record_initial_dispatch(&self, url: &Url) {
        let Some(scope_key) = OriginScopeKey::from_url(url) else {
            return;
        };
        let now = Instant::now();
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        let Some(scope) = state.scope_for_use(scope_key, now, self.scope_capacity, self.scope_ttl)
        else {
            return;
        };
        scope.window.record_initial(now);
    }

    pub(super) fn reserve_retry(&self, url: &Url) -> bool {
        let Some(scope_key) = OriginScopeKey::from_url(url) else {
            return false;
        };
        let now = Instant::now();
        let Ok(mut state) = self.state.lock() else {
            return false;
        };
        let Some(scope) = state.scope_for_use(scope_key, now, self.scope_capacity, self.scope_ttl)
        else {
            return false;
        };
        scope.window.reserve_retry(now, self.ratio)
    }

    #[cfg(test)]
    pub(crate) fn poison_state_for_test(&self) -> bool {
        std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    let _guard = self.state.lock().expect("retry budget test lock");
                    panic!("intentional retry budget test poison");
                })
                .join()
                .is_err()
        })
    }

    #[cfg(test)]
    pub(super) fn tracked_scope_count(&self) -> usize {
        self.state.lock().map_or(0, |state| state.scopes.len())
    }

    #[cfg(test)]
    pub(super) const fn scope_capacity(&self) -> usize {
        self.scope_capacity
    }

    #[cfg(test)]
    pub(super) const fn scope_ttl(&self) -> Duration {
        self.scope_ttl
    }

    #[cfg(test)]
    pub(super) const fn ratio(&self) -> FixedRatio {
        self.ratio
    }

    #[cfg(test)]
    pub(super) fn active_bucket_count(&self, url: &Url) -> usize {
        let Some(key) = OriginScopeKey::from_url(url) else {
            return 0;
        };
        self.state
            .lock()
            .ok()
            .and_then(|state| {
                state
                    .scopes
                    .get(&key)
                    .map(|scope| scope.window.active_len())
            })
            .unwrap_or(0)
    }
}

#[derive(Default)]
struct BudgetState {
    scopes: HashMap<OriginScopeKey, ScopeState>,
}

impl BudgetState {
    fn scope_for_use(
        &mut self,
        key: OriginScopeKey,
        now: Instant,
        capacity: usize,
        ttl: Duration,
    ) -> Option<&mut ScopeState> {
        if self.scopes.contains_key(&key) {
            let scope = self
                .scopes
                .get_mut(&key)
                .expect("a contained retry scope remains present");
            if is_idle_expired(now, scope.last_used, ttl) {
                *scope = ScopeState::new(now);
            } else {
                scope.last_used = now;
            }
            return Some(scope);
        }

        self.scopes
            .retain(|_, scope| !is_idle_expired(now, scope.last_used, ttl));
        if self.scopes.len() >= capacity {
            return None;
        }
        self.scopes.insert(key.clone(), ScopeState::new(now));
        self.scopes.get_mut(&key)
    }
}

fn is_idle_expired(now: Instant, last_used: Instant, ttl: Duration) -> bool {
    now.saturating_duration_since(last_used) >= ttl
}

struct ScopeState {
    window: RollingWindow,
    last_used: Instant,
}

impl ScopeState {
    fn new(now: Instant) -> Self {
        Self {
            window: RollingWindow::new(now),
            last_used: now,
        }
    }
}

struct RollingWindow {
    buckets: [WindowBucket; WINDOW_BUCKETS],
    current_index: usize,
    current_started: Instant,
}

impl RollingWindow {
    fn new(now: Instant) -> Self {
        Self {
            buckets: [WindowBucket::EMPTY; WINDOW_BUCKETS],
            current_index: 0,
            current_started: now,
        }
    }

    fn record_initial(&mut self, now: Instant) {
        self.rotate(now);
        self.buckets[self.current_index].increment_initial();
    }

    fn reserve_retry(&mut self, now: Instant, ratio: FixedRatio) -> bool {
        self.rotate(now);
        let Some((initial_dispatches, approved_retries)) = self.totals() else {
            return false;
        };
        let Some(allowed_retries) = ratio.allowed_retries(initial_dispatches) else {
            return false;
        };
        if u128::from(approved_retries) >= allowed_retries {
            return false;
        }
        self.buckets[self.current_index].increment_retry()
    }

    fn rotate(&mut self, now: Instant) {
        let elapsed = now.saturating_duration_since(self.current_started);
        let elapsed_buckets = elapsed.as_secs();
        if elapsed_buckets == 0 {
            return;
        }

        if elapsed_buckets >= WINDOW_BUCKETS as u64 {
            self.buckets.fill(WindowBucket::EMPTY);
            self.current_index = 0;
        } else {
            for _ in 0..elapsed_buckets {
                self.current_index = (self.current_index + 1) % WINDOW_BUCKETS;
                self.buckets[self.current_index] = WindowBucket::EMPTY;
            }
        }

        let complete_width = Duration::from_secs(elapsed_buckets);
        let remainder = elapsed.checked_sub(complete_width).unwrap_or_default();
        self.current_started = now.checked_sub(remainder).unwrap_or(now);
    }

    fn totals(&self) -> Option<(u64, u64)> {
        self.buckets
            .iter()
            .try_fold((0_u64, 0_u64), |(initial, retries), bucket| {
                if bucket.overflowed {
                    return None;
                }
                Some((
                    initial.checked_add(bucket.initial_dispatches)?,
                    retries.checked_add(bucket.approved_retries)?,
                ))
            })
    }

    #[cfg(test)]
    fn active_len(&self) -> usize {
        self.buckets
            .iter()
            .filter(|bucket| {
                bucket.initial_dispatches != 0 || bucket.approved_retries != 0 || bucket.overflowed
            })
            .count()
    }
}

#[derive(Clone, Copy)]
struct WindowBucket {
    initial_dispatches: u64,
    approved_retries: u64,
    overflowed: bool,
}

impl WindowBucket {
    const EMPTY: Self = Self {
        initial_dispatches: 0,
        approved_retries: 0,
        overflowed: false,
    };

    fn increment_initial(&mut self) {
        let Some(next) = self.initial_dispatches.checked_add(1) else {
            self.overflowed = true;
            return;
        };
        self.initial_dispatches = next;
    }

    fn increment_retry(&mut self) -> bool {
        let Some(next) = self.approved_retries.checked_add(1) else {
            self.overflowed = true;
            return false;
        };
        self.approved_retries = next;
        true
    }
}

#[derive(Clone, Eq, Hash, PartialEq)]
struct OriginScopeKey {
    scheme: Box<str>,
    host: OriginHost,
    port: u16,
}

impl OriginScopeKey {
    fn from_url(url: &Url) -> Option<Self> {
        let host = match url.host()? {
            Host::Domain(domain) => OriginHost::Domain(domain.into()),
            Host::Ipv4(address) => OriginHost::Ipv4(address),
            Host::Ipv6(address) => OriginHost::Ipv6(address),
        };
        Some(Self {
            scheme: url.scheme().into(),
            host,
            port: url.port_or_known_default()?,
        })
    }
}

#[derive(Clone, Eq, Hash, PartialEq)]
enum OriginHost {
    Domain(Box<str>),
    Ipv4(Ipv4Addr),
    Ipv6(Ipv6Addr),
}

#[cfg(test)]
mod tests {
    use std::panic;
    use std::sync::{Arc, PoisonError};

    use super::*;

    fn retained_totals(budget: &RetryBudget, url: &Url) -> (u64, u64) {
        let key = OriginScopeKey::from_url(url).expect("test URL has an origin");
        let state = budget.state.lock().unwrap_or_else(PoisonError::into_inner);
        state
            .scopes
            .get(&key)
            .and_then(|scope| scope.window.totals())
            .expect("retained test scope has valid counters")
    }

    #[test]
    fn poisoned_budget_state_fails_closed_without_granting_retry() {
        let budget = Arc::new(RetryBudget::new(
            FixedRatio::from_f64(1.0).expect("valid test ratio"),
            1,
            Duration::from_secs(60),
        ));
        let target = Url::parse("https://poisoned-budget.example.test/data").expect("test URL");
        budget.record_initial_dispatch(&target);
        assert_eq!(retained_totals(&budget, &target), (1, 0));

        assert!(budget.poison_state_for_test());
        assert!(budget.state.is_poisoned());

        for _ in 0..3 {
            let recorded = panic::catch_unwind(|| budget.record_initial_dispatch(&target));
            assert!(matches!(recorded, Ok(())));

            let reservation = panic::catch_unwind(|| budget.reserve_retry(&target));
            assert_eq!(reservation.ok(), Some(false));
            assert!(budget.state.is_poisoned());
            assert_eq!(retained_totals(&budget, &target), (1, 0));
        }
    }
}

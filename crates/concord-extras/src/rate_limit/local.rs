use std::collections::{BTreeMap, BTreeSet};
#[cfg(test)]
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use concord_core::contracts::{BoxFuture, RateLimitErrorKind};
use concord_core::rate_limit::{
    Cooldown, CooldownScope, DEFAULT_RATE_LIMIT_KEY_BYTES, MAX_RATE_LIMIT_KEY_BYTES,
    RateLimitError, RateLimitObserver, RateLimitPermit, RateLimitPlan, RateLimitRequest,
    RateLimitResponse, RateLimitResponseHead, RateLimitRule, RateLimitWindow, RateLimiter,
};

use super::RetryAfterObserver;

const DEFAULT_QUOTA_CAPACITY: usize = 4_096;
const DEFAULT_COOLDOWN_CAPACITY: usize = 4_096;
const DEFAULT_IDLE_TTL: Duration = Duration::from_secs(15 * 60);
const NANOS_PER_SECOND: u128 = 1_000_000_000;

/// A bounded process-local, limiter-object-local token-bucket limiter.
///
/// New local limiters hold at most 4,096 quota entries and 4,096 cooldown entries,
/// prune fully replenished quota entries after 15 idle minutes, and accept
/// resolved keys up to 1,024 bytes. Builder methods can replace each bound.
///
/// Capacity pressure fails with [`RateLimitErrorKind::StateCapacity`] rather
/// than evicting active quota or cooldown history. Use an application-supplied
/// custom limiter when distributed or exact remote-policy enforcement is
/// required.
///
/// Internal quota and cooldown state is not exposed:
///
/// ```compile_fail
/// use concord_extras::rate_limit::LocalRateLimiter;
///
/// let limiter = LocalRateLimiter::new();
/// let _ = limiter.state;
/// ```
pub struct LocalRateLimiter {
    config: LocalLimiterConfig,
    observer: Arc<dyn RateLimitObserver>,
    state: Mutex<LocalLimiterState>,
    internal_failed: AtomicBool,
    #[cfg(test)]
    acquisition_calls: AtomicUsize,
}

struct LocalLimiterConfig {
    quota_capacity: usize,
    cooldown_capacity: usize,
    idle_ttl: Duration,
    key_limit: usize,
}

#[allow(
    clippy::new_without_default,
    reason = "the normative contract specifies an explicit constructor without a Default contract"
)]
impl LocalRateLimiter {
    /// Creates an independent local limiter with the normative finite defaults.
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: LocalLimiterConfig {
                quota_capacity: DEFAULT_QUOTA_CAPACITY,
                cooldown_capacity: DEFAULT_COOLDOWN_CAPACITY,
                idle_ttl: DEFAULT_IDLE_TTL,
                key_limit: DEFAULT_RATE_LIMIT_KEY_BYTES,
            },
            observer: Arc::new(RetryAfterObserver),
            state: Mutex::new(LocalLimiterState::new()),
            internal_failed: AtomicBool::new(false),
            #[cfg(test)]
            acquisition_calls: AtomicUsize::new(0),
        }
    }

    /// Replaces the built-in [`RetryAfterObserver`].
    ///
    /// The custom observer must delegate to it when built-in `Retry-After`
    /// behavior should be retained.
    #[must_use]
    pub fn observer(mut self, observer: Arc<dyn RateLimitObserver>) -> Self {
        self.observer = observer;
        self
    }

    /// Sets the positive finite quota-entry capacity.
    pub fn quota_capacity(mut self, entries: usize) -> Result<Self, RateLimitError> {
        if entries == 0 {
            return Err(invalid_configuration());
        }
        self.config.quota_capacity = entries;
        Ok(self)
    }

    /// Sets the positive finite cooldown-entry capacity.
    pub fn cooldown_capacity(mut self, entries: usize) -> Result<Self, RateLimitError> {
        if entries == 0 {
            return Err(invalid_configuration());
        }
        self.config.cooldown_capacity = entries;
        Ok(self)
    }

    /// Sets the positive finite idle quota-entry lifetime.
    pub fn idle_ttl(mut self, duration: Duration) -> Result<Self, RateLimitError> {
        if duration.is_zero() {
            return Err(invalid_configuration());
        }
        self.config.idle_ttl = duration;
        Ok(self)
    }

    /// Sets the maximum aggregate byte length of one resolved key.
    ///
    /// Zero permits only a global key. Values above
    /// [`MAX_RATE_LIMIT_KEY_BYTES`] are rejected.
    pub fn key_limit(mut self, bytes: usize) -> Result<Self, RateLimitError> {
        if bytes > MAX_RATE_LIMIT_KEY_BYTES {
            return Err(invalid_configuration());
        }
        self.config.key_limit = bytes;
        Ok(self)
    }

    #[cfg(test)]
    pub(crate) fn test_acquisition_count(&self) -> usize {
        self.acquisition_calls.load(AtomicOrdering::SeqCst)
    }

    #[cfg(test)]
    pub(crate) fn test_quota_entry_count(&self) -> usize {
        self.state.lock().expect("local limiter state").quota.len()
    }

    fn acquire_loop<'a>(
        &'a self,
        request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        Box::pin(async move {
            if self.internal_failed.load(AtomicOrdering::Acquire) {
                return Err(internal_error());
            }
            let contributions = Contributions::from_plan(request.plan, self.config.key_limit)
                .map_err(|error| self.record_failure(error))?;
            let rule_keys = RuleKeys::from_plan(request.plan);
            loop {
                let now = tokio::time::Instant::now();
                let evaluation = match self.state.lock() {
                    Ok(mut state) => {
                        let result = (|| {
                            state.prune_expired_cooldowns(now);
                            state.prune_quota(now, self.config.idle_ttl, &contributions)?;
                            state.evaluate(
                                now,
                                request.host,
                                &rule_keys,
                                &contributions,
                                self.config.quota_capacity,
                            )
                        })();
                        result.map_err(|error| self.record_failure(error))?
                    }
                    Err(_) => return Err(self.fail_internal()),
                };
                match evaluation {
                    Evaluation::Granted => return Ok(RateLimitPermit::granted()),
                    Evaluation::Wait(duration) => {
                        let wake = now
                            .checked_add(duration)
                            .ok_or_else(|| self.fail_internal())?;
                        tokio::time::sleep_until(wake).await;
                    }
                }
                if self.internal_failed.load(AtomicOrdering::Acquire) {
                    return Err(internal_error());
                }
            }
        })
    }

    fn observe_response(&self, response: RateLimitResponse<'_>) -> Result<(), RateLimitError> {
        if self.internal_failed.load(AtomicOrdering::Acquire) {
            return Err(internal_error());
        }
        let RateLimitResponse {
            request,
            status,
            headers,
            cooldown_cap,
        } = response;
        let RateLimitRequest {
            endpoint,
            method,
            safe_url,
            host,
            page_index,
            plan,
        } = request;
        let response_head = RateLimitResponseHead {
            endpoint,
            method,
            safe_url,
            host,
            page_index,
            plan,
            status,
            headers,
        };
        let Some(cooldown) = self.observer.observe(&response_head) else {
            return Ok(());
        };
        self.apply_cooldown(host, plan, cooldown, cooldown_cap)
    }

    fn apply_cooldown(
        &self,
        host: Option<&str>,
        plan: &RateLimitPlan,
        cooldown: Cooldown,
        cap: Duration,
    ) -> Result<(), RateLimitError> {
        let duration = cooldown.duration.min(cap);
        if duration.is_zero() {
            return Ok(());
        }
        if self.internal_failed.load(AtomicOrdering::Acquire) {
            return Err(internal_error());
        }
        let now = tokio::time::Instant::now();
        let expiration = match now.checked_add(duration) {
            Some(expiration) => expiration,
            None => return Err(self.fail_internal()),
        };
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(_) => return Err(self.fail_internal()),
        };
        state.prune_expired_cooldowns(now);
        let result = match cooldown.scope {
            CooldownScope::Client => {
                state.extend_client_cooldown(expiration);
                Ok(())
            }
            CooldownScope::Host => state.extend_cooldowns(
                host.map(|host| CooldownKey::Host(host.to_owned())),
                expiration,
                self.config.cooldown_capacity,
            ),
            CooldownScope::CurrentBuckets => {
                if plan.is_empty() {
                    state.extend_cooldowns(
                        host.map(|host| CooldownKey::Host(host.to_owned())),
                        expiration,
                        self.config.cooldown_capacity,
                    )
                } else {
                    let rule_keys = RuleKeys::from_plan(plan);
                    state.extend_cooldowns(
                        rule_keys.0.into_iter().map(CooldownKey::Bucket),
                        expiration,
                        self.config.cooldown_capacity,
                    )
                }
            }
        };
        result.map_err(|error| self.record_failure(error))
    }

    fn record_failure(&self, error: RateLimitError) -> RateLimitError {
        if error.kind() == RateLimitErrorKind::Internal {
            self.internal_failed.store(true, AtomicOrdering::Release);
        }
        error
    }

    fn fail_internal(&self) -> RateLimitError {
        self.record_failure(internal_error())
    }
}

impl RateLimiter for LocalRateLimiter {
    fn preparation_key_limit(&self) -> usize {
        self.config.key_limit
    }

    fn acquire<'a>(
        &'a self,
        request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        #[cfg(test)]
        self.acquisition_calls.fetch_add(1, AtomicOrdering::SeqCst);
        self.acquire_loop(request)
    }

    fn observe<'a>(
        &'a self,
        response: RateLimitResponse<'a>,
    ) -> BoxFuture<'a, Result<(), RateLimitError>> {
        Box::pin(async move { self.observe_response(response) })
    }
}

struct Contributions(BTreeMap<BucketKey, u128>);

impl Contributions {
    fn from_plan(plan: &RateLimitPlan, key_limit: usize) -> Result<Self, RateLimitError> {
        if key_limit > MAX_RATE_LIMIT_KEY_BYTES {
            return Err(invalid_configuration());
        }
        if plan
            .rules()
            .iter()
            .any(|rule| rule.key().total_bytes() > key_limit)
        {
            return Err(RateLimitError::new(RateLimitErrorKind::InvalidKey));
        }
        let mut contributions = BTreeMap::<BucketKey, u128>::new();
        for rule in plan.rules() {
            let rule_key = RuleKey::new(rule);
            for window in rule.windows() {
                let key = BucketKey::new(rule_key.clone(), window);
                let cost = contributions.entry(key).or_default();
                *cost = cost
                    .checked_add(u128::from(rule.cost().get()))
                    .ok_or_else(internal_error)?;
            }
        }
        if contributions
            .iter()
            .any(|(key, cost)| *cost > u128::from(key.capacity))
        {
            return Err(invalid_configuration());
        }
        Ok(Self(contributions))
    }
}

struct RuleKeys(BTreeSet<RuleKey>);

impl RuleKeys {
    fn from_plan(plan: &RateLimitPlan) -> Self {
        let keys = plan.rules().iter().map(RuleKey::new).collect();
        Self(keys)
    }
}

#[derive(Clone, Eq, Ord, PartialEq, PartialOrd)]
struct RuleKey {
    id: Arc<str>,
    parts: Arc<[(Arc<str>, Arc<str>)]>,
}

impl RuleKey {
    fn new(rule: &RateLimitRule) -> Self {
        Self {
            id: rule.id().as_str().into(),
            parts: Arc::from(
                rule.key()
                    .parts()
                    .iter()
                    .map(|part| (Arc::from(part.name()), Arc::from(part.value())))
                    .collect::<Vec<_>>()
                    .into_boxed_slice(),
            ),
        }
    }
}

#[derive(Clone, Eq, Ord, PartialEq, PartialOrd)]
struct BucketKey {
    rule: RuleKey,
    capacity: u32,
    duration: Duration,
}

impl BucketKey {
    fn new(rule: RuleKey, window: &RateLimitWindow) -> Self {
        Self {
            rule,
            capacity: window.capacity().get(),
            duration: window.duration(),
        }
    }

    fn full_balance(&self) -> Result<u128, RateLimitError> {
        self.duration
            .as_nanos()
            .checked_mul(u128::from(self.capacity))
            .ok_or_else(internal_error)
    }

    fn required_balance(&self, cost: u128) -> Result<u128, RateLimitError> {
        self.duration
            .as_nanos()
            .checked_mul(cost)
            .ok_or_else(internal_error)
    }
}

#[derive(Eq, Ord, PartialEq, PartialOrd)]
enum CooldownKey {
    Host(String),
    Bucket(RuleKey),
}

struct LocalLimiterState {
    quota: BTreeMap<BucketKey, QuotaEntry>,
    cooldowns: BTreeMap<CooldownKey, CooldownEntry>,
    client_cooldown: Option<tokio::time::Instant>,
}

impl LocalLimiterState {
    const fn new() -> Self {
        Self {
            quota: BTreeMap::new(),
            cooldowns: BTreeMap::new(),
            client_cooldown: None,
        }
    }

    fn prune_expired_cooldowns(&mut self, now: tokio::time::Instant) {
        if self
            .client_cooldown
            .is_some_and(|expiration| expiration <= now)
        {
            self.client_cooldown = None;
        }
        self.cooldowns.retain(|_, entry| entry.expiration > now);
    }

    fn prune_quota(
        &mut self,
        now: tokio::time::Instant,
        idle_ttl: Duration,
        protected: &Contributions,
    ) -> Result<(), RateLimitError> {
        let mut removable = Vec::new();
        for (key, entry) in &mut self.quota {
            if protected.0.contains_key(key) {
                continue;
            }
            entry.refill(now, key)?;
            if entry.balance == key.full_balance()?
                && now.saturating_duration_since(entry.last_used) >= idle_ttl
            {
                removable.push(key.clone());
            }
        }
        for key in removable {
            self.quota.remove(&key);
        }
        Ok(())
    }

    fn evaluate(
        &mut self,
        now: tokio::time::Instant,
        host: Option<&str>,
        rule_keys: &RuleKeys,
        contributions: &Contributions,
        quota_capacity: usize,
    ) -> Result<Evaluation, RateLimitError> {
        let cooldown_wait = self.cooldown_wait(now, host, rule_keys);
        if !cooldown_wait.is_zero() {
            return Ok(Evaluation::Wait(cooldown_wait));
        }

        self.insert_missing_quota(now, contributions, quota_capacity)?;

        let mut quota_wait = Duration::ZERO;
        for (key, cost) in &contributions.0 {
            let entry = self.quota.get_mut(key).ok_or_else(internal_error)?;
            entry.refill(now, key)?;
            entry.last_used = now;
            let required = key.required_balance(*cost)?;
            if entry.balance < required {
                let deficit = required - entry.balance;
                let wait_nanos = ceil_div(deficit, u128::from(key.capacity));
                quota_wait = quota_wait.max(duration_from_nanos(wait_nanos)?);
            }
        }
        if !quota_wait.is_zero() {
            return Ok(Evaluation::Wait(quota_wait));
        }

        let mut balances = Vec::with_capacity(contributions.0.len());
        for (key, cost) in &contributions.0 {
            let required = key.required_balance(*cost)?;
            let balance = self
                .quota
                .get(key)
                .ok_or_else(internal_error)?
                .balance
                .checked_sub(required)
                .ok_or_else(internal_error)?;
            balances.push((key, balance));
        }
        for (key, balance) in balances {
            self.quota.get_mut(key).ok_or_else(internal_error)?.balance = balance;
        }
        Ok(Evaluation::Granted)
    }

    fn cooldown_wait(
        &self,
        now: tokio::time::Instant,
        host: Option<&str>,
        rule_keys: &RuleKeys,
    ) -> Duration {
        let mut wait = self.client_cooldown.map_or(Duration::ZERO, |expiration| {
            expiration.saturating_duration_since(now)
        });
        if let Some(host) = host {
            let key = CooldownKey::Host(host.to_owned());
            wait = wait.max(self.cooldown_duration(now, &key));
        }
        for rule_key in &rule_keys.0 {
            let key = CooldownKey::Bucket(rule_key.clone());
            wait = wait.max(self.cooldown_duration(now, &key));
        }
        wait
    }

    fn cooldown_duration(&self, now: tokio::time::Instant, key: &CooldownKey) -> Duration {
        self.cooldowns.get(key).map_or(Duration::ZERO, |entry| {
            entry.expiration.saturating_duration_since(now)
        })
    }

    fn insert_missing_quota(
        &mut self,
        now: tokio::time::Instant,
        contributions: &Contributions,
        capacity: usize,
    ) -> Result<(), RateLimitError> {
        let missing = contributions
            .0
            .keys()
            .filter(|key| !self.quota.contains_key(*key))
            .cloned()
            .collect::<Vec<_>>();
        if self
            .quota
            .len()
            .checked_add(missing.len())
            .ok_or_else(internal_error)?
            > capacity
        {
            return Err(state_capacity());
        }
        let pending = missing
            .into_iter()
            .map(|key| {
                let entry = QuotaEntry {
                    balance: key.full_balance()?,
                    last_refill: now,
                    last_used: now,
                };
                Ok((key, entry))
            })
            .collect::<Result<Vec<_>, RateLimitError>>()?;
        for (key, entry) in pending {
            self.quota.insert(key, entry);
        }
        Ok(())
    }

    fn extend_client_cooldown(&mut self, expiration: tokio::time::Instant) {
        self.client_cooldown = Some(
            self.client_cooldown
                .map_or(expiration, |current| current.max(expiration)),
        );
    }

    fn extend_cooldowns<I>(
        &mut self,
        keys: I,
        expiration: tokio::time::Instant,
        capacity: usize,
    ) -> Result<(), RateLimitError>
    where
        I: IntoIterator<Item = CooldownKey>,
    {
        let keys = keys.into_iter().collect::<BTreeSet<_>>();
        let missing = keys
            .iter()
            .filter(|key| !self.cooldowns.contains_key(*key))
            .count();
        if self
            .cooldowns
            .len()
            .checked_add(missing)
            .ok_or_else(internal_error)?
            > capacity
        {
            return Err(state_capacity());
        }
        for key in keys {
            self.cooldowns
                .entry(key)
                .and_modify(|entry| entry.expiration = entry.expiration.max(expiration))
                .or_insert(CooldownEntry { expiration });
        }
        Ok(())
    }
}

struct QuotaEntry {
    balance: u128,
    last_refill: tokio::time::Instant,
    last_used: tokio::time::Instant,
}

impl QuotaEntry {
    fn refill(&mut self, now: tokio::time::Instant, key: &BucketKey) -> Result<(), RateLimitError> {
        let elapsed = now.saturating_duration_since(self.last_refill);
        if elapsed.is_zero() {
            return Ok(());
        }
        let full = key.full_balance()?;
        self.balance = if elapsed >= key.duration {
            full
        } else {
            let refill = elapsed
                .as_nanos()
                .checked_mul(u128::from(key.capacity))
                .ok_or_else(internal_error)?;
            self.balance.saturating_add(refill).min(full)
        };
        self.last_refill = now;
        Ok(())
    }
}

struct CooldownEntry {
    expiration: tokio::time::Instant,
}

enum Evaluation {
    Granted,
    Wait(Duration),
}

fn ceil_div(dividend: u128, divisor: u128) -> u128 {
    let quotient = dividend / divisor;
    quotient + u128::from(dividend % divisor != 0)
}

fn duration_from_nanos(nanos: u128) -> Result<Duration, RateLimitError> {
    let seconds = nanos / NANOS_PER_SECOND;
    let subsecond = nanos % NANOS_PER_SECOND;
    let seconds = u64::try_from(seconds).map_err(|_| internal_error())?;
    let subsecond = u32::try_from(subsecond).map_err(|_| internal_error())?;
    Ok(Duration::new(seconds, subsecond))
}

fn invalid_configuration() -> RateLimitError {
    RateLimitError::new(RateLimitErrorKind::InvalidConfiguration)
}

fn state_capacity() -> RateLimitError {
    RateLimitError::new(RateLimitErrorKind::StateCapacity)
}

fn internal_error() -> RateLimitError {
    RateLimitError::new(RateLimitErrorKind::Internal)
}

#[cfg(test)]
mod tests;

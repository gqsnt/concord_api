use std::collections::{BTreeMap, BTreeSet};
#[cfg(test)]
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::error::RateLimitErrorKind;
use crate::hooks::BoxFuture;

use super::{
    Cooldown, CooldownScope, DefaultRateLimitObserver, RateLimitError, RateLimitObserver,
    RateLimitPermit, RateLimitPlan, RateLimitRequest, RateLimitResponse, RateLimitResponseHead,
    RateLimiter, limiter::DEFAULT_KEY_LIMIT, observation_cooldown_cap,
};

const DEFAULT_QUOTA_CAPACITY: usize = 4_096;
const DEFAULT_COOLDOWN_CAPACITY: usize = 4_096;
const DEFAULT_IDLE_TTL: Duration = Duration::from_secs(15 * 60);
const NANOS_PER_SECOND: u128 = 1_000_000_000;

/// A bounded best-effort process-local pacing limiter.
///
/// New governors hold at most 4,096 quota entries and 4,096 cooldown entries,
/// prune quota entries idle for 15 minutes, and accept resolved keys up to
/// 1,024 bytes. Builder methods can replace each bound.
///
/// State eviction may forget local history and permit a fresh burst. Use an
/// application-supplied custom limiter when strict or distributed enforcement
/// is required.
pub struct GovernorRateLimiter {
    config: GovernorConfig,
    observer: Arc<dyn RateLimitObserver>,
    state: Mutex<GovernorState>,
    internal_failed: AtomicBool,
    #[cfg(test)]
    acquisition_calls: AtomicUsize,
}

struct GovernorConfig {
    quota_capacity: usize,
    cooldown_capacity: usize,
    idle_ttl: Duration,
    key_limit: usize,
}

#[allow(
    clippy::new_without_default,
    reason = "the normative advanced API specifies an explicit constructor without a Default contract"
)]
impl GovernorRateLimiter {
    /// Creates an independent governor with the normative finite defaults.
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: GovernorConfig {
                quota_capacity: DEFAULT_QUOTA_CAPACITY,
                cooldown_capacity: DEFAULT_COOLDOWN_CAPACITY,
                idle_ttl: DEFAULT_IDLE_TTL,
                key_limit: DEFAULT_KEY_LIMIT,
            },
            observer: Arc::new(DefaultRateLimitObserver),
            state: Mutex::new(GovernorState::new()),
            internal_failed: AtomicBool::new(false),
            #[cfg(test)]
            acquisition_calls: AtomicUsize::new(0),
        }
    }

    /// Replaces the default response-head observer.
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
    /// Zero permits only a global key.
    pub fn key_limit(mut self, bytes: usize) -> Result<Self, RateLimitError> {
        self.config.key_limit = bytes;
        Ok(self)
    }

    pub(crate) const fn preparation_key_limit(&self) -> usize {
        self.config.key_limit
    }

    #[cfg(test)]
    pub(crate) fn test_acquisition_count(&self) -> usize {
        self.acquisition_calls.load(AtomicOrdering::SeqCst)
    }

    #[cfg(test)]
    pub(crate) fn test_quota_entry_count(&self) -> usize {
        self.state.lock().expect("governor state").quota.len()
    }

    fn acquire_loop<'a>(
        &'a self,
        request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        Box::pin(async move {
            if self.internal_failed.load(AtomicOrdering::Acquire) {
                return Err(internal_error());
            }
            let contributions = Contributions::from_plan(request.plan, self.config.key_limit)?;
            let rule_keys = RuleKeys::from_plan(request.plan);
            loop {
                let now = tokio::time::Instant::now();
                let evaluation = {
                    let mut state = self.state.lock().map_err(|_| internal_error())?;
                    state.prune(now, self.config.idle_ttl);
                    state.evaluate(
                        now,
                        request.host,
                        &rule_keys,
                        &contributions,
                        self.config.quota_capacity,
                    )?
                };
                match evaluation {
                    Evaluation::Granted => return Ok(RateLimitPermit::granted()),
                    Evaluation::Wait(duration) => {
                        let wake = now.checked_add(duration).ok_or_else(internal_error)?;
                        tokio::time::sleep_until(wake).await;
                    }
                }
                if self.internal_failed.load(AtomicOrdering::Acquire) {
                    return Err(internal_error());
                }
            }
        })
    }

    fn observe_response(&self, response: RateLimitResponse<'_>) {
        let RateLimitResponse {
            request,
            status,
            headers,
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
            return;
        };
        self.apply_cooldown(host, plan, cooldown, observation_cooldown_cap());
    }

    fn apply_cooldown(
        &self,
        host: Option<&str>,
        plan: &RateLimitPlan,
        cooldown: Cooldown,
        cap: Duration,
    ) {
        let duration = cooldown.duration.min(cap);
        if duration.is_zero() || self.internal_failed.load(AtomicOrdering::Acquire) {
            return;
        }
        let now = tokio::time::Instant::now();
        let Some(expiration) = now.checked_add(duration) else {
            self.internal_failed.store(true, AtomicOrdering::Release);
            return;
        };
        let Ok(mut state) = self.state.lock() else {
            self.internal_failed.store(true, AtomicOrdering::Release);
            return;
        };
        state.prune(now, self.config.idle_ttl);
        let result = match cooldown.scope {
            CooldownScope::Client => state.extend_client_cooldown(expiration),
            CooldownScope::Host => host.map_or(Ok(()), |host| {
                state.extend_cooldown(
                    CooldownKey::Host(host.to_owned()),
                    expiration,
                    self.config.cooldown_capacity,
                )
            }),
            CooldownScope::CurrentBuckets => {
                if plan.is_empty() {
                    host.map_or(Ok(()), |host| {
                        state.extend_cooldown(
                            CooldownKey::Host(host.to_owned()),
                            expiration,
                            self.config.cooldown_capacity,
                        )
                    })
                } else {
                    let rule_keys = RuleKeys::from_plan(plan);
                    rule_keys.0.into_iter().try_for_each(|key| {
                        state.extend_cooldown(
                            CooldownKey::Bucket(key),
                            expiration,
                            self.config.cooldown_capacity,
                        )
                    })
                }
            }
        };
        if result.is_err() {
            self.internal_failed.store(true, AtomicOrdering::Release);
        }
    }
}

impl RateLimiter for GovernorRateLimiter {
    fn acquire<'a>(
        &'a self,
        request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        #[cfg(test)]
        self.acquisition_calls.fetch_add(1, AtomicOrdering::SeqCst);
        self.acquire_loop(request)
    }

    fn observe<'a>(&'a self, response: RateLimitResponse<'a>) -> BoxFuture<'a, ()> {
        Box::pin(async move { self.observe_response(response) })
    }
}

struct Contributions(BTreeMap<BucketKey, u128>);

impl Contributions {
    fn from_plan(plan: &RateLimitPlan, key_limit: usize) -> Result<Self, RateLimitError> {
        plan.validate_key_limit(key_limit)?;
        let mut contributions = BTreeMap::<BucketKey, u128>::new();
        for rule in plan.rules() {
            for window in rule.windows() {
                let key = BucketKey::new(rule, window);
                let cost = contributions.entry(key).or_default();
                *cost = cost
                    .checked_add(u128::from(rule.cost().get()))
                    .ok_or_else(invalid_configuration)?;
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
    id: Box<str>,
    parts: Box<[(Box<str>, Box<str>)]>,
}

impl RuleKey {
    fn new(rule: &super::RateLimitRule) -> Self {
        Self {
            id: rule.id().as_str().into(),
            parts: rule
                .key()
                .parts()
                .iter()
                .map(|part| (part.name().into(), part.value().into()))
                .collect::<Vec<_>>()
                .into_boxed_slice(),
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
    fn new(rule: &super::RateLimitRule, window: &super::RateLimitWindow) -> Self {
        Self {
            rule: RuleKey::new(rule),
            capacity: window.capacity().get(),
            duration: window.duration(),
        }
    }

    fn full_balance(&self) -> Result<u128, RateLimitError> {
        self.duration
            .as_nanos()
            .checked_mul(u128::from(self.capacity))
            .ok_or_else(invalid_configuration)
    }

    fn required_balance(&self, cost: u128) -> Result<u128, RateLimitError> {
        self.duration
            .as_nanos()
            .checked_mul(cost)
            .ok_or_else(invalid_configuration)
    }
}

#[derive(Eq, Ord, PartialEq, PartialOrd)]
enum CooldownKey {
    Host(String),
    Bucket(RuleKey),
}

struct GovernorState {
    quota: BTreeMap<BucketKey, QuotaEntry>,
    cooldowns: BTreeMap<CooldownKey, CooldownEntry>,
    client_cooldown: Option<tokio::time::Instant>,
    next_touch: u128,
}

impl GovernorState {
    const fn new() -> Self {
        Self {
            quota: BTreeMap::new(),
            cooldowns: BTreeMap::new(),
            client_cooldown: None,
            next_touch: 0,
        }
    }

    fn next_touch(&mut self) -> Result<u128, RateLimitError> {
        self.next_touch = self.next_touch.checked_add(1).ok_or_else(internal_error)?;
        Ok(self.next_touch)
    }

    fn prune(&mut self, now: tokio::time::Instant, idle_ttl: Duration) {
        if self
            .client_cooldown
            .is_some_and(|expiration| expiration <= now)
        {
            self.client_cooldown = None;
        }
        self.cooldowns.retain(|_, entry| entry.expiration > now);
        self.quota
            .retain(|_, entry| now.saturating_duration_since(entry.last_used) < idle_ttl);
    }

    fn evaluate(
        &mut self,
        now: tokio::time::Instant,
        host: Option<&str>,
        rule_keys: &RuleKeys,
        contributions: &Contributions,
        quota_capacity: usize,
    ) -> Result<Evaluation, RateLimitError> {
        let cooldown_wait = self.cooldown_wait(now, host, rule_keys)?;
        if !cooldown_wait.is_zero() {
            return Ok(Evaluation::Wait(cooldown_wait));
        }

        self.make_quota_space(contributions, quota_capacity)?;
        for key in contributions.0.keys() {
            if !self.quota.contains_key(key) {
                let touch = self.next_touch()?;
                self.quota.insert(
                    key.clone(),
                    QuotaEntry {
                        balance: key.full_balance()?,
                        last_refill: now,
                        last_used: now,
                        touch,
                    },
                );
            }
        }

        let mut quota_wait = Duration::ZERO;
        for (key, cost) in &contributions.0 {
            let touch = self.next_touch()?;
            let entry = self.quota.get_mut(key).ok_or_else(internal_error)?;
            entry.refill(now, key)?;
            entry.last_used = now;
            entry.touch = touch;
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

        for (key, cost) in &contributions.0 {
            let required = key.required_balance(*cost)?;
            let entry = self.quota.get_mut(key).ok_or_else(internal_error)?;
            entry.balance = entry
                .balance
                .checked_sub(required)
                .ok_or_else(internal_error)?;
        }
        Ok(Evaluation::Granted)
    }

    fn cooldown_wait(
        &mut self,
        now: tokio::time::Instant,
        host: Option<&str>,
        rule_keys: &RuleKeys,
    ) -> Result<Duration, RateLimitError> {
        let mut wait = self.client_cooldown.map_or(Duration::ZERO, |expiration| {
            expiration.saturating_duration_since(now)
        });
        if let Some(host) = host {
            let key = CooldownKey::Host(host.to_owned());
            wait = wait.max(self.touch_cooldown(now, &key)?);
        }
        for rule_key in &rule_keys.0 {
            let key = CooldownKey::Bucket(rule_key.clone());
            wait = wait.max(self.touch_cooldown(now, &key)?);
        }
        Ok(wait)
    }

    fn touch_cooldown(
        &mut self,
        now: tokio::time::Instant,
        key: &CooldownKey,
    ) -> Result<Duration, RateLimitError> {
        if !self.cooldowns.contains_key(key) {
            return Ok(Duration::ZERO);
        }
        let touch = self.next_touch()?;
        let entry = self.cooldowns.get_mut(key).ok_or_else(internal_error)?;
        entry.touch = touch;
        Ok(entry.expiration.saturating_duration_since(now))
    }

    fn make_quota_space(
        &mut self,
        contributions: &Contributions,
        capacity: usize,
    ) -> Result<(), RateLimitError> {
        if contributions.0.len() > capacity {
            return Err(invalid_configuration());
        }
        let missing = contributions
            .0
            .keys()
            .filter(|key| !self.quota.contains_key(*key))
            .count();
        while self
            .quota
            .len()
            .checked_add(missing)
            .ok_or_else(internal_error)?
            > capacity
        {
            let candidate = self
                .quota
                .iter()
                .filter(|(key, _)| !contributions.0.contains_key(*key))
                .min_by(|(first_key, first), (second_key, second)| {
                    first
                        .touch
                        .cmp(&second.touch)
                        .then_with(|| first_key.cmp(second_key))
                })
                .map(|(key, _)| key.clone())
                .ok_or_else(internal_error)?;
            self.quota.remove(&candidate);
        }
        Ok(())
    }

    fn extend_client_cooldown(
        &mut self,
        expiration: tokio::time::Instant,
    ) -> Result<(), RateLimitError> {
        self.client_cooldown = Some(
            self.client_cooldown
                .map_or(expiration, |current| current.max(expiration)),
        );
        Ok(())
    }

    fn extend_cooldown(
        &mut self,
        key: CooldownKey,
        expiration: tokio::time::Instant,
        capacity: usize,
    ) -> Result<(), RateLimitError> {
        if self.cooldowns.contains_key(&key) {
            let touch = self.next_touch()?;
            let entry = self.cooldowns.get_mut(&key).ok_or_else(internal_error)?;
            entry.expiration = entry.expiration.max(expiration);
            entry.touch = touch;
            return Ok(());
        }
        if self.cooldowns.len() == capacity {
            let candidate = self
                .cooldowns
                .iter()
                .min_by(|(first_key, first), (second_key, second)| {
                    first
                        .touch
                        .cmp(&second.touch)
                        .then_with(|| first_key.cmp(second_key))
                })
                .map(|(key, _)| match key {
                    CooldownKey::Host(host) => CooldownKey::Host(host.clone()),
                    CooldownKey::Bucket(rule) => CooldownKey::Bucket(rule.clone()),
                })
                .ok_or_else(internal_error)?;
            self.cooldowns.remove(&candidate);
        }
        let touch = self.next_touch()?;
        self.cooldowns
            .insert(key, CooldownEntry { expiration, touch });
        Ok(())
    }
}

struct QuotaEntry {
    balance: u128,
    last_refill: tokio::time::Instant,
    last_used: tokio::time::Instant,
    touch: u128,
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
    touch: u128,
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

fn internal_error() -> RateLimitError {
    RateLimitError::new(RateLimitErrorKind::Internal)
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use http::{HeaderMap, Method, StatusCode};

    use super::*;
    use crate::rate_limit::{
        RateLimitId, RateLimitKey, RateLimitKeyPart, RateLimitRule, RateLimitWindow,
    };

    fn rule(id: &str, key: &str, cost: u32, windows: &[(u32, Duration)]) -> RateLimitRule {
        RateLimitRule::new(
            RateLimitId::new(id).expect("ID"),
            if key.is_empty() {
                RateLimitKey::global()
            } else {
                RateLimitKey::from_parts([RateLimitKeyPart::new("tenant", key)]).expect("key")
            },
            NonZeroU32::new(cost).expect("cost"),
            windows.iter().map(|(capacity, duration)| {
                RateLimitWindow::new(NonZeroU32::new(*capacity).expect("capacity"), *duration)
                    .expect("window")
            }),
        )
        .expect("rule")
    }

    fn plan(rules: Vec<RateLimitRule>) -> RateLimitPlan {
        RateLimitPlan::from_rules(rules)
    }

    fn rate_error<T>(result: Result<T, RateLimitError>, message: &str) -> RateLimitError {
        match result {
            Ok(_) => panic!("{message}"),
            Err(error) => error,
        }
    }

    async fn acquire(limiter: &GovernorRateLimiter, plan: &RateLimitPlan, host: Option<&str>) {
        limiter
            .acquire(RateLimitRequest {
                endpoint: "RateApi::get",
                method: &Method::GET,
                safe_url: "https://example.test/get",
                host,
                page_index: 0,
                plan,
            })
            .await
            .expect("acquire");
    }

    fn spawn_acquire(
        limiter: Arc<GovernorRateLimiter>,
        plan: Arc<RateLimitPlan>,
        host: Option<&'static str>,
    ) -> tokio::task::JoinHandle<Result<RateLimitPermit, RateLimitError>> {
        tokio::spawn(async move {
            limiter
                .acquire(RateLimitRequest {
                    endpoint: "RateApi::get",
                    method: &Method::GET,
                    safe_url: "https://example.test/get",
                    host,
                    page_index: 0,
                    plan: &plan,
                })
                .await
        })
    }

    #[derive(Clone, Copy)]
    enum FixedScope {
        Buckets,
        Host,
        Client,
    }

    struct FixedObserver {
        duration: Duration,
        scope: FixedScope,
    }

    struct NoCooldownObserver;

    impl RateLimitObserver for NoCooldownObserver {
        fn observe(&self, _response: &RateLimitResponseHead<'_>) -> Option<Cooldown> {
            None
        }
    }

    impl RateLimitObserver for FixedObserver {
        fn observe(&self, _response: &RateLimitResponseHead<'_>) -> Option<Cooldown> {
            let scope = match self.scope {
                FixedScope::Buckets => CooldownScope::CurrentBuckets,
                FixedScope::Host => CooldownScope::Host,
                FixedScope::Client => CooldownScope::Client,
            };
            Some(Cooldown {
                duration: self.duration,
                scope,
            })
        }
    }

    async fn observe(
        limiter: &Arc<GovernorRateLimiter>,
        plan: &RateLimitPlan,
        host: Option<&str>,
        status: StatusCode,
        cap: Duration,
    ) {
        let limiter_object: Arc<dyn RateLimiter> = limiter.clone();
        let headers = HeaderMap::new();
        super::super::observe_with_cooldown_cap(
            &limiter_object,
            RateLimitResponse {
                request: RateLimitRequest {
                    endpoint: "RateApi::get",
                    method: &Method::GET,
                    safe_url: "https://example.test/get",
                    host,
                    page_index: 0,
                    plan,
                },
                status,
                headers: crate::redaction::SanitizedHeaders::new(&headers),
            },
            cap,
        )
        .await;
    }

    async fn observe_headers(
        limiter: &Arc<GovernorRateLimiter>,
        plan: &RateLimitPlan,
        host: Option<&str>,
        status: StatusCode,
        headers: &HeaderMap,
        cap: Duration,
    ) {
        let limiter_object: Arc<dyn RateLimiter> = limiter.clone();
        super::super::observe_with_cooldown_cap(
            &limiter_object,
            RateLimitResponse {
                request: RateLimitRequest {
                    endpoint: "RateApi::get",
                    method: &Method::GET,
                    safe_url: "https://example.test/get",
                    host,
                    page_index: 0,
                    plan,
                },
                status,
                headers: crate::redaction::SanitizedHeaders::new(headers),
            },
            cap,
        )
        .await;
    }

    #[test]
    fn defaults_and_configuration_validation_are_finite() {
        let limiter = GovernorRateLimiter::new();
        assert_eq!(limiter.config.quota_capacity, 4_096);
        assert_eq!(limiter.config.cooldown_capacity, 4_096);
        assert_eq!(limiter.config.idle_ttl, Duration::from_secs(15 * 60));
        assert_eq!(limiter.config.key_limit, 1_024);
        assert_eq!(
            rate_error(
                GovernorRateLimiter::new().quota_capacity(0),
                "zero quota capacity unexpectedly succeeded",
            )
            .kind(),
            RateLimitErrorKind::InvalidConfiguration
        );
        assert_eq!(
            rate_error(
                GovernorRateLimiter::new().cooldown_capacity(0),
                "zero cooldown capacity unexpectedly succeeded",
            )
            .kind(),
            RateLimitErrorKind::InvalidConfiguration
        );
        assert_eq!(
            rate_error(
                GovernorRateLimiter::new().idle_ttl(Duration::ZERO),
                "zero idle TTL unexpectedly succeeded",
            )
            .kind(),
            RateLimitErrorKind::InvalidConfiguration
        );
        GovernorRateLimiter::new()
            .key_limit(0)
            .expect("zero key limit is valid");
    }

    #[tokio::test(start_paused = true)]
    async fn first_burst_is_immediate_and_refill_wait_is_controlled() {
        let limiter = Arc::new(GovernorRateLimiter::new());
        let plan = Arc::new(plan(vec![rule(
            "tenant",
            "north",
            1,
            &[(1, Duration::from_secs(10))],
        )]));
        acquire(&limiter, &plan, Some("example.test")).await;

        let waiting_limiter = Arc::clone(&limiter);
        let waiting_plan = Arc::clone(&plan);
        let task = tokio::spawn(async move {
            acquire(&waiting_limiter, &waiting_plan, Some("example.test")).await;
        });
        tokio::task::yield_now().await;
        assert!(!task.is_finished());
        tokio::time::advance(Duration::from_secs(9)).await;
        tokio::task::yield_now().await;
        assert!(!task.is_finished());
        tokio::time::advance(Duration::from_secs(1)).await;
        task.await.expect("waiting task");
    }

    #[tokio::test(start_paused = true)]
    async fn page_varying_actual_keys_are_distinct_and_equivalent_keys_aggregate() {
        let limiter = Arc::new(GovernorRateLimiter::new());
        let first = Arc::new(plan(vec![rule(
            "page",
            "offset-41082731",
            1,
            &[(1, Duration::from_secs(60))],
        )]));
        let second = Arc::new(plan(vec![rule(
            "page",
            "offset-41082733",
            1,
            &[(1, Duration::from_secs(60))],
        )]));

        acquire(&limiter, &first, Some("first.example.test")).await;
        acquire(&limiter, &second, Some("second.example.test")).await;
        assert_eq!(
            limiter.test_quota_entry_count(),
            2,
            "progressed page keys must occupy distinct governor buckets"
        );

        let repeated = spawn_acquire(
            Arc::clone(&limiter),
            Arc::clone(&first),
            Some("first.example.test"),
        );
        tokio::task::yield_now().await;
        assert!(
            !repeated.is_finished(),
            "an equivalent actual page key must aggregate with its existing bucket"
        );
        repeated.abort();
        let _ = repeated.await;
    }

    #[tokio::test(start_paused = true)]
    async fn multiple_rules_and_windows_wait_for_the_complete_maximum() {
        let limiter = Arc::new(GovernorRateLimiter::new());
        let plan = Arc::new(plan(vec![
            rule(
                "tenant",
                "north",
                1,
                &[(2, Duration::from_secs(10)), (2, Duration::from_secs(20))],
            ),
            rule("host", "example", 1, &[(2, Duration::from_secs(5))]),
        ]));
        acquire(&limiter, &plan, Some("example.test")).await;
        acquire(&limiter, &plan, Some("example.test")).await;

        let waiting = spawn_acquire(
            Arc::clone(&limiter),
            Arc::clone(&plan),
            Some("example.test"),
        );
        tokio::task::yield_now().await;
        assert!(!waiting.is_finished());
        tokio::time::advance(Duration::from_secs(9)).await;
        tokio::task::yield_now().await;
        assert!(!waiting.is_finished());
        tokio::time::advance(Duration::from_secs(1)).await;
        waiting
            .await
            .expect("waiting task")
            .expect("complete plan reservation");
    }

    #[tokio::test(start_paused = true)]
    async fn unavailable_rule_causes_no_partial_reservation_or_lock_across_wait() {
        let limiter = Arc::new(GovernorRateLimiter::new());
        let unavailable = Arc::new(plan(vec![rule(
            "slow",
            "north",
            1,
            &[(1, Duration::from_secs(10))],
        )]));
        acquire(&limiter, &unavailable, Some("example.test")).await;

        let combined = Arc::new(plan(vec![
            rule("slow", "north", 1, &[(1, Duration::from_secs(10))]),
            rule("available", "west", 1, &[(2, Duration::from_secs(100))]),
        ]));
        let waiting = spawn_acquire(
            Arc::clone(&limiter),
            Arc::clone(&combined),
            Some("example.test"),
        );
        tokio::task::yield_now().await;
        assert!(!waiting.is_finished());
        assert!(
            limiter.state.try_lock().is_ok(),
            "governor lock must be released before sleeping"
        );
        waiting.abort();
        let _ = waiting.await;

        let available = Arc::new(plan(vec![rule(
            "available",
            "west",
            1,
            &[(2, Duration::from_secs(100))],
        )]));
        acquire(&limiter, &available, Some("example.test")).await;
        let second = spawn_acquire(
            Arc::clone(&limiter),
            Arc::clone(&available),
            Some("example.test"),
        );
        tokio::task::yield_now().await;
        assert!(
            second.is_finished(),
            "canceled combined wait must leave both available tokens unconsumed"
        );
        second
            .await
            .expect("available task")
            .expect("second available reservation");
    }

    #[tokio::test(start_paused = true)]
    async fn cancellation_while_waiting_consumes_no_future_quota() {
        let limiter = Arc::new(GovernorRateLimiter::new());
        let plan = Arc::new(plan(vec![rule(
            "tenant",
            "north",
            1,
            &[(1, Duration::from_secs(10))],
        )]));
        acquire(&limiter, &plan, Some("example.test")).await;

        let waiting_limiter = Arc::clone(&limiter);
        let waiting_plan = Arc::clone(&plan);
        let waiting = tokio::spawn(async move {
            acquire(&waiting_limiter, &waiting_plan, Some("example.test")).await;
        });
        tokio::task::yield_now().await;
        assert!(!waiting.is_finished());
        waiting.abort();
        let _ = waiting.await;

        tokio::time::advance(Duration::from_secs(10)).await;
        acquire(&limiter, &plan, Some("example.test")).await;
    }

    #[tokio::test(start_paused = true)]
    async fn successful_reservation_is_not_refunded_after_later_cancellation() {
        let limiter = Arc::new(GovernorRateLimiter::new());
        let plan = Arc::new(plan(vec![rule(
            "tenant",
            "north",
            1,
            &[(1, Duration::from_secs(10))],
        )]));
        acquire(&limiter, &plan, Some("example.test")).await;

        let later = spawn_acquire(
            Arc::clone(&limiter),
            Arc::clone(&plan),
            Some("example.test"),
        );
        tokio::task::yield_now().await;
        assert!(
            !later.is_finished(),
            "a completed reservation remains consumed even if later request work is absent"
        );
        later.abort();
        let _ = later.await;
    }

    #[tokio::test(start_paused = true)]
    async fn duplicate_equivalent_contributions_cannot_undercount() {
        let limiter = GovernorRateLimiter::new();
        let plan = plan(vec![
            rule("same", "north", 1, &[(2, Duration::from_secs(10))]),
            rule("same", "north", 1, &[(2, Duration::from_secs(10))]),
        ]);
        acquire(&limiter, &plan, Some("example.test")).await;
        let request = limiter.acquire(RateLimitRequest {
            endpoint: "RateApi::get",
            method: &Method::GET,
            safe_url: "https://example.test/get",
            host: Some("example.test"),
            page_index: 0,
            plan: &plan,
        });
        tokio::pin!(request);
        assert!(
            std::future::poll_fn(|context| std::task::Poll::Ready(
                request.as_mut().poll(context).is_pending()
            ))
            .await
        );
    }

    #[tokio::test]
    async fn equivalent_contributions_above_capacity_fail_before_state_insertion() {
        let limiter = GovernorRateLimiter::new();
        let plan = plan(vec![
            rule("same", "north", 1, &[(2, Duration::from_secs(10))]),
            rule("same", "north", 1, &[(2, Duration::from_secs(10))]),
            rule("same", "north", 1, &[(2, Duration::from_secs(10))]),
        ]);
        let error = rate_error(
            limiter
                .acquire(RateLimitRequest {
                    endpoint: "RateApi::get",
                    method: &Method::GET,
                    safe_url: "https://example.test/get",
                    host: Some("example.test"),
                    page_index: 0,
                    plan: &plan,
                })
                .await,
            "over-capacity aggregate unexpectedly granted",
        );
        assert_eq!(error.kind(), RateLimitErrorKind::InvalidConfiguration);
        assert!(limiter.state.lock().expect("state").quota.is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn quota_state_is_bounded_idle_pruned_and_eviction_allows_fresh_burst() {
        let limiter = GovernorRateLimiter::new()
            .quota_capacity(2)
            .expect("quota capacity")
            .idle_ttl(Duration::from_secs(5))
            .expect("idle TTL");
        let first = plan(vec![rule(
            "first",
            "one",
            1,
            &[(1, Duration::from_secs(100))],
        )]);
        let second = plan(vec![rule(
            "second",
            "two",
            1,
            &[(1, Duration::from_secs(100))],
        )]);
        let third = plan(vec![rule(
            "third",
            "three",
            1,
            &[(1, Duration::from_secs(100))],
        )]);
        acquire(&limiter, &first, Some("example.test")).await;
        acquire(&limiter, &second, Some("example.test")).await;
        acquire(&limiter, &third, Some("example.test")).await;
        assert_eq!(limiter.state.lock().expect("state").quota.len(), 2);

        acquire(&limiter, &first, Some("example.test")).await;
        assert_eq!(
            limiter.state.lock().expect("state").quota.len(),
            2,
            "re-encountered evicted key receives a fresh local burst within the bound"
        );

        tokio::time::advance(Duration::from_secs(5)).await;
        acquire(&limiter, &third, Some("example.test")).await;
        let state = limiter.state.lock().expect("state");
        assert_eq!(state.quota.len(), 1, "idle entries are pruned at the TTL");
        assert_eq!(
            state
                .quota
                .keys()
                .next()
                .expect("one entry")
                .rule
                .id
                .as_ref(),
            "third"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn current_bucket_cooldown_is_exact_and_empty_plan_falls_back_to_host() {
        let limiter = Arc::new(GovernorRateLimiter::new().observer(Arc::new(FixedObserver {
            duration: Duration::from_secs(10),
            scope: FixedScope::Buckets,
        })));
        let first = Arc::new(plan(vec![rule(
            "first",
            "one",
            1,
            &[(10, Duration::from_secs(1))],
        )]));
        let second = Arc::new(plan(vec![rule(
            "second",
            "two",
            1,
            &[(10, Duration::from_secs(1))],
        )]));
        observe(
            &limiter,
            &first,
            Some("origin.test"),
            StatusCode::TOO_MANY_REQUESTS,
            Duration::from_secs(60),
        )
        .await;

        let blocked = spawn_acquire(Arc::clone(&limiter), Arc::clone(&first), Some("other.test"));
        let unrelated = spawn_acquire(
            Arc::clone(&limiter),
            Arc::clone(&second),
            Some("origin.test"),
        );
        tokio::task::yield_now().await;
        assert!(!blocked.is_finished());
        assert!(unrelated.is_finished());
        unrelated
            .await
            .expect("unrelated task")
            .expect("unrelated bucket");
        blocked.abort();
        let _ = blocked.await;

        let empty = Arc::new(RateLimitPlan::empty());
        observe(
            &limiter,
            &empty,
            Some("empty.test"),
            StatusCode::TOO_MANY_REQUESTS,
            Duration::from_secs(60),
        )
        .await;
        let host_blocked = spawn_acquire(
            Arc::clone(&limiter),
            Arc::clone(&second),
            Some("empty.test"),
        );
        let host_free = spawn_acquire(Arc::clone(&limiter), Arc::clone(&second), Some("free.test"));
        tokio::task::yield_now().await;
        assert!(!host_blocked.is_finished());
        assert!(host_free.is_finished());
        host_free.await.expect("free host task").expect("free host");
        host_blocked.abort();
        let _ = host_blocked.await;
    }

    #[tokio::test(start_paused = true)]
    async fn host_and_client_cooldowns_affect_only_their_normative_scopes() {
        let plan = Arc::new(plan(vec![rule(
            "quota",
            "key",
            1,
            &[(100, Duration::from_secs(1))],
        )]));
        let host_limiter = Arc::new(GovernorRateLimiter::new().observer(Arc::new(FixedObserver {
            duration: Duration::from_secs(10),
            scope: FixedScope::Host,
        })));
        observe(
            &host_limiter,
            &plan,
            Some("blocked.test"),
            StatusCode::OK,
            Duration::from_secs(60),
        )
        .await;
        let blocked = spawn_acquire(
            Arc::clone(&host_limiter),
            Arc::clone(&plan),
            Some("blocked.test"),
        );
        let free = spawn_acquire(
            Arc::clone(&host_limiter),
            Arc::clone(&plan),
            Some("free.test"),
        );
        tokio::task::yield_now().await;
        assert!(!blocked.is_finished());
        assert!(free.is_finished());
        free.await.expect("free task").expect("free host");
        blocked.abort();
        let _ = blocked.await;

        let client_limiter =
            Arc::new(GovernorRateLimiter::new().observer(Arc::new(FixedObserver {
                duration: Duration::from_secs(10),
                scope: FixedScope::Client,
            })));
        observe(
            &client_limiter,
            &plan,
            Some("first.test"),
            StatusCode::OK,
            Duration::from_secs(60),
        )
        .await;
        let client_blocked = spawn_acquire(
            Arc::clone(&client_limiter),
            Arc::clone(&plan),
            Some("different.test"),
        );
        tokio::task::yield_now().await;
        assert!(!client_blocked.is_finished());
        client_blocked.abort();
        let _ = client_blocked.await;
    }

    #[tokio::test(start_paused = true)]
    async fn cooldown_is_clamped_extended_never_shortened_and_zero_is_noop() {
        let plan = Arc::new(RateLimitPlan::empty());
        let long = Arc::new(GovernorRateLimiter::new().observer(Arc::new(FixedObserver {
            duration: Duration::from_secs(20),
            scope: FixedScope::Client,
        })));
        observe(
            &long,
            &plan,
            Some("example.test"),
            StatusCode::OK,
            Duration::from_secs(10),
        )
        .await;
        tokio::time::advance(Duration::from_secs(2)).await;
        long.apply_cooldown(
            Some("example.test"),
            &plan,
            Cooldown {
                duration: Duration::from_secs(3),
                scope: CooldownScope::Client,
            },
            Duration::from_secs(10),
        );
        let waiting = spawn_acquire(Arc::clone(&long), Arc::clone(&plan), Some("example.test"));
        tokio::time::advance(Duration::from_secs(7)).await;
        tokio::task::yield_now().await;
        assert!(
            !waiting.is_finished(),
            "shorter update must not shorten the cap"
        );
        tokio::time::advance(Duration::from_secs(1)).await;
        waiting
            .await
            .expect("clamped task")
            .expect("clamped cooldown expired");

        long.apply_cooldown(
            Some("example.test"),
            &plan,
            Cooldown {
                duration: Duration::ZERO,
                scope: CooldownScope::Client,
            },
            Duration::from_secs(60),
        );
        acquire(&long, &plan, Some("example.test")).await;

        long.apply_cooldown(
            Some("example.test"),
            &plan,
            Cooldown {
                duration: Duration::from_secs(4),
                scope: CooldownScope::Client,
            },
            Duration::from_secs(60),
        );
        tokio::time::advance(Duration::from_secs(1)).await;
        long.apply_cooldown(
            Some("example.test"),
            &plan,
            Cooldown {
                duration: Duration::from_secs(6),
                scope: CooldownScope::Client,
            },
            Duration::from_secs(60),
        );
        let extended = spawn_acquire(Arc::clone(&long), Arc::clone(&plan), Some("example.test"));
        tokio::time::advance(Duration::from_secs(5)).await;
        tokio::task::yield_now().await;
        assert!(
            !extended.is_finished(),
            "later expiration extends existing state"
        );
        tokio::time::advance(Duration::from_secs(1)).await;
        extended
            .await
            .expect("extended task")
            .expect("extended cooldown expired");
    }

    #[tokio::test(start_paused = true)]
    async fn cooldown_state_capacity_uses_deterministic_eviction() {
        let limiter = Arc::new(
            GovernorRateLimiter::new()
                .cooldown_capacity(1)
                .expect("cooldown capacity")
                .observer(Arc::new(FixedObserver {
                    duration: Duration::from_secs(10),
                    scope: FixedScope::Host,
                })),
        );
        let plan = Arc::new(RateLimitPlan::empty());
        observe(
            &limiter,
            &plan,
            Some("first.test"),
            StatusCode::OK,
            Duration::from_secs(60),
        )
        .await;
        observe(
            &limiter,
            &plan,
            Some("second.test"),
            StatusCode::OK,
            Duration::from_secs(60),
        )
        .await;
        assert_eq!(limiter.state.lock().expect("state").cooldowns.len(), 1);

        let evicted = spawn_acquire(Arc::clone(&limiter), Arc::clone(&plan), Some("first.test"));
        let retained = spawn_acquire(Arc::clone(&limiter), Arc::clone(&plan), Some("second.test"));
        tokio::task::yield_now().await;
        assert!(evicted.is_finished());
        assert!(!retained.is_finished());
        evicted
            .await
            .expect("evicted task")
            .expect("evicted cooldown is forgotten");
        retained.abort();
        let _ = retained.await;
    }

    #[tokio::test(start_paused = true)]
    async fn default_retry_after_cooldown_is_future_only_host_fallback_and_runtime_capped() {
        let limiter = Arc::new(GovernorRateLimiter::new());
        let plan = Arc::new(RateLimitPlan::empty());
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::RETRY_AFTER,
            http::HeaderValue::from_static(" 20 "),
        );
        observe_headers(
            &limiter,
            &plan,
            Some("limited.test"),
            StatusCode::TOO_MANY_REQUESTS,
            &headers,
            Duration::from_secs(5),
        )
        .await;

        let blocked = spawn_acquire(
            Arc::clone(&limiter),
            Arc::clone(&plan),
            Some("limited.test"),
        );
        let free = spawn_acquire(Arc::clone(&limiter), Arc::clone(&plan), Some("free.test"));
        tokio::task::yield_now().await;
        assert!(!blocked.is_finished());
        assert!(free.is_finished());
        free.await.expect("free task").expect("other host");
        tokio::time::advance(Duration::from_secs(4)).await;
        tokio::task::yield_now().await;
        assert!(!blocked.is_finished());
        tokio::time::advance(Duration::from_secs(1)).await;
        blocked
            .await
            .expect("blocked task")
            .expect("runtime-capped cooldown");

        headers.insert(
            http::header::RETRY_AFTER,
            http::HeaderValue::from_static("malformed"),
        );
        observe_headers(
            &limiter,
            &plan,
            Some("malformed.test"),
            StatusCode::TOO_MANY_REQUESTS,
            &headers,
            Duration::from_secs(5),
        )
        .await;
        acquire(&limiter, &plan, Some("malformed.test")).await;

        headers.insert(
            http::header::RETRY_AFTER,
            http::HeaderValue::from_static("18446744073709551615"),
        );
        observe_headers(
            &limiter,
            &plan,
            Some("excessive.test"),
            StatusCode::TOO_MANY_REQUESTS,
            &headers,
            Duration::from_secs(5),
        )
        .await;
        let excessive = spawn_acquire(
            Arc::clone(&limiter),
            Arc::clone(&plan),
            Some("excessive.test"),
        );
        tokio::task::yield_now().await;
        assert!(!excessive.is_finished());
        tokio::time::advance(Duration::from_secs(5)).await;
        excessive
            .await
            .expect("excessive task")
            .expect("excessive delta is safely runtime-capped");
    }

    #[tokio::test(start_paused = true)]
    async fn configured_observer_replaces_rather_than_composes_with_default() {
        let limiter = Arc::new(GovernorRateLimiter::new().observer(Arc::new(NoCooldownObserver)));
        let plan = RateLimitPlan::empty();
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::RETRY_AFTER,
            http::HeaderValue::from_static("60"),
        );
        observe_headers(
            &limiter,
            &plan,
            Some("example.test"),
            StatusCode::TOO_MANY_REQUESTS,
            &headers,
            Duration::from_secs(60),
        )
        .await;
        acquire(&limiter, &plan, Some("example.test")).await;
        assert!(limiter.state.lock().expect("state").cooldowns.is_empty());
    }

    #[tokio::test]
    async fn zero_key_limit_accepts_global_and_rejects_nonempty_without_text() {
        const KEY_SENTINEL: &str = "governor-key-private-ae7e93";
        let limiter = GovernorRateLimiter::new().key_limit(0).expect("limit");
        let global = plan(vec![rule("global", "", 1, &[(1, Duration::from_secs(1))])]);
        acquire(&limiter, &global, None).await;

        let keyed = plan(vec![rule(
            "keyed",
            KEY_SENTINEL,
            1,
            &[(1, Duration::from_secs(1))],
        )]);
        let error = rate_error(
            limiter
                .acquire(RateLimitRequest {
                    endpoint: "RateApi::get",
                    method: &Method::GET,
                    safe_url: "https://example.test/get",
                    host: Some("example.test"),
                    page_index: 0,
                    plan: &keyed,
                })
                .await,
            "nonempty key unexpectedly fit zero limit",
        );
        assert_eq!(error.kind(), RateLimitErrorKind::InvalidKey);
        assert!(!error.to_string().contains(KEY_SENTINEL));
        assert!(!format!("{error:?}").contains(KEY_SENTINEL));
    }

    #[tokio::test]
    async fn configured_key_limit_revalidates_before_governor_state_insertion() {
        const KEY_SENTINEL: &str = "configured-governor-key-private-781d32";
        let limit = "tenant".len() + KEY_SENTINEL.len() - 1;
        let limiter = GovernorRateLimiter::new()
            .key_limit(limit)
            .expect("configured key limit");
        let keyed = plan(vec![rule(
            "keyed",
            KEY_SENTINEL,
            1,
            &[(1, Duration::from_secs(1))],
        )]);
        let error = rate_error(
            limiter
                .acquire(RateLimitRequest {
                    endpoint: "RateApi::get",
                    method: &Method::GET,
                    safe_url: "https://example.test/get",
                    host: Some("example.test"),
                    page_index: 0,
                    plan: &keyed,
                })
                .await,
            "oversized configured key unexpectedly granted",
        );
        assert_eq!(error.kind(), RateLimitErrorKind::InvalidKey);
        assert_eq!(limiter.test_acquisition_count(), 1);
        assert_eq!(limiter.test_quota_entry_count(), 0);
        let mut source: Option<&(dyn std::error::Error + 'static)> = Some(&error);
        while let Some(current) = source {
            assert!(!current.to_string().contains(KEY_SENTINEL));
            assert!(!format!("{current:?}").contains(KEY_SENTINEL));
            source = current.source();
        }
    }

    #[test]
    fn poisoned_state_fails_future_acquisition_closed() {
        let limiter = Arc::new(GovernorRateLimiter::new().observer(Arc::new(FixedObserver {
            duration: Duration::from_secs(1),
            scope: FixedScope::Client,
        })));
        let poison = Arc::clone(&limiter);
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let _guard = poison.state.lock().expect("initial lock");
            panic!("intentional test poison");
        }));
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        let empty = RateLimitPlan::empty();
        runtime.block_on(observe(
            &limiter,
            &empty,
            Some("example.test"),
            StatusCode::OK,
            Duration::from_secs(60),
        ));
        assert!(limiter.internal_failed.load(AtomicOrdering::Acquire));
        let error = rate_error(
            runtime.block_on(limiter.acquire(RateLimitRequest {
                endpoint: "RateApi::get",
                method: &Method::GET,
                safe_url: "https://example.test/get",
                host: Some("example.test"),
                page_index: 0,
                plan: &empty,
            })),
            "poisoned state unexpectedly granted",
        );
        assert_eq!(error.kind(), RateLimitErrorKind::Internal);
    }

    #[test]
    fn boundary_helpers_are_checked() {
        assert_eq!(ceil_div(0, 3), 0);
        assert_eq!(ceil_div(1, 3), 1);
        assert_eq!(ceil_div(4, 3), 2);
        assert_eq!(
            duration_from_nanos(NANOS_PER_SECOND + 7).expect("duration"),
            Duration::new(1, 7)
        );
        assert_eq!(
            duration_from_nanos(u128::MAX)
                .expect_err("unrepresentable duration")
                .kind(),
            RateLimitErrorKind::Internal
        );
        let now = tokio::time::Instant::now();
        if let Some(wake) = now.checked_add(Duration::MAX) {
            assert!(wake > now);
        }
        let maximum = plan(vec![rule(
            "maximum",
            "",
            u32::MAX,
            &[(u32::MAX, Duration::MAX)],
        )]);
        let contributions = Contributions::from_plan(&maximum, 0).expect("maximum contribution");
        let (key, cost) = contributions.0.first_key_value().expect("one bucket");
        assert_eq!(
            key.full_balance().expect("maximum full balance"),
            key.required_balance(*cost)
                .expect("maximum required balance")
        );
        let mut state = GovernorState::new();
        state.next_touch = u128::MAX;
        assert_eq!(
            state.next_touch().expect_err("touch overflow").kind(),
            RateLimitErrorKind::Internal
        );
    }
}

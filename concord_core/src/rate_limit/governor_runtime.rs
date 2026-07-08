use super::limiter::RateLimitFuture;
use super::{
    DefaultRateLimitResponsePolicy, RateLimitBucketId, RateLimitBucketUse, RateLimitContext,
    RateLimitKey, RateLimitKeyPart, RateLimitKeyValue, RateLimitObservation, RateLimitPermit,
    RateLimitResponseAction, RateLimitResponseContext, RateLimitResponsePolicy, RateLimitTarget,
    RateLimitWindow, RateLimiter,
};
use crate::error::{ApiClientError, ErrorContext};
use governor::{DefaultDirectRateLimiter, Quota, RateLimiter as Governor};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::time::Instant;

pub type DefaultRateLimiter = GovernorRateLimiter;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
struct GovernorWindowSpec {
    id: RateLimitBucketId,
    key: ResolvedRateLimitKey,
    window: RateLimitWindow,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
struct ResolvedRateLimitKey(Vec<(Cow<'static, str>, Cow<'static, str>)>);

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
enum RateLimitCooldownKey {
    Client,
    Request {
        method: http::Method,
        endpoint: &'static str,
        url: String,
    },
    Endpoint {
        method: http::Method,
        endpoint: &'static str,
    },
    Host {
        host: String,
    },
    BucketKind {
        kind: Cow<'static, str>,
        key: ResolvedRateLimitKey,
    },
    Bucket {
        id: RateLimitBucketId,
        key: ResolvedRateLimitKey,
    },
}

pub struct GovernorRateLimiter {
    windows: Mutex<GovernorWindowState>,
    cooldowns: Mutex<HashMap<RateLimitCooldownKey, Instant>>,
    response_policy: Arc<dyn RateLimitResponsePolicy>,
    max_window_entries: usize,
    max_cooldown_entries: usize,
    window_idle_ttl: Duration,
}

#[derive(Clone)]
struct GovernorWindowEntry {
    limiter: Arc<DefaultDirectRateLimiter>,
    last_used: Instant,
}

#[derive(Default)]
struct GovernorWindowState {
    windows: HashMap<GovernorWindowSpec, GovernorWindowEntry>,
    next_prune_at: Option<Instant>,
}

impl Default for GovernorRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl GovernorRateLimiter {
    const DEFAULT_MAX_WINDOW_ENTRIES: usize = 4096;
    pub const DEFAULT_MAX_COOLDOWN_ENTRIES: usize = 4096;
    const DEFAULT_WINDOW_IDLE_TTL: Duration = Duration::from_secs(15 * 60);

    pub fn new() -> Self {
        Self {
            windows: Mutex::new(GovernorWindowState::default()),
            cooldowns: Mutex::new(HashMap::new()),
            response_policy: Arc::new(DefaultRateLimitResponsePolicy),
            max_window_entries: Self::DEFAULT_MAX_WINDOW_ENTRIES,
            max_cooldown_entries: Self::DEFAULT_MAX_COOLDOWN_ENTRIES,
            window_idle_ttl: Self::DEFAULT_WINDOW_IDLE_TTL,
        }
    }

    pub fn with_response_policy(mut self, policy: Arc<dyn RateLimitResponsePolicy>) -> Self {
        self.response_policy = policy;
        self
    }

    pub fn with_max_window_entries(mut self, max_window_entries: usize) -> Self {
        self.max_window_entries = max_window_entries.max(1);
        self
    }

    pub fn with_max_cooldown_entries(mut self, max_cooldown_entries: usize) -> Self {
        self.max_cooldown_entries = max_cooldown_entries.max(1);
        self
    }

    pub fn with_window_idle_ttl(mut self, window_idle_ttl: Duration) -> Self {
        self.window_idle_ttl = window_idle_ttl;
        self
    }

    fn limiter_for(
        &self,
        ctx: &RateLimitContext<'_>,
        spec: GovernorWindowSpec,
    ) -> Result<Arc<DefaultDirectRateLimiter>, ApiClientError> {
        let mut guard = self
            .windows
            .lock()
            .map_err(|_| rate_limit_internal_error(ctx, "rate limit window lock poisoned"))?;
        let now = Instant::now();
        self.prune_windows_if_needed(&mut guard, now);
        if let Some(limiter) = {
            guard.windows.get_mut(&spec).map(|existing| {
                existing.last_used = now;
                existing.limiter.clone()
            })
        } {
            self.note_window_prune_at(&mut guard, now);
            return Ok(limiter);
        }

        let current_len = guard.windows.len();
        if current_len >= self.max_window_entries {
            let excess = current_len + 1 - self.max_window_entries;
            self.evict_oldest_windows(&mut guard.windows, excess);
        }

        let quota = quota_for_window(ctx, &spec.window)?;
        let limiter = Arc::new(Governor::direct(quota));
        guard.windows.insert(
            spec,
            GovernorWindowEntry {
                limiter: limiter.clone(),
                last_used: now,
            },
        );
        self.note_window_prune_at(&mut guard, now);
        Ok(limiter)
    }

    fn prune_windows_if_needed(&self, state: &mut GovernorWindowState, now: Instant) {
        if state.next_prune_at.is_none_or(|deadline| now < deadline) {
            return;
        }

        self.prune_windows(state, now);
    }

    fn prune_windows(&self, state: &mut GovernorWindowState, now: Instant) {
        if !self.window_idle_ttl.is_zero() {
            state.windows.retain(|_, entry| {
                now.saturating_duration_since(entry.last_used) <= self.window_idle_ttl
            });
        }

        let current_len = state.windows.len();
        if current_len > self.max_window_entries {
            self.evict_oldest_windows(&mut state.windows, current_len - self.max_window_entries);
        }

        state.next_prune_at = self.next_window_prune_at(&state.windows);
    }

    fn evict_oldest_windows(
        &self,
        windows: &mut HashMap<GovernorWindowSpec, GovernorWindowEntry>,
        count: usize,
    ) {
        if count == 0 || windows.is_empty() {
            return;
        }

        let mut oldest = windows
            .iter()
            .map(|(spec, entry)| (spec.clone(), entry.last_used))
            .collect::<Vec<_>>();
        oldest.sort_by_key(|(_, last_used)| *last_used);
        for (spec, _) in oldest.into_iter().take(count) {
            windows.remove(&spec);
        }
    }

    fn note_window_prune_at(&self, state: &mut GovernorWindowState, now: Instant) {
        if self.window_idle_ttl.is_zero() {
            state.next_prune_at = Some(now);
            return;
        }

        let Some(deadline) = now.checked_add(self.window_idle_ttl) else {
            return;
        };
        state.next_prune_at = Some(
            state
                .next_prune_at
                .map_or(deadline, |current| current.min(deadline)),
        );
    }

    fn next_window_prune_at(
        &self,
        windows: &HashMap<GovernorWindowSpec, GovernorWindowEntry>,
    ) -> Option<Instant> {
        if self.window_idle_ttl.is_zero() {
            return windows.values().map(|entry| entry.last_used).min();
        }

        windows
            .values()
            .filter_map(|entry| entry.last_used.checked_add(self.window_idle_ttl))
            .min()
    }

    async fn wait_cooldown(&self, ctx: &RateLimitContext<'_>) -> Result<(), ApiClientError> {
        loop {
            let now = Instant::now();
            let delay = {
                let keys = cooldown_keys_for_acquire(ctx)?;
                let mut guard = self.cooldowns.lock().map_err(|_| {
                    rate_limit_internal_error(ctx, "rate limit cooldown lock poisoned")
                })?;
                prune_cooldowns(&mut guard, now);
                keys.into_iter()
                    .filter_map(|key| guard.get(&key).copied())
                    .filter_map(|until| until.checked_duration_since(now))
                    .max()
            };

            let Some(delay) = delay else {
                return Ok(());
            };
            if delay.is_zero() {
                return Ok(());
            }
            if delay > ctx.max_cooldown {
                return Err(rate_limit_configuration_error(
                    ctx,
                    "rate-limit cooldown exceeds configured maximum",
                ));
            }
            tokio::time::sleep(delay).await;
        }
    }

    fn store_observation(
        &self,
        ctx: &RateLimitResponseContext<'_>,
        observation: RateLimitObservation,
    ) -> Result<RateLimitResponseAction, ApiClientError> {
        if !observation.limited {
            return Ok(RateLimitResponseAction::Continue);
        }

        if observation.delay.is_none_or(|delay| delay.is_zero())
            || matches!(observation.target, RateLimitTarget::None)
        {
            return Ok(RateLimitResponseAction::Limited {
                retry_after: observation.delay,
                target: observation.target,
                cooldown_stored: false,
            });
        }

        let mut cooldown_stored = false;
        if let Some(delay) = observation.delay {
            cooldown_stored =
                self.store_cooldown(&ctx.meta, &observation.target, delay, ctx.max_cooldown)?;
        }

        Ok(RateLimitResponseAction::Limited {
            retry_after: observation.delay,
            target: observation.target,
            cooldown_stored,
        })
    }

    fn store_cooldown(
        &self,
        ctx: &RateLimitContext<'_>,
        target: &RateLimitTarget,
        delay: std::time::Duration,
        max_cooldown: Duration,
    ) -> Result<bool, ApiClientError> {
        let keys = cooldown_keys_for_target(ctx, target)?;
        if keys.is_empty() {
            return Ok(false);
        }

        if delay > max_cooldown {
            return Err(rate_limit_configuration_error(
                ctx,
                "rate-limit cooldown exceeds configured maximum",
            ));
        }
        let now = Instant::now();
        let until = now.checked_add(delay).ok_or_else(|| {
            rate_limit_configuration_error(ctx, "rate-limit cooldown duration overflowed")
        })?;
        let mut guard = self
            .cooldowns
            .lock()
            .map_err(|_| rate_limit_internal_error(ctx, "rate limit cooldown lock poisoned"))?;
        prune_cooldowns(&mut guard, now);
        let new_entries = keys
            .iter()
            .filter(|key| !guard.contains_key(*key))
            .collect::<HashSet<_>>()
            .len();
        if guard.len().saturating_add(new_entries) > self.max_cooldown_entries {
            return Err(rate_limit_configuration_error(
                ctx,
                "rate-limit cooldown entry cap exceeded",
            ));
        }
        for key in keys {
            let entry = guard.entry(key).or_insert(until);
            if *entry < until {
                *entry = until;
            }
        }
        Ok(true)
    }
}

fn prune_cooldowns(cooldowns: &mut HashMap<RateLimitCooldownKey, Instant>, now: Instant) {
    cooldowns.retain(|_, until| *until > now);
}

impl RateLimiter for GovernorRateLimiter {
    fn acquire<'a>(
        &'a self,
        ctx: RateLimitContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitPermit, ApiClientError>> {
        Box::pin(async move {
            self.wait_cooldown(&ctx).await?;

            if ctx.plan.is_empty() {
                return Ok(RateLimitPermit);
            }

            for bucket in ctx.plan.buckets() {
                let key = resolve_key(&ctx, &bucket.key)?;
                for window in &bucket.windows {
                    let spec = GovernorWindowSpec {
                        id: bucket.id.clone(),
                        key: key.clone(),
                        window: window.clone(),
                    };
                    let limiter = self.limiter_for(&ctx, spec)?;
                    limiter.until_n_ready(bucket.cost).await.map_err(|_| {
                        rate_limit_acquire_error(&ctx, "rate-limit cost exceeds bucket capacity")
                    })?;
                }
            }

            Ok(RateLimitPermit)
        })
    }

    fn on_response<'a>(
        &'a self,
        ctx: RateLimitResponseContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitResponseAction, ApiClientError>> {
        Box::pin(async move {
            let observation = self.response_policy.observe(&ctx);
            self.store_observation(&ctx, observation)
        })
    }
}

fn quota_for_window(
    ctx: &RateLimitContext<'_>,
    window: &RateLimitWindow,
) -> Result<Quota, ApiClientError> {
    if window.per.is_zero() {
        return Err(rate_limit_configuration_error(
            ctx,
            "rate-limit window duration must not be zero",
        ));
    }
    let per_cell = window.per.checked_div(window.max.get()).ok_or_else(|| {
        rate_limit_configuration_error(ctx, "rate-limit window duration is too small for max")
    })?;
    if per_cell.is_zero() {
        return Err(rate_limit_configuration_error(
            ctx,
            "rate-limit window duration is too small for max",
        ));
    }
    Quota::with_period(per_cell)
        .map(|quota| quota.allow_burst(window.max))
        .ok_or_else(|| rate_limit_configuration_error(ctx, "invalid rate-limit quota"))
}

fn rate_limit_error(
    ctx: &RateLimitContext<'_>,
    kind: crate::rate_limit::RateLimitErrorKind,
    msg: &'static str,
) -> ApiClientError {
    ApiClientError::rate_limit(
        ErrorContext {
            endpoint: ctx.endpoint,
            method: ctx.method.clone(),
        },
        kind,
        msg,
    )
}

fn rate_limit_configuration_error(ctx: &RateLimitContext<'_>, msg: &'static str) -> ApiClientError {
    rate_limit_error(
        ctx,
        crate::rate_limit::RateLimitErrorKind::InvalidConfiguration,
        msg,
    )
}

fn rate_limit_invalid_key_error(ctx: &RateLimitContext<'_>, msg: &'static str) -> ApiClientError {
    rate_limit_error(ctx, crate::rate_limit::RateLimitErrorKind::InvalidKey, msg)
}

fn rate_limit_acquire_error(ctx: &RateLimitContext<'_>, msg: &'static str) -> ApiClientError {
    rate_limit_error(
        ctx,
        crate::rate_limit::RateLimitErrorKind::AcquireFailed,
        msg,
    )
}

fn rate_limit_internal_error(ctx: &RateLimitContext<'_>, msg: &'static str) -> ApiClientError {
    rate_limit_error(ctx, crate::rate_limit::RateLimitErrorKind::Internal, msg)
}

fn resolve_key(
    ctx: &RateLimitContext<'_>,
    key: &RateLimitKey,
) -> Result<ResolvedRateLimitKey, ApiClientError> {
    let mut parts = Vec::with_capacity(key.parts().len());
    for part in key.parts() {
        parts.push((part.name.clone(), resolve_key_part_value(ctx, part)?));
    }
    Ok(ResolvedRateLimitKey(parts))
}

fn resolve_key_part_value<'a>(
    ctx: &'a RateLimitContext<'_>,
    part: &'a RateLimitKeyPart,
) -> Result<Cow<'static, str>, ApiClientError> {
    match &part.value {
        RateLimitKeyValue::Static(value) => Ok(match value {
            Cow::Borrowed(value) => Cow::Borrowed(*value),
            Cow::Owned(value) => Cow::Owned(value.clone()),
        }),
        RateLimitKeyValue::Endpoint => Ok(Cow::Borrowed(ctx.endpoint)),
        RateLimitKeyValue::Method => Ok(Cow::Owned(ctx.method.as_str().to_string())),
        RateLimitKeyValue::UrlHost => ctx
            .url_host
            .map(|host| Cow::Owned(host.to_owned()))
            .ok_or_else(|| missing_host_key_error(ctx)),
    }
}

fn request_cooldown_key(ctx: &RateLimitContext<'_>) -> RateLimitCooldownKey {
    RateLimitCooldownKey::Request {
        method: ctx.method.clone(),
        endpoint: ctx.endpoint,
        url: ctx.url.to_owned(),
    }
}

fn client_cooldown_key() -> RateLimitCooldownKey {
    RateLimitCooldownKey::Client
}

fn host_cooldown_key(ctx: &RateLimitContext<'_>) -> Result<RateLimitCooldownKey, ApiClientError> {
    let host = ctx.url_host.ok_or_else(|| missing_host_key_error(ctx))?;
    Ok(RateLimitCooldownKey::Host {
        host: host.to_owned(),
    })
}

fn endpoint_cooldown_key(ctx: &RateLimitContext<'_>) -> RateLimitCooldownKey {
    RateLimitCooldownKey::Endpoint {
        method: ctx.method.clone(),
        endpoint: ctx.endpoint,
    }
}

fn bucket_kind_cooldown_key(
    ctx: &RateLimitContext<'_>,
    bucket: &RateLimitBucketUse,
) -> Result<RateLimitCooldownKey, ApiClientError> {
    Ok(RateLimitCooldownKey::BucketKind {
        kind: bucket.id.kind.clone(),
        key: resolve_key(ctx, &bucket.key)?,
    })
}

fn bucket_cooldown_key(
    ctx: &RateLimitContext<'_>,
    bucket: &RateLimitBucketUse,
) -> Result<RateLimitCooldownKey, ApiClientError> {
    Ok(RateLimitCooldownKey::Bucket {
        id: bucket.id.clone(),
        key: resolve_key(ctx, &bucket.key)?,
    })
}

fn cooldown_keys_for_acquire(
    ctx: &RateLimitContext<'_>,
) -> Result<Vec<RateLimitCooldownKey>, ApiClientError> {
    let mut keys = vec![
        client_cooldown_key(),
        endpoint_cooldown_key(ctx),
        request_cooldown_key(ctx),
    ];
    if ctx.url_host.is_some() {
        keys.push(host_cooldown_key(ctx)?);
    }
    for bucket in ctx.plan.buckets() {
        keys.push(bucket_kind_cooldown_key(ctx, bucket)?);
        keys.push(bucket_cooldown_key(ctx, bucket)?);
    }
    Ok(keys)
}

fn cooldown_keys_for_target(
    ctx: &RateLimitContext<'_>,
    target: &RateLimitTarget,
) -> Result<Vec<RateLimitCooldownKey>, ApiClientError> {
    match target {
        RateLimitTarget::None => Ok(Vec::new()),
        RateLimitTarget::Request => Ok(vec![request_cooldown_key(ctx)]),
        RateLimitTarget::Endpoint => Ok(vec![endpoint_cooldown_key(ctx)]),
        RateLimitTarget::Host => {
            if ctx.url_host.is_none() {
                return Err(missing_host_key_error(ctx));
            }
            Ok(vec![host_cooldown_key(ctx)?])
        }
        RateLimitTarget::Client => Ok(vec![client_cooldown_key()]),
        RateLimitTarget::CurrentPlan { fallback } => {
            if ctx.plan.is_empty() {
                cooldown_keys_for_target(ctx, fallback)
            } else {
                ctx.plan
                    .buckets()
                    .iter()
                    .map(|bucket| bucket_cooldown_key(ctx, bucket))
                    .collect()
            }
        }
        RateLimitTarget::BucketKind { kind, fallback } => {
            let keys = ctx
                .plan
                .buckets()
                .iter()
                .filter(|bucket| bucket.id.kind.as_ref() == kind.as_ref())
                .map(|bucket| bucket_kind_cooldown_key(ctx, bucket))
                .collect::<Result<Vec<_>, _>>()?;
            if keys.is_empty() {
                cooldown_keys_for_target(ctx, fallback)
            } else {
                Ok(keys)
            }
        }
        RateLimitTarget::Bucket { id, fallback } => {
            let keys = ctx
                .plan
                .buckets()
                .iter()
                .filter(|bucket| &bucket.id == id)
                .map(|bucket| bucket_cooldown_key(ctx, bucket))
                .collect::<Result<Vec<_>, _>>()?;
            if keys.is_empty() {
                cooldown_keys_for_target(ctx, fallback)
            } else {
                Ok(keys)
            }
        }
    }
}

fn missing_host_key_error(ctx: &RateLimitContext<'_>) -> ApiClientError {
    rate_limit_invalid_key_error(
        ctx,
        "rate_limit key `[host]` requires request URL to have a host",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rate_limit::RateLimitPlan;
    use std::num::NonZeroU32;
    use std::panic::AssertUnwindSafe;

    fn test_context() -> RateLimitContext<'static> {
        static METHOD: http::Method = http::Method::GET;
        static URL: &str = "https://example.com/a";
        static ENDPOINT: &str = "TestEndpoint";

        let bucket = RateLimitBucketUse::new(
            "method",
            "test",
            RateLimitKey::new(vec![RateLimitKeyPart::static_value("k", "v")]),
        )
        .with_windows(vec![RateLimitWindow::new(
            NonZeroU32::new(10).expect("non-zero"),
            Duration::from_secs(10),
        )]);
        let plan = RateLimitPlan::from_buckets(vec![bucket]);
        RateLimitContext {
            endpoint: ENDPOINT,
            method: &METHOD,
            url: URL,
            url_host: Some("example.com"),
            attempt: 0,
            page_index: 0,
            idempotent: true,
            max_cooldown: Duration::from_secs(60),
            plan: Box::leak(Box::new(plan)),
        }
    }

    fn hostless_context(plan: RateLimitPlan) -> RateLimitContext<'static> {
        static METHOD: http::Method = http::Method::GET;
        static URL: &str = "urn:test:hostless";
        static ENDPOINT: &str = "HostlessEndpoint";

        RateLimitContext {
            endpoint: ENDPOINT,
            method: &METHOD,
            url: URL,
            url_host: None,
            attempt: 0,
            page_index: 0,
            idempotent: true,
            max_cooldown: Duration::from_secs(60),
            plan: Box::leak(Box::new(plan)),
        }
    }

    fn one_window_bucket(key: RateLimitKey) -> RateLimitBucketUse {
        RateLimitBucketUse::new("method", "test", key).with_windows(vec![RateLimitWindow::new(
            NonZeroU32::new(10).expect("non-zero"),
            Duration::from_secs(10),
        )])
    }

    fn poison_mutex<T>(mutex: &Mutex<T>) {
        let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let _guard = mutex.lock().expect("test mutex should be available");
            panic!("poison test mutex");
        }));
    }

    fn error_chain_contains(error: &(dyn std::error::Error + 'static), needle: &str) -> bool {
        let mut current = Some(error);
        while let Some(err) = current {
            if err.to_string().contains(needle) || format!("{err:?}").contains(needle) {
                return true;
            }
            current = err.source();
        }
        false
    }

    fn request_context_with_url(url: String) -> RateLimitContext<'static> {
        static METHOD: http::Method = http::Method::GET;
        static ENDPOINT: &str = "RequestCooldownEndpoint";
        let plan = Box::leak(Box::new(RateLimitPlan::default()));
        RateLimitContext {
            endpoint: ENDPOINT,
            method: &METHOD,
            url: Box::leak(url.into_boxed_str()),
            url_host: Some("example.com"),
            attempt: 0,
            page_index: 0,
            idempotent: true,
            max_cooldown: Duration::from_secs(60),
            plan,
        }
    }

    #[derive(Clone)]
    struct FixedResponsePolicy {
        observation: RateLimitObservation,
    }

    impl RateLimitResponsePolicy for FixedResponsePolicy {
        fn observe(&self, _ctx: &RateLimitResponseContext<'_>) -> RateLimitObservation {
            self.observation.clone()
        }
    }

    #[tokio::test]
    async fn empty_plan_acquire_succeeds_without_creating_governor_state() {
        let limiter = GovernorRateLimiter::new();
        limiter
            .acquire(request_context_with_url(
                "https://example.com/empty".to_string(),
            ))
            .await
            .expect("empty plan should acquire");

        let windows = limiter.windows.lock().expect("window lock");
        let cooldowns = limiter.cooldowns.lock().expect("cooldown lock");
        assert!(windows.windows.is_empty());
        assert!(cooldowns.is_empty());
    }

    #[tokio::test]
    async fn empty_plan_acquire_does_not_touch_poisoned_window_state() {
        let limiter = GovernorRateLimiter::new();
        poison_mutex(&limiter.windows);

        limiter
            .acquire(request_context_with_url(
                "https://example.com/empty".to_string(),
            ))
            .await
            .expect("empty plan should bypass window enforcement");
    }

    #[tokio::test]
    async fn empty_plan_acquire_still_respects_active_cooldown() {
        let limiter = GovernorRateLimiter::new();
        let ctx = request_context_with_url("https://example.com/cooling".to_string());
        {
            let mut guard = limiter.cooldowns.lock().expect("cooldown lock");
            guard.insert(
                request_cooldown_key(&ctx),
                Instant::now() + Duration::from_secs(120),
            );
        }

        let err = limiter
            .acquire(ctx)
            .await
            .expect_err("active cooldown should still apply to empty plans");

        assert!(matches!(err, ApiClientError::RateLimit { .. }));
        assert!(matches!(
            err.rate_limit_error().map(|err| err.kind()),
            Some(crate::rate_limit::RateLimitErrorKind::InvalidConfiguration)
        ));
        assert!(err.to_string().contains("configured maximum"));
    }

    #[tokio::test]
    async fn no_cooldown_target_observation_does_not_touch_cooldown_storage() {
        let limiter =
            GovernorRateLimiter::new().with_response_policy(Arc::new(FixedResponsePolicy {
                observation: RateLimitObservation::limited()
                    .with_delay(Duration::from_secs(1))
                    .with_target(RateLimitTarget::None),
            }));
        poison_mutex(&limiter.cooldowns);
        let headers = http::HeaderMap::new();
        let meta = test_context();
        let ctx = RateLimitResponseContext {
            meta,
            status: http::StatusCode::TOO_MANY_REQUESTS,
            headers: crate::debug::SanitizedHeaders::new(&headers),
            max_cooldown: Duration::from_secs(60),
        };

        let action = limiter
            .on_response(ctx)
            .await
            .expect("no-target cooldown observation should bypass storage");

        assert!(action.is_limited());
        assert!(!action.cooldown_stored());
    }

    #[test]
    fn rate_limit_host_key_requires_host() {
        let plan = RateLimitPlan::from_buckets(vec![one_window_bucket(RateLimitKey::new(vec![
            RateLimitKeyPart::url_host(),
        ]))]);
        let ctx = hostless_context(plan);
        let err = resolve_key(&ctx, &ctx.plan.buckets()[0].key)
            .expect_err("host key should require a request host");
        let msg = err.to_string();
        assert!(msg.contains("host"));
        assert!(!msg.contains(concat!("unknown", "-", "host")));
    }

    #[tokio::test]
    async fn rate_limit_host_key_failure_happens_before_permit() {
        let plan = RateLimitPlan::from_buckets(vec![one_window_bucket(RateLimitKey::new(vec![
            RateLimitKeyPart::url_host(),
        ]))]);
        let ctx = hostless_context(plan);
        let limiter = GovernorRateLimiter::new();

        let err = limiter
            .acquire(ctx)
            .await
            .expect_err("hostless host key should fail before permit acquisition");
        let msg = err.to_string();
        assert!(msg.contains("rate_limit key `[host]`"));
        assert!(!msg.contains(concat!("unknown", "-", "host")));
        assert!(!msg.contains("<unknown>"));
    }

    #[test]
    fn rate_limit_cooldown_overflow_returns_typed_error() {
        let limiter = GovernorRateLimiter::new();
        let ctx = test_context();
        let err = limiter
            .store_cooldown(
                &ctx,
                &RateLimitTarget::Endpoint,
                Duration::MAX,
                Duration::MAX,
            )
            .expect_err("overflowing cooldown should fail");

        assert!(matches!(err, ApiClientError::RateLimit { .. }));
        assert!(matches!(
            err.rate_limit_error().map(|err| err.kind()),
            Some(crate::rate_limit::RateLimitErrorKind::InvalidConfiguration)
        ));
        assert!(err.to_string().contains("cooldown duration overflowed"));
    }

    #[test]
    fn rate_limit_cooldown_cap_returns_typed_error() {
        let limiter = GovernorRateLimiter::new();
        let ctx = test_context();
        let err = limiter
            .store_cooldown(
                &ctx,
                &RateLimitTarget::Endpoint,
                Duration::from_secs(61),
                Duration::from_secs(60),
            )
            .expect_err("over-cap cooldown should fail");

        assert!(matches!(err, ApiClientError::RateLimit { .. }));
        assert!(matches!(
            err.rate_limit_error().map(|err| err.kind()),
            Some(crate::rate_limit::RateLimitErrorKind::InvalidConfiguration)
        ));
        assert!(err.to_string().contains("configured maximum"));
    }

    #[test]
    fn default_cooldown_entry_cap_is_finite() {
        const {
            assert!(GovernorRateLimiter::DEFAULT_MAX_COOLDOWN_ENTRIES > 0);
        }
    }

    #[test]
    fn cooldown_entry_cap_allows_entries_up_to_cap() {
        let limiter = GovernorRateLimiter::new().with_max_cooldown_entries(2);

        for idx in 0..2 {
            let ctx = request_context_with_url(format!("https://example.com/request-{idx}"));
            let stored = limiter
                .store_cooldown(
                    &ctx,
                    &RateLimitTarget::Request,
                    Duration::from_secs(1),
                    Duration::from_secs(60),
                )
                .expect("cooldown below cap should store");
            assert!(stored);
        }

        let guard = limiter.cooldowns.lock().expect("cooldown lock");
        assert_eq!(guard.len(), 2);
    }

    #[test]
    fn cooldown_entry_cap_fails_closed_for_new_distinct_key() {
        let limiter = GovernorRateLimiter::new().with_max_cooldown_entries(2);

        for idx in 0..2 {
            let ctx = request_context_with_url(format!("https://example.com/request-{idx}"));
            limiter
                .store_cooldown(
                    &ctx,
                    &RateLimitTarget::Request,
                    Duration::from_secs(1),
                    Duration::from_secs(60),
                )
                .expect("cooldown below cap should store");
        }

        let secret_key_material = "SECRET_RATE_LIMIT_COOLDOWN_KEY_MUST_NOT_APPEAR";
        let ctx = request_context_with_url(format!("https://example.com/{secret_key_material}"));
        let err = limiter
            .store_cooldown(
                &ctx,
                &RateLimitTarget::Request,
                Duration::from_secs(1),
                Duration::from_secs(60),
            )
            .expect_err("new cooldown above cap should fail closed");

        assert!(matches!(err, ApiClientError::RateLimit { .. }));
        assert!(matches!(
            err.rate_limit_error().map(|err| err.kind()),
            Some(crate::rate_limit::RateLimitErrorKind::InvalidConfiguration)
        ));
        assert!(err.to_string().contains("cooldown entry cap exceeded"));
        assert!(!err.to_string().contains(secret_key_material));
        assert!(!format!("{err:?}").contains(secret_key_material));
        assert!(!error_chain_contains(&err, secret_key_material));
        let guard = limiter.cooldowns.lock().expect("cooldown lock");
        assert_eq!(guard.len(), 2);
    }

    #[test]
    fn expired_cooldown_entries_are_pruned_before_cap_enforcement() {
        let limiter = GovernorRateLimiter::new().with_max_cooldown_entries(1);
        let old_ctx = request_context_with_url("https://example.com/old".to_string());
        {
            let mut guard = limiter.cooldowns.lock().expect("cooldown lock");
            guard.insert(
                request_cooldown_key(&old_ctx),
                Instant::now() - Duration::from_secs(1),
            );
        }

        let new_ctx = request_context_with_url("https://example.com/new".to_string());
        limiter
            .store_cooldown(
                &new_ctx,
                &RateLimitTarget::Request,
                Duration::from_secs(1),
                Duration::from_secs(60),
            )
            .expect("expired cooldown should be pruned before cap check");

        let guard = limiter.cooldowns.lock().expect("cooldown lock");
        assert_eq!(guard.len(), 1);
        assert!(guard.contains_key(&request_cooldown_key(&new_ctx)));
    }

    #[test]
    fn updating_existing_cooldown_key_does_not_consume_new_capacity() {
        let limiter = GovernorRateLimiter::new().with_max_cooldown_entries(1);
        let ctx = request_context_with_url("https://example.com/repeated".to_string());

        limiter
            .store_cooldown(
                &ctx,
                &RateLimitTarget::Request,
                Duration::from_millis(1),
                Duration::from_secs(60),
            )
            .expect("initial cooldown should store");
        limiter
            .store_cooldown(
                &ctx,
                &RateLimitTarget::Request,
                Duration::from_secs(1),
                Duration::from_secs(60),
            )
            .expect("same cooldown key should update even at cap");

        let guard = limiter.cooldowns.lock().expect("cooldown lock");
        assert_eq!(guard.len(), 1);
    }

    #[test]
    fn duplicate_new_cooldown_keys_count_as_one_new_entry() {
        let limiter = GovernorRateLimiter::new().with_max_cooldown_entries(1);
        let key = RateLimitKey::new(vec![RateLimitKeyPart::static_value("tenant", "same")]);
        let plan = RateLimitPlan::from_buckets(vec![
            one_window_bucket(key.clone()),
            one_window_bucket(key),
        ]);
        let ctx = hostless_context(plan);

        let stored = limiter
            .store_cooldown(
                &ctx,
                &RateLimitTarget::current_plan_or_endpoint(),
                Duration::from_secs(1),
                Duration::from_secs(60),
            )
            .expect("duplicate new cooldown keys should consume one entry");

        assert!(stored);
        let guard = limiter.cooldowns.lock().expect("cooldown lock");
        assert_eq!(guard.len(), 1);
        assert!(
            guard.contains_key(&bucket_cooldown_key(&ctx, &ctx.plan.buckets()[0]).expect("key"))
        );
    }

    #[test]
    fn no_cooldown_observation_path_is_unaffected_by_entry_cap() {
        let limiter = GovernorRateLimiter::new().with_max_cooldown_entries(1);
        let ctx = test_context();
        let stored = limiter
            .store_cooldown(
                &ctx,
                &RateLimitTarget::None,
                Duration::from_secs(1),
                Duration::from_secs(60),
            )
            .expect("no-target cooldown observation should not fail");

        assert!(!stored);
        let guard = limiter.cooldowns.lock().expect("cooldown lock");
        assert!(guard.is_empty());
    }

    #[test]
    fn cooldown_targets_remain_distinct_for_request_endpoint_client_and_host() {
        let limiter = GovernorRateLimiter::new().with_max_cooldown_entries(8);
        let ctx = request_context_with_url("https://example.com/distinct".to_string());
        let targets = [
            RateLimitTarget::Request,
            RateLimitTarget::Endpoint,
            RateLimitTarget::Client,
            RateLimitTarget::Host,
        ];

        for target in targets {
            let stored = limiter
                .store_cooldown(
                    &ctx,
                    &target,
                    Duration::from_secs(1),
                    Duration::from_secs(60),
                )
                .expect("distinct cooldown target should store");
            assert!(stored);
        }

        let guard = limiter.cooldowns.lock().expect("cooldown lock");
        assert_eq!(guard.len(), 4);
    }

    #[test]
    fn rate_limit_endpoint_key_allows_hostless_url() {
        let plan = RateLimitPlan::from_buckets(vec![one_window_bucket(RateLimitKey::new(vec![
            RateLimitKeyPart::endpoint(),
        ]))]);
        let ctx = hostless_context(plan);
        let key = resolve_key(&ctx, &ctx.plan.buckets()[0].key)
            .expect("endpoint key should not require host");
        assert_eq!(
            key,
            ResolvedRateLimitKey(vec![("endpoint".into(), "HostlessEndpoint".into())])
        );
    }

    #[test]
    fn rate_limit_static_key_allows_hostless_url() {
        let plan = RateLimitPlan::from_buckets(vec![one_window_bucket(RateLimitKey::new(vec![
            RateLimitKeyPart::static_value("tenant", "public"),
        ]))]);
        let ctx = hostless_context(plan);
        let key = resolve_key(&ctx, &ctx.plan.buckets()[0].key)
            .expect("static key should not require host");
        assert_eq!(
            key,
            ResolvedRateLimitKey(vec![("tenant".into(), "public".into())])
        );
    }

    #[test]
    fn rate_limit_method_key_allows_hostless_url() {
        let plan = RateLimitPlan::from_buckets(vec![one_window_bucket(RateLimitKey::new(vec![
            RateLimitKeyPart::method(),
        ]))]);
        let ctx = hostless_context(plan);
        let key = resolve_key(&ctx, &ctx.plan.buckets()[0].key)
            .expect("method key should not require host");
        assert_eq!(
            key,
            ResolvedRateLimitKey(vec![("method".into(), "GET".into())])
        );
    }

    #[tokio::test]
    async fn poisoned_rate_limit_window_lock_returns_typed_error() {
        let limiter = GovernorRateLimiter::new();
        poison_mutex(&limiter.windows);

        let err = limiter
            .acquire(test_context())
            .await
            .expect_err("poisoned rate-limit window lock should fail");
        assert!(matches!(err, ApiClientError::RateLimit { .. }));
        assert!(matches!(
            err.rate_limit_error().map(|err| err.kind()),
            Some(crate::rate_limit::RateLimitErrorKind::Internal)
        ));
        assert!(err.to_string().contains("rate limit window lock poisoned"));
    }

    #[tokio::test]
    async fn poisoned_rate_limit_cooldown_lock_returns_typed_error() {
        let limiter = GovernorRateLimiter::new();
        poison_mutex(&limiter.cooldowns);

        let err = limiter
            .acquire(test_context())
            .await
            .expect_err("poisoned rate-limit cooldown lock should fail");
        assert!(matches!(err, ApiClientError::RateLimit { .. }));
        assert!(matches!(
            err.rate_limit_error().map(|err| err.kind()),
            Some(crate::rate_limit::RateLimitErrorKind::Internal)
        ));
        assert!(
            err.to_string()
                .contains("rate limit cooldown lock poisoned")
        );
    }

    #[test]
    fn window_entry_cap_is_enforced() {
        let limiter = GovernorRateLimiter::new()
            .with_max_window_entries(1)
            .with_window_idle_ttl(Duration::ZERO);
        let ctx = test_context();

        let spec_a = GovernorWindowSpec {
            id: RateLimitBucketId::new("method", "a"),
            key: ResolvedRateLimitKey(vec![("k".into(), "a".into())]),
            window: RateLimitWindow::new(
                NonZeroU32::new(10).expect("non-zero"),
                Duration::from_secs(10),
            ),
        };
        let spec_b = GovernorWindowSpec {
            id: RateLimitBucketId::new("method", "b"),
            key: ResolvedRateLimitKey(vec![("k".into(), "b".into())]),
            window: RateLimitWindow::new(
                NonZeroU32::new(10).expect("non-zero"),
                Duration::from_secs(10),
            ),
        };

        let _ = limiter.limiter_for(&ctx, spec_a).expect("first limiter");
        let _ = limiter.limiter_for(&ctx, spec_b).expect("second limiter");

        let guard = limiter.windows.lock().expect("window lock");
        assert_eq!(guard.windows.len(), 1);
    }

    #[test]
    fn idle_window_entries_are_pruned() {
        let limiter = GovernorRateLimiter::new()
            .with_max_window_entries(8)
            .with_window_idle_ttl(Duration::from_millis(1));
        let ctx = test_context();

        let spec_a = GovernorWindowSpec {
            id: RateLimitBucketId::new("method", "a"),
            key: ResolvedRateLimitKey(vec![("k".into(), "a".into())]),
            window: RateLimitWindow::new(
                NonZeroU32::new(10).expect("non-zero"),
                Duration::from_secs(10),
            ),
        };
        let spec_b = GovernorWindowSpec {
            id: RateLimitBucketId::new("method", "b"),
            key: ResolvedRateLimitKey(vec![("k".into(), "b".into())]),
            window: RateLimitWindow::new(
                NonZeroU32::new(10).expect("non-zero"),
                Duration::from_secs(10),
            ),
        };

        let _ = limiter
            .limiter_for(&ctx, spec_a.clone())
            .expect("first limiter");
        std::thread::sleep(Duration::from_millis(5));
        let _ = limiter.limiter_for(&ctx, spec_b).expect("second limiter");

        let guard = limiter.windows.lock().expect("window lock");
        assert_eq!(guard.windows.len(), 1);
        assert!(!guard.windows.contains_key(&spec_a));
    }
}

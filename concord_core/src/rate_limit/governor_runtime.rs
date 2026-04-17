use super::limiter::RateLimitFuture;
use super::{
    DefaultRateLimitResponsePolicy, RateLimitBucketId, RateLimitBucketUse, RateLimitContext,
    RateLimitKey, RateLimitKeyPart, RateLimitKeyValue, RateLimitObservation, RateLimitPermit,
    RateLimitResponseAction, RateLimitResponseContext, RateLimitResponsePolicy, RateLimitTarget,
    RateLimitWindow, RateLimiter,
};
use crate::error::{ApiClientError, ErrorContext};
use governor::{DefaultDirectRateLimiter, Quota, RateLimiter as Governor};
use std::collections::HashMap;
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
struct ResolvedRateLimitKey(Vec<(String, String)>);

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
struct RateLimitCooldownKey(String);

pub struct GovernorRateLimiter {
    windows: Mutex<HashMap<GovernorWindowSpec, GovernorWindowEntry>>,
    cooldowns: Mutex<HashMap<RateLimitCooldownKey, Instant>>,
    response_policy: Arc<dyn RateLimitResponsePolicy>,
    max_window_entries: usize,
    window_idle_ttl: Duration,
}

#[derive(Clone)]
struct GovernorWindowEntry {
    limiter: Arc<DefaultDirectRateLimiter>,
    last_used: Instant,
}

impl Default for GovernorRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl GovernorRateLimiter {
    const DEFAULT_MAX_WINDOW_ENTRIES: usize = 4096;
    const DEFAULT_WINDOW_IDLE_TTL: Duration = Duration::from_secs(15 * 60);

    pub fn new() -> Self {
        Self {
            windows: Mutex::new(HashMap::new()),
            cooldowns: Mutex::new(HashMap::new()),
            response_policy: Arc::new(DefaultRateLimitResponsePolicy),
            max_window_entries: Self::DEFAULT_MAX_WINDOW_ENTRIES,
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

    pub fn with_window_idle_ttl(mut self, window_idle_ttl: Duration) -> Self {
        self.window_idle_ttl = window_idle_ttl;
        self
    }

    fn limiter_for(
        &self,
        ctx: &RateLimitContext<'_>,
        spec: GovernorWindowSpec,
    ) -> Result<Arc<DefaultDirectRateLimiter>, ApiClientError> {
        let mut guard = self.windows.lock().expect("rate limit window lock");
        let now = Instant::now();
        self.prune_windows(&mut guard, now);
        if let Some(existing) = guard.get_mut(&spec) {
            existing.last_used = now;
            return Ok(existing.limiter.clone());
        }

        if guard.len() >= self.max_window_entries {
            let excess = guard.len() + 1 - self.max_window_entries;
            self.evict_oldest_windows(&mut guard, excess);
        }

        let quota = quota_for_window(ctx, &spec.window)?;
        let limiter = Arc::new(Governor::direct(quota));
        guard.insert(
            spec,
            GovernorWindowEntry {
                limiter: limiter.clone(),
                last_used: now,
            },
        );
        Ok(limiter)
    }

    fn prune_windows(
        &self,
        windows: &mut HashMap<GovernorWindowSpec, GovernorWindowEntry>,
        now: Instant,
    ) {
        if !self.window_idle_ttl.is_zero() {
            windows.retain(|_, entry| {
                now.saturating_duration_since(entry.last_used) <= self.window_idle_ttl
            });
        }

        if windows.len() > self.max_window_entries {
            self.evict_oldest_windows(windows, windows.len() - self.max_window_entries);
        }
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

    async fn wait_cooldown(&self, ctx: &RateLimitContext<'_>) {
        loop {
            let now = Instant::now();
            let delay = {
                let keys = cooldown_keys_for_acquire(ctx);
                let mut guard = self.cooldowns.lock().expect("rate limit cooldown lock");
                guard.retain(|_, until| *until > now);
                keys.into_iter()
                    .filter_map(|key| guard.get(&key).copied())
                    .filter_map(|until| until.checked_duration_since(now))
                    .max()
            };

            let Some(delay) = delay else {
                return;
            };
            if delay.is_zero() {
                return;
            }
            tokio::time::sleep(delay).await;
        }
    }

    fn store_observation(
        &self,
        ctx: &RateLimitResponseContext<'_>,
        observation: RateLimitObservation,
    ) -> RateLimitResponseAction {
        if !observation.limited {
            return RateLimitResponseAction::Continue;
        }

        let mut cooldown_stored = false;
        if let Some(delay) = observation.delay
            && !delay.is_zero()
        {
            cooldown_stored = self.store_cooldown(&ctx.meta, &observation.target, delay);
        }

        RateLimitResponseAction::Limited {
            retry_after: observation.delay,
            target: observation.target,
            cooldown_stored,
        }
    }

    fn store_cooldown(
        &self,
        ctx: &RateLimitContext<'_>,
        target: &RateLimitTarget,
        delay: std::time::Duration,
    ) -> bool {
        let keys = cooldown_keys_for_target(ctx, target);
        if keys.is_empty() {
            return false;
        }

        let until = Instant::now() + delay;
        let mut guard = self.cooldowns.lock().expect("rate limit cooldown lock");
        for key in keys {
            let entry = guard.entry(key).or_insert(until);
            if *entry < until {
                *entry = until;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rate_limit::RateLimitPlan;
    use std::num::NonZeroU32;

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
            plan: Box::leak(Box::new(plan)),
        }
    }

    #[test]
    fn window_entry_cap_is_enforced() {
        let limiter = GovernorRateLimiter::new()
            .with_max_window_entries(1)
            .with_window_idle_ttl(Duration::ZERO);
        let ctx = test_context();

        let spec_a = GovernorWindowSpec {
            id: RateLimitBucketId::new("method", "a"),
            key: ResolvedRateLimitKey(vec![("k".to_string(), "a".to_string())]),
            window: RateLimitWindow::new(
                NonZeroU32::new(10).expect("non-zero"),
                Duration::from_secs(10),
            ),
        };
        let spec_b = GovernorWindowSpec {
            id: RateLimitBucketId::new("method", "b"),
            key: ResolvedRateLimitKey(vec![("k".to_string(), "b".to_string())]),
            window: RateLimitWindow::new(
                NonZeroU32::new(10).expect("non-zero"),
                Duration::from_secs(10),
            ),
        };

        let _ = limiter.limiter_for(&ctx, spec_a).expect("first limiter");
        let _ = limiter.limiter_for(&ctx, spec_b).expect("second limiter");

        let guard = limiter.windows.lock().expect("window lock");
        assert_eq!(guard.len(), 1);
    }

    #[test]
    fn idle_window_entries_are_pruned() {
        let limiter = GovernorRateLimiter::new()
            .with_max_window_entries(8)
            .with_window_idle_ttl(Duration::from_millis(1));
        let ctx = test_context();

        let spec_a = GovernorWindowSpec {
            id: RateLimitBucketId::new("method", "a"),
            key: ResolvedRateLimitKey(vec![("k".to_string(), "a".to_string())]),
            window: RateLimitWindow::new(
                NonZeroU32::new(10).expect("non-zero"),
                Duration::from_secs(10),
            ),
        };
        let spec_b = GovernorWindowSpec {
            id: RateLimitBucketId::new("method", "b"),
            key: ResolvedRateLimitKey(vec![("k".to_string(), "b".to_string())]),
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
        assert_eq!(guard.len(), 1);
        assert!(!guard.contains_key(&spec_a));
    }
}

impl RateLimiter for GovernorRateLimiter {
    fn acquire<'a>(
        &'a self,
        ctx: RateLimitContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitPermit, ApiClientError>> {
        Box::pin(async move {
            self.wait_cooldown(&ctx).await;

            for bucket in ctx.plan.buckets() {
                let key = resolve_key(&ctx, &bucket.key);
                for window in &bucket.windows {
                    let spec = GovernorWindowSpec {
                        id: bucket.id.clone(),
                        key: key.clone(),
                        window: window.clone(),
                    };
                    let limiter = self.limiter_for(&ctx, spec)?;
                    limiter.until_n_ready(bucket.cost).await.map_err(|_| {
                        rate_limit_policy_error(&ctx, "rate-limit cost exceeds bucket capacity")
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
            Ok(self.store_observation(&ctx, observation))
        })
    }
}

fn quota_for_window(
    ctx: &RateLimitContext<'_>,
    window: &RateLimitWindow,
) -> Result<Quota, ApiClientError> {
    if window.per.is_zero() {
        return Err(rate_limit_policy_error(
            ctx,
            "rate-limit window duration must not be zero",
        ));
    }
    let per_cell = window.per.checked_div(window.max.get()).ok_or_else(|| {
        rate_limit_policy_error(ctx, "rate-limit window duration is too small for max")
    })?;
    if per_cell.is_zero() {
        return Err(rate_limit_policy_error(
            ctx,
            "rate-limit window duration is too small for max",
        ));
    }
    Quota::with_period(per_cell)
        .map(|quota| quota.allow_burst(window.max))
        .ok_or_else(|| rate_limit_policy_error(ctx, "invalid rate-limit quota"))
}

fn rate_limit_policy_error(ctx: &RateLimitContext<'_>, msg: &'static str) -> ApiClientError {
    ApiClientError::PolicyViolation {
        ctx: ErrorContext {
            endpoint: ctx.endpoint,
            method: ctx.method.clone(),
        },
        msg,
    }
}

fn resolve_key(ctx: &RateLimitContext<'_>, key: &RateLimitKey) -> ResolvedRateLimitKey {
    ResolvedRateLimitKey(
        key.parts()
            .iter()
            .map(|part| {
                (
                    part.name.to_string(),
                    resolve_key_part_value(ctx, part).into_owned(),
                )
            })
            .collect(),
    )
}

fn resolve_key_part_value<'a>(
    ctx: &'a RateLimitContext<'_>,
    part: &'a RateLimitKeyPart,
) -> std::borrow::Cow<'a, str> {
    match &part.value {
        RateLimitKeyValue::Static(value) => std::borrow::Cow::Borrowed(value.as_ref()),
        RateLimitKeyValue::Endpoint => std::borrow::Cow::Borrowed(ctx.endpoint),
        RateLimitKeyValue::Method => std::borrow::Cow::Owned(ctx.method.as_str().to_string()),
        RateLimitKeyValue::UrlHost => {
            std::borrow::Cow::Borrowed(ctx.url_host.unwrap_or("<unknown-host>"))
        }
    }
}

fn request_cooldown_key(ctx: &RateLimitContext<'_>) -> RateLimitCooldownKey {
    RateLimitCooldownKey(format!(
        "request:{}:{}:{}",
        ctx.method, ctx.endpoint, ctx.url
    ))
}

fn client_cooldown_key() -> RateLimitCooldownKey {
    RateLimitCooldownKey("client".to_string())
}

fn host_cooldown_key(ctx: &RateLimitContext<'_>) -> RateLimitCooldownKey {
    RateLimitCooldownKey(format!("host:{}", ctx.url_host.unwrap_or("<unknown-host>")))
}

fn endpoint_cooldown_key(ctx: &RateLimitContext<'_>) -> RateLimitCooldownKey {
    RateLimitCooldownKey(format!(
        "endpoint:{}:{}:{}",
        ctx.url_host.unwrap_or("<unknown-host>"),
        ctx.method,
        ctx.endpoint
    ))
}

fn bucket_kind_cooldown_key(
    ctx: &RateLimitContext<'_>,
    bucket: &RateLimitBucketUse,
) -> RateLimitCooldownKey {
    let key = resolve_key(ctx, &bucket.key);
    RateLimitCooldownKey(format!("bucket-kind:{}:{:?}", bucket.id.kind.as_ref(), key))
}

fn bucket_cooldown_key(
    ctx: &RateLimitContext<'_>,
    bucket: &RateLimitBucketUse,
) -> RateLimitCooldownKey {
    let key = resolve_key(ctx, &bucket.key);
    RateLimitCooldownKey(format!(
        "bucket:{}:{}:{:?}",
        bucket.id.kind.as_ref(),
        bucket.id.name.as_ref(),
        key
    ))
}

fn cooldown_keys_for_acquire(ctx: &RateLimitContext<'_>) -> Vec<RateLimitCooldownKey> {
    let mut keys = vec![
        client_cooldown_key(),
        host_cooldown_key(ctx),
        endpoint_cooldown_key(ctx),
        request_cooldown_key(ctx),
    ];
    for bucket in ctx.plan.buckets() {
        keys.push(bucket_kind_cooldown_key(ctx, bucket));
        keys.push(bucket_cooldown_key(ctx, bucket));
    }
    keys
}

fn cooldown_keys_for_target(
    ctx: &RateLimitContext<'_>,
    target: &RateLimitTarget,
) -> Vec<RateLimitCooldownKey> {
    match target {
        RateLimitTarget::None => Vec::new(),
        RateLimitTarget::Request => vec![request_cooldown_key(ctx)],
        RateLimitTarget::Endpoint => vec![endpoint_cooldown_key(ctx)],
        RateLimitTarget::Host => vec![host_cooldown_key(ctx)],
        RateLimitTarget::Client => vec![client_cooldown_key()],
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
                .collect::<Vec<_>>();
            if keys.is_empty() {
                cooldown_keys_for_target(ctx, fallback)
            } else {
                keys
            }
        }
        RateLimitTarget::Bucket { id, fallback } => {
            let keys = ctx
                .plan
                .buckets()
                .iter()
                .filter(|bucket| &bucket.id == id)
                .map(|bucket| bucket_cooldown_key(ctx, bucket))
                .collect::<Vec<_>>();
            if keys.is_empty() {
                cooldown_keys_for_target(ctx, fallback)
            } else {
                keys
            }
        }
    }
}

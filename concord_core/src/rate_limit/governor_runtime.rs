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
    windows: Mutex<HashMap<GovernorWindowSpec, Arc<DefaultDirectRateLimiter>>>,
    cooldowns: Mutex<HashMap<RateLimitCooldownKey, Instant>>,
    response_policy: Arc<dyn RateLimitResponsePolicy>,
}

impl Default for GovernorRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl GovernorRateLimiter {
    pub fn new() -> Self {
        Self {
            windows: Mutex::new(HashMap::new()),
            cooldowns: Mutex::new(HashMap::new()),
            response_policy: Arc::new(DefaultRateLimitResponsePolicy),
        }
    }

    pub fn with_response_policy(mut self, policy: Arc<dyn RateLimitResponsePolicy>) -> Self {
        self.response_policy = policy;
        self
    }

    fn limiter_for(
        &self,
        ctx: &RateLimitContext<'_>,
        spec: GovernorWindowSpec,
    ) -> Result<Arc<DefaultDirectRateLimiter>, ApiClientError> {
        let mut guard = self.windows.lock().expect("rate limit window lock");
        if let Some(existing) = guard.get(&spec) {
            return Ok(existing.clone());
        }

        let quota = quota_for_window(ctx, &spec.window)?;
        let limiter = Arc::new(Governor::direct(quota));
        guard.insert(spec, limiter.clone());
        Ok(limiter)
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

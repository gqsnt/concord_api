use concord_core::advanced::{
    RateLimitBucketUse, RateLimitContext, RateLimitKey, RateLimitKeyPart, RateLimitPlan,
    RateLimitWindow, SanitizedHeaders,
};
use http::{HeaderMap, Method, StatusCode};
use std::borrow::Cow;
use std::num::NonZeroU32;
use std::time::Duration;

pub fn bucket(
    kind: impl Into<Cow<'static, str>>,
    name: impl Into<Cow<'static, str>>,
    key: RateLimitKey,
    max: u32,
    per: Duration,
) -> RateLimitBucketUse {
    RateLimitBucketUse::new(kind, name, key).with_window(RateLimitWindow::new(
        NonZeroU32::new(max).expect("benchmark rate limit max must be non-zero"),
        per,
    ))
}

pub fn single_bucket_plan(
    kind: impl Into<Cow<'static, str>>,
    name: impl Into<Cow<'static, str>>,
    key_parts: Vec<RateLimitKeyPart>,
) -> RateLimitPlan {
    RateLimitPlan::from_buckets(vec![bucket(
        kind,
        name,
        RateLimitKey::new(key_parts),
        10_000,
        Duration::from_secs(1),
    )])
}

pub fn multi_bucket_plan(
    buckets: usize,
    windows_per_bucket: usize,
    unique_keys: bool,
) -> RateLimitPlan {
    let mut plan = RateLimitPlan::new();
    for idx in 0..buckets {
        let key = if unique_keys {
            RateLimitKey::new(vec![RateLimitKeyPart::static_value(
                "key",
                format!("key-{idx}"),
            )])
        } else {
            RateLimitKey::new(vec![RateLimitKeyPart::static_value("key", "same")])
        };
        let mut bucket = RateLimitBucketUse::new("bench", format!("bucket-{idx}"), key)
            .with_cost(NonZeroU32::new(1).expect("non-zero"));
        bucket.windows = (0..windows_per_bucket)
            .map(|window_idx| {
                RateLimitWindow::new(
                    NonZeroU32::new(10_000).expect("non-zero"),
                    Duration::from_secs(1 + (window_idx as u64 % 2)),
                )
            })
            .collect();
        plan.push_bucket(bucket);
    }
    plan
}

pub fn context<'a>(
    endpoint: &'static str,
    method: &'a Method,
    url: &'a str,
    url_host: Option<&'a str>,
    plan: &'a RateLimitPlan,
) -> RateLimitContext<'a> {
    RateLimitContext {
        endpoint,
        method,
        url,
        url_host,
        page_index: 0,
        idempotent: *method == Method::GET || *method == Method::HEAD,
        max_cooldown: Duration::from_secs(1),
        plan,
    }
}

pub fn response_headers() -> HeaderMap {
    HeaderMap::new()
}

pub fn response_context<'a>(
    meta: RateLimitContext<'a>,
    status: StatusCode,
    headers: &'a HeaderMap,
) -> concord_core::advanced::RateLimitResponseContext<'a> {
    concord_core::advanced::RateLimitResponseContext {
        meta,
        status,
        headers: SanitizedHeaders::new(headers),
        max_cooldown: Duration::from_secs(1),
    }
}

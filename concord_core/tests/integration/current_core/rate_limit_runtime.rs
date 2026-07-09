use super::common::{
    MockResponse, MockTransport, TestAuthVars, TextEndpoint, client, retry_policy,
};
use crate::support::assert_error_chain_does_not_contain_any;
use concord_core::advanced::{
    ErrorContext, RateLimitBucketUse, RateLimitContext, RateLimitErrorKind, RateLimitKey,
    RateLimitKeyPart, RateLimitKeyValue, RateLimitPermit, RateLimitPlan, RateLimitResponseAction,
    RateLimitResponseContext, RateLimitWindow, RateLimiter,
};
use concord_core::error::ErrorCategory;
use concord_core::internal::ResolvedPolicy;
use concord_core::prelude::ApiClientError;
use http::{HeaderValue, Method, StatusCode};
use std::num::NonZeroU32;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
use std::time::Duration;
use tokio::sync::Mutex;

fn one_second_window() -> RateLimitWindow {
    RateLimitWindow::new(
        NonZeroU32::new(10).expect("non-zero"),
        Duration::from_secs(1),
    )
}

fn bucket(kind: &'static str, name: &'static str, key: RateLimitKey) -> RateLimitBucketUse {
    RateLimitBucketUse::new(kind, name, key).with_window(one_second_window())
}

fn plan_with_buckets(buckets: Vec<RateLimitBucketUse>) -> ResolvedPolicy {
    ResolvedPolicy {
        rate_limit: RateLimitPlan::from_buckets(buckets),
        ..Default::default()
    }
}

fn keyed_policy_headers(
    policy: &mut ResolvedPolicy,
    header_name: &'static str,
    value: &'static str,
) {
    policy
        .headers
        .insert(header_name, HeaderValue::from_static(value));
}

fn key_value_label(ctx: &RateLimitContext<'_>, part: &RateLimitKeyPart) -> String {
    let value = match &part.value {
        RateLimitKeyValue::Static(value) => value.as_ref().to_string(),
        RateLimitKeyValue::Endpoint => ctx.endpoint.to_string(),
        RateLimitKeyValue::Method => ctx.method.as_str().to_string(),
        RateLimitKeyValue::UrlHost => ctx.url_host.unwrap_or("<none>").to_string(),
    };
    format!("{}={value}", part.name)
}

fn bucket_label(ctx: &RateLimitContext<'_>, bucket: &RateLimitBucketUse) -> String {
    let parts = bucket
        .key
        .parts()
        .iter()
        .map(|part| key_value_label(ctx, part))
        .collect::<Vec<_>>()
        .join(",");
    let windows = bucket
        .windows
        .iter()
        .map(|window| format!("{}@{}s", window.max, window.per.as_secs()))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{}:{}:[{parts}]:cost={}:windows=[{windows}]",
        bucket.id.kind, bucket.id.name, bucket.cost
    )
}

fn plan_label(ctx: &RateLimitContext<'_>) -> String {
    ctx.plan
        .buckets()
        .iter()
        .map(|bucket| bucket_label(ctx, bucket))
        .collect::<Vec<_>>()
        .join("|")
}

#[derive(Clone)]
struct RecordingRateLimiter {
    events: Arc<Mutex<Vec<String>>>,
    acquire_count: Arc<AtomicUsize>,
    fail_on_acquire: Option<usize>,
}

impl RecordingRateLimiter {
    fn new(events: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            events,
            acquire_count: Arc::new(AtomicUsize::new(0)),
            fail_on_acquire: None,
        }
    }

    fn failing_on_acquire(events: Arc<Mutex<Vec<String>>>, fail_on_acquire: usize) -> Self {
        Self {
            events,
            acquire_count: Arc::new(AtomicUsize::new(0)),
            fail_on_acquire: Some(fail_on_acquire),
        }
    }
}

impl RateLimiter for RecordingRateLimiter {
    fn acquire<'a>(
        &'a self,
        ctx: RateLimitContext<'a>,
    ) -> concord_core::advanced::RateLimitFuture<'a, Result<RateLimitPermit, ApiClientError>> {
        let events = self.events.clone();
        let acquire_count = self.acquire_count.clone();
        let fail_on_acquire = self.fail_on_acquire;
        Box::pin(async move {
            let acquire = acquire_count.fetch_add(1, AtomicOrdering::SeqCst) + 1;
            let label = plan_label(&ctx);
            events
                .lock()
                .await
                .push(format!("rate_acquire#{acquire}:{label}"));
            if fail_on_acquire == Some(acquire) {
                return Err(ApiClientError::RateLimit {
                    ctx: ErrorContext {
                        endpoint: ctx.endpoint,
                        method: ctx.method.clone(),
                    },
                    source: concord_core::advanced::RateLimitError::new(
                        RateLimitErrorKind::AcquireFailed,
                        "rate-limit acquisition denied",
                    ),
                });
            }
            Ok(RateLimitPermit)
        })
    }

    fn on_response<'a>(
        &'a self,
        ctx: RateLimitResponseContext<'a>,
    ) -> concord_core::advanced::RateLimitFuture<'a, Result<RateLimitResponseAction, ApiClientError>>
    {
        let events = self.events.clone();
        Box::pin(async move {
            events
                .lock()
                .await
                .push(format!("rate_response#{}:{}", ctx.meta.attempt, ctx.status));
            Ok(RateLimitResponseAction::Continue)
        })
    }
}

#[tokio::test]
async fn rate_limit_acquires_planned_buckets_before_transport_and_preserves_order()
-> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let limiter = Arc::new(RecordingRateLimiter::new(events.clone()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "ok")],
    );
    let sent = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    client.configure(|cfg| {
        cfg.rate_limiter(limiter);
    });

    let mut policy = plan_with_buckets(vec![
        bucket(
            "runtime",
            "host",
            RateLimitKey::new(vec![RateLimitKeyPart::url_host()]),
        ),
        bucket(
            "runtime",
            "method",
            RateLimitKey::new(vec![RateLimitKeyPart::method()]),
        ),
        bucket(
            "runtime",
            "endpoint",
            RateLimitKey::new(vec![RateLimitKeyPart::endpoint()]),
        ),
        bucket(
            "runtime",
            "tenant",
            RateLimitKey::new(vec![RateLimitKeyPart::static_value("tenant", "tenant-a")]),
        ),
    ]);
    keyed_policy_headers(
        &mut policy,
        "x-rate-limit-sentinel",
        "RATE_LIMIT_REQUEST_SENTINEL",
    );

    let decoded = client
        .request(TextEndpoint {
            policy,
            ..Default::default()
        })
        .response()
        .await?;

    assert_eq!(decoded.value(), "ok");
    assert_eq!(sent.sent_count().await, 1);
    let events = events.lock().await.clone();
    let acquire = events
        .iter()
        .position(|event| event.starts_with("rate_acquire#1:"))
        .expect("rate-limit acquire should be recorded");
    let transport = events
        .iter()
        .position(|event| event == "transport")
        .expect("transport send should be recorded");
    assert!(acquire < transport);
    assert_eq!(
        events
            .iter()
            .find(|event| event.starts_with("rate_acquire#1:"))
            .expect("rate-limit acquire should record bucket order"),
        "rate_acquire#1:runtime:host:[route.host=example.com]:cost=1:windows=[10@1s]|runtime:method:[method=GET]:cost=1:windows=[10@1s]|runtime:endpoint:[endpoint=Text]:cost=1:windows=[10@1s]|runtime:tenant:[tenant=tenant-a]:cost=1:windows=[10@1s]"
    );
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 1);
    let rate_limit = &requests[0].rate_limit;
    assert_eq!(rate_limit.buckets().len(), 4);
    assert_eq!(rate_limit.buckets()[0].id.kind.as_ref(), "runtime");
    assert_eq!(rate_limit.buckets()[0].id.name.as_ref(), "host");
    assert_eq!(
        rate_limit.buckets()[0].key.parts()[0].value,
        RateLimitKeyValue::UrlHost
    );
    assert_eq!(rate_limit.buckets()[1].id.name.as_ref(), "method");
    assert_eq!(
        rate_limit.buckets()[1].key.parts()[0].value,
        RateLimitKeyValue::Method
    );
    assert_eq!(rate_limit.buckets()[2].id.name.as_ref(), "endpoint");
    assert_eq!(
        rate_limit.buckets()[2].key.parts()[0].value,
        RateLimitKeyValue::Endpoint
    );
    assert_eq!(rate_limit.buckets()[3].id.name.as_ref(), "tenant");
    assert_eq!(
        rate_limit.buckets()[3].key.parts()[0].value,
        RateLimitKeyValue::Static("tenant-a".into())
    );
    assert_eq!(
        requests[0]
            .headers
            .get("x-rate-limit-sentinel")
            .and_then(|value| value.to_str().ok()),
        Some("RATE_LIMIT_REQUEST_SENTINEL")
    );
    Ok(())
}

#[tokio::test]
async fn rate_limit_acquire_failure_reports_context_and_redacts_planned_sentinel() {
    const RATE_LIMIT_SENTINEL: &str = "RATE_LIMIT_ACQUIRE_SENTINEL";

    let events = Arc::new(Mutex::new(Vec::new()));
    let limiter = Arc::new(RecordingRateLimiter::failing_on_acquire(events.clone(), 1));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "unused")],
    );
    let sent = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    client.configure(|cfg| {
        cfg.rate_limiter(limiter);
    });

    let policy = plan_with_buckets(vec![bucket(
        "runtime",
        "tenant",
        RateLimitKey::new(vec![RateLimitKeyPart::static_value(
            "tenant",
            RATE_LIMIT_SENTINEL,
        )]),
    )]);
    let err = client
        .request(TextEndpoint {
            policy,
            ..Default::default()
        })
        .response()
        .await
        .expect_err("rate-limit acquire failure should be typed");

    assert!(matches!(err, ApiClientError::RateLimit { .. }));
    assert_eq!(err.category(), ErrorCategory::RateLimit);
    assert_eq!(err.context().endpoint, "Text");
    assert_eq!(err.context().method, Method::GET);
    assert_eq!(
        err.rate_limit_error().map(|err| err.kind()),
        Some(RateLimitErrorKind::AcquireFailed)
    );
    assert_eq!(sent.sent_count().await, 0);
    let events = events.lock().await.clone();
    assert!(
        events
            .iter()
            .any(|event| event.contains(RATE_LIMIT_SENTINEL)),
        "rate-limit path should record the sentinel before failure: {events:?}"
    );
    assert_error_chain_does_not_contain_any(&err, &[RATE_LIMIT_SENTINEL]);
}

#[tokio::test]
async fn rate_limit_retry_boundary_stops_after_late_acquire_failure() -> Result<(), ApiClientError>
{
    const REQUEST_SENTINEL: &str = "RATE_LIMIT_RETRY_REQUEST_SENTINEL";

    let events = Arc::new(Mutex::new(Vec::new()));
    let limiter = Arc::new(RecordingRateLimiter::failing_on_acquire(events.clone(), 2));
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, "retry-me"),
            MockResponse::text(StatusCode::OK, "should-not-send"),
        ],
    );
    let sent = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    client.configure(|cfg| {
        cfg.rate_limiter(limiter);
    });
    let mut policy = retry_policy(2);
    policy.rate_limit = RateLimitPlan::from_buckets(vec![bucket(
        "runtime",
        "endpoint",
        RateLimitKey::new(vec![RateLimitKeyPart::endpoint()]),
    )]);
    keyed_policy_headers(&mut policy, "x-rate-limit-sentinel", REQUEST_SENTINEL);

    let err = client
        .request(TextEndpoint {
            policy,
            ..Default::default()
        })
        .response()
        .await
        .expect_err("late rate-limit failure should stop retrying");

    assert!(matches!(err, ApiClientError::RateLimit { .. }));
    assert_eq!(err.category(), ErrorCategory::RateLimit);
    assert_eq!(err.context().endpoint, "Text");
    assert_eq!(err.context().method, Method::GET);
    assert_eq!(
        err.rate_limit_error().map(|err| err.kind()),
        Some(RateLimitErrorKind::AcquireFailed)
    );
    assert_eq!(sent.sent_count().await, 1);
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0]
            .headers
            .get("x-rate-limit-sentinel")
            .and_then(|value| value.to_str().ok()),
        Some(REQUEST_SENTINEL)
    );
    let events = events.lock().await.clone();
    let expected_label = "runtime:endpoint:[endpoint=Text]:cost=1:windows=[10@1s]";
    let acquires = events
        .iter()
        .filter(|event| event.starts_with("rate_acquire#"))
        .collect::<Vec<_>>();
    assert_eq!(acquires.len(), 2);
    assert_eq!(acquires[0], &format!("rate_acquire#1:{expected_label}"));
    assert_eq!(acquires[1], &format!("rate_acquire#2:{expected_label}"));
    assert_error_chain_does_not_contain_any(&err, &[REQUEST_SENTINEL]);
    Ok(())
}

#[tokio::test]
async fn empty_rate_limit_plan_carries_no_buckets_into_acquire() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let limiter = Arc::new(RecordingRateLimiter::new(events.clone()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "ok")],
    );
    let sent = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    client.configure(|cfg| {
        cfg.rate_limiter(limiter);
    });

    let decoded = client
        .request(TextEndpoint {
            policy: ResolvedPolicy::default(),
            ..Default::default()
        })
        .response()
        .await?;

    assert_eq!(decoded.value(), "ok");
    assert_eq!(sent.sent_count().await, 1);
    let events = events.lock().await.clone();
    assert_eq!(events[0], "rate_acquire#1:");
    assert!(events.iter().all(|event| !event.contains("windows=[")));
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 1);
    assert!(requests[0].rate_limit.is_empty());
    Ok(())
}

use std::error::Error;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use concord_core::advanced::{
    BoxFuture, Cooldown, CooldownScope, DefaultRateLimitObserver, NoopRateLimiter, RateLimitError,
    RateLimitErrorKind, RateLimitId, RateLimitKey, RateLimitKeyPart, RateLimitObserver,
    RateLimitPermit, RateLimitRequest, RateLimitResponse, RateLimitResponseHead, RateLimitRule,
    RateLimitWindow, RateLimiter, RuntimeConfigBuilder,
};

struct CustomLimiter;

impl RateLimiter for CustomLimiter {
    fn acquire<'a>(
        &'a self,
        request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        for rule in request.plan.rules() {
            let _ = (
                rule.id().as_str(),
                rule.key().is_global(),
                rule.key().total_bytes(),
                rule.key()
                    .parts()
                    .iter()
                    .map(|part| (part.name(), part.value()))
                    .collect::<Vec<_>>(),
                rule.cost(),
                rule.windows()
                    .iter()
                    .map(|window| (window.capacity(), window.duration()))
                    .collect::<Vec<_>>(),
            );
        }
        let _ = (
            request.endpoint,
            request.method,
            request.safe_url,
            request.host,
            request.page_index,
            request.plan.rules(),
            request.plan.is_empty(),
        );
        Box::pin(async { Ok(RateLimitPermit::granted()) })
    }

    fn observe<'a>(&'a self, response: RateLimitResponse<'a>) -> BoxFuture<'a, ()> {
        let _ = (
            response.request.endpoint,
            response.request.method,
            response.request.safe_url,
            response.request.host,
            response.request.page_index,
            response.request.plan.rules(),
            response.status,
            response.headers,
        );
        Box::pin(async {})
    }
}

struct CustomObserver;

impl RateLimitObserver for CustomObserver {
    fn observe(&self, response: &RateLimitResponseHead<'_>) -> Option<Cooldown> {
        for rule in response.plan.rules() {
            let _ = (
                rule.id().as_str(),
                rule.key()
                    .parts()
                    .iter()
                    .map(|part| (part.name(), part.value()))
                    .collect::<Vec<_>>(),
                rule.cost(),
                rule.windows()
                    .iter()
                    .map(|window| (window.capacity(), window.duration()))
                    .collect::<Vec<_>>(),
            );
        }
        let _ = (
            response.endpoint,
            response.method,
            response.safe_url,
            response.host,
            response.page_index,
            response.plan.rules(),
            response.status,
            &response.headers,
        );
        Some(Cooldown {
            duration: Duration::from_secs(1),
            scope: CooldownScope::CurrentBuckets,
        })
    }
}

fn configure_limiter(
    builder: RuntimeConfigBuilder,
    limiter: Arc<dyn RateLimiter>,
) -> RuntimeConfigBuilder {
    builder.rate_limiter(limiter)
}

fn observe_without_replacement<'a>(
    limiter: &'a dyn RateLimiter,
    response: RateLimitResponse<'a>,
) -> BoxFuture<'a, ()> {
    limiter.observe(response)
}

#[test]
fn custom_observation_contract_returns_only_completion() {
    let _: for<'a> fn(&'a dyn RateLimiter, RateLimitResponse<'a>) -> BoxFuture<'a, ()> =
        observe_without_replacement;
}

#[test]
fn extensions_construct_opaque_permits_and_typed_key_safe_errors() {
    let _permit = RateLimitPermit::granted();
    for kind in [
        RateLimitErrorKind::InvalidConfiguration,
        RateLimitErrorKind::InvalidKey,
        RateLimitErrorKind::Acquire,
        RateLimitErrorKind::Internal,
    ] {
        let error = RateLimitError::new(kind);
        assert_eq!(error.kind(), kind);
        assert!(error.source().is_none());
    }
    let safe = RateLimitError::with_safe_message(
        RateLimitErrorKind::Acquire,
        "operator-approved safe limiter message",
    );
    assert_eq!(safe.kind(), RateLimitErrorKind::Acquire);
    assert!(safe.source().is_none());
}

#[test]
fn custom_extensions_compile_with_body_free_plan_inspection() {
    let _: Arc<dyn RateLimiter> = Arc::new(CustomLimiter);
    let _: Arc<dyn RateLimitObserver> = Arc::new(CustomObserver);
}

#[test]
fn exact_advanced_rate_limit_constructors_accessors_and_traits_compile() {
    let id = RateLimitId::new("application").expect("rate-limit ID");
    assert_eq!(id.as_str(), "application");
    let first = RateLimitKeyPart::new("host", "api.example.test");
    let second = RateLimitKeyPart::new("vars.tenant", "north");
    let key = RateLimitKey::from_parts([first, second]).expect("ordered key");
    assert!(!key.is_global());
    assert_eq!(key.parts()[0].name(), "host");
    assert_eq!(key.parts()[0].value(), "api.example.test");
    assert_eq!(key.total_bytes(), 4 + 16 + 11 + 5);
    let global = RateLimitKey::global();
    assert!(global.is_global());
    assert_eq!(global.total_bytes(), 0);

    let window = RateLimitWindow::new(
        NonZeroU32::new(100).expect("capacity"),
        Duration::from_secs(60),
    )
    .expect("window");
    assert_eq!(window.capacity().get(), 100);
    assert_eq!(window.duration(), Duration::from_secs(60));
    let rule =
        RateLimitRule::new(id, key, NonZeroU32::new(2).expect("cost"), [window]).expect("rule");
    assert_eq!(rule.id().as_str(), "application");
    assert_eq!(rule.key().parts().len(), 2);
    assert_eq!(rule.cost().get(), 2);
    assert_eq!(rule.windows().len(), 1);

    let safe = RateLimitError::with_safe_message(
        RateLimitErrorKind::Acquire,
        "operator-approved safe limiter message",
    );
    assert_eq!(safe.kind(), RateLimitErrorKind::Acquire);
    assert!(
        safe.to_string()
            .contains("operator-approved safe limiter message")
    );
    assert!(safe.source().is_none());
    let structural = RateLimitError::new(RateLimitErrorKind::InvalidConfiguration);
    assert_eq!(structural.kind(), RateLimitErrorKind::InvalidConfiguration);

    let _: fn(RuntimeConfigBuilder, Arc<dyn RateLimiter>) -> RuntimeConfigBuilder =
        configure_limiter;
    let _: Arc<dyn RateLimiter> = Arc::new(CustomLimiter);
    let _: Arc<dyn RateLimiter> = Arc::new(NoopRateLimiter::new());
    let _: Arc<dyn RateLimitObserver> = Arc::new(CustomObserver);
    let _: Arc<dyn RateLimitObserver> = Arc::new(DefaultRateLimitObserver);
}

#[cfg(feature = "rate-limit-governor")]
#[test]
fn governor_configuration_surface_is_exact_and_finite() {
    use concord_core::advanced::GovernorRateLimiter;

    let _governor = GovernorRateLimiter::new()
        .observer(Arc::new(CustomObserver))
        .quota_capacity(1)
        .expect("quota capacity")
        .cooldown_capacity(1)
        .expect("cooldown capacity")
        .idle_ttl(Duration::from_secs(1))
        .expect("idle TTL")
        .key_limit(0)
        .expect("zero key limit");
}

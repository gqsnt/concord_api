use std::error::Error;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use concord_core::contracts::{
    BoxFuture, DEFAULT_RATE_LIMIT_KEY_BYTES, Enforcement, LimiterCapability, NoopRateLimiter,
    RateKeyEncoder, RateKeyPart, RateLimitDeclaration, RateLimitError, RateLimitErrorKind,
    RateLimitId, RateLimitKey, RateLimitPermit, RateLimitPlan, RateLimitPolicy, RateLimitRequest,
    RateLimitResponse, RateLimitWindow, RateLimiter, RuntimeConfigBuilder,
};
use concord_core::rate_limit::{
    CanonicalKeyWriter, DEFAULT_RATE_SCHEMA_REGISTRATIONS, ENDPOINT_LABEL, ORIGIN_LABEL,
    RATE_KEY_FRAMING_VERSION, RateDimensionKind, RatePlanSummary, RatePolicySchema,
    UTF8_ENCODING_ID, encode_canonical, encode_canonical_with, is_valid_encoding_id,
};

struct CustomLimiter;

impl RateLimiter for CustomLimiter {
    fn capability(&self) -> LimiterCapability {
        LimiterCapability::new(Enforcement::SharedPersistent, "com.example.limiter", 3)
    }

    fn acquire<'a>(
        &'a self,
        request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        for rule in request.plan.policies() {
            let schema: &RatePolicySchema = rule.schema();
            let _ = (
                rule.id().as_str(),
                rule.key().total_bytes(),
                rule.key()
                    .parts()
                    .iter()
                    .map(|part| (part.name(), part.kind(), part.encoding_id(), part.value()))
                    .collect::<Vec<_>>(),
                rule.cost(),
                rule.windows()
                    .iter()
                    .map(|window| (window.capacity(), window.duration()))
                    .collect::<Vec<_>>(),
                schema.id().as_str(),
                schema
                    .dimensions()
                    .iter()
                    .map(|dimension| (dimension.label(), dimension.kind(), dimension.encoding_id()))
                    .collect::<Vec<_>>(),
                schema
                    .windows()
                    .iter()
                    .map(|window| (window.duration(), window.capacity()))
                    .collect::<Vec<_>>(),
                schema.framing_version(),
                schema.uses_origin(),
            );
        }
        let _ = (
            request.endpoint,
            request.method,
            request.logical_origin,
            request.page_index,
            request.attempt.index,
            request.attempt.policy_retry_index,
            request.attempt.authentication_recovery,
            request.plan.policies(),
            request.plan.is_empty(),
            request.plan.uses_origin(),
            request.capability.enforcement(),
            request.capability.namespace(),
            request.capability.algorithm_version(),
        );
        Box::pin(async { Ok(RateLimitPermit::granted()) })
    }

    fn observe(&self, response: RateLimitResponse<'_>) {
        let _ = (
            response.request.endpoint,
            response.request.method,
            response.request.logical_origin,
            response.request.page_index,
            response.request.attempt,
            response.request.plan.policies(),
            response.request.capability,
            response.status,
            response.server_delay,
            response.cooldown_cap,
        );
    }
}

fn configure_limiter(
    builder: RuntimeConfigBuilder,
    limiter: Arc<dyn RateLimiter>,
) -> RuntimeConfigBuilder {
    builder.rate_limiter(limiter)
}

fn observe_without_replacement(limiter: &dyn RateLimiter, response: RateLimitResponse<'_>) {
    limiter.observe(response)
}

/// A downstream canonical key contract.
struct TenantId(u64);

impl RateKeyPart for TenantId {
    const ENCODING_ID: &'static str = "com.example.tenant/1";

    fn write_canonical(&self, out: &mut CanonicalKeyWriter) -> Result<(), RateLimitError> {
        out.write_tagged_bytes("com.example.tenant", &self.0.to_be_bytes())
    }
}

/// A downstream `via` encoder over a foreign type.
struct Lowercase;

impl RateKeyEncoder<str> for Lowercase {
    const ENCODING_ID: &'static str = "com.example.lowercase/1";

    fn write_canonical(value: &str, out: &mut CanonicalKeyWriter) -> Result<(), RateLimitError> {
        value.to_ascii_lowercase().write_canonical(out)
    }
}

#[test]
fn custom_observation_contract_returns_safe_completion() {
    let _: for<'a> fn(&'a dyn RateLimiter, RateLimitResponse<'a>) = observe_without_replacement;
}

#[test]
fn extensions_construct_opaque_permits_and_typed_key_safe_errors() {
    let _permit = RateLimitPermit::granted();
    for kind in [
        RateLimitErrorKind::InvalidConfiguration,
        RateLimitErrorKind::InvalidKey,
        RateLimitErrorKind::StateCapacity,
        RateLimitErrorKind::Acquire,
        RateLimitErrorKind::Internal,
        RateLimitErrorKind::SchemaConflict,
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
    let limiter: Arc<dyn RateLimiter> = Arc::new(CustomLimiter);
    assert_eq!(
        limiter.preparation_key_limit(),
        DEFAULT_RATE_LIMIT_KEY_BYTES
    );
    assert_eq!(
        limiter.capability().enforcement(),
        Enforcement::SharedPersistent
    );
    limiter.capability().validate().expect("valid capability");
    assert_eq!(
        NoopRateLimiter::new().capability().enforcement(),
        Enforcement::Unenforced
    );
}

#[test]
fn exact_rate_limit_contract_constructors_accessors_and_traits_compile() {
    let id = RateLimitId::new("application").expect("rate-limit ID");
    assert_eq!(id.as_str(), "application");
    let key = RateLimitKey::from_canonical_values([
        ("host", "api.example.test"),
        ("client.tenant", "north"),
    ])
    .expect("ordered canonical key");
    assert_eq!(key.parts()[0].name(), "host");
    assert_eq!(key.parts()[0].kind(), RateDimensionKind::Custom);
    assert_eq!(key.parts()[0].encoding_id(), UTF8_ENCODING_ID);
    assert_ne!(key.parts()[0].value(), b"api.example.test");
    assert!(key.total_bytes() > "host".len() + "api.example.test".len());
    assert!(RateLimitKey::from_canonical_values([(ORIGIN_LABEL, "x")]).is_err());
    assert!(RateLimitKey::from_canonical_values([(ENDPOINT_LABEL, "x")]).is_err());

    let heterogeneous = RateLimitKey::from_encoded_parts([
        (
            "tenant",
            encode_canonical(&TenantId(7), 64).expect("tenant"),
        ),
        (
            "region",
            encode_canonical_with::<Lowercase, str>("EU", 64).expect("region"),
        ),
    ])
    .expect("heterogeneous key");
    assert_eq!(
        heterogeneous.parts()[0].encoding_id(),
        "com.example.tenant/1"
    );
    assert_eq!(
        heterogeneous.parts()[1].encoding_id(),
        "com.example.lowercase/1"
    );
    assert_eq!(
        heterogeneous.parts()[1].value(),
        encode_canonical("eu", 64).expect("eu").as_bytes()
    );
    assert!(is_valid_encoding_id(TenantId::ENCODING_ID));

    let window = RateLimitWindow::new(
        NonZeroU32::new(100).expect("capacity"),
        Duration::from_secs(60),
    )
    .expect("window");
    assert_eq!(window.capacity().get(), 100);
    assert_eq!(window.duration(), Duration::from_secs(60));
    let rule =
        RateLimitPolicy::new(id, key, NonZeroU32::new(2).expect("cost"), [window]).expect("rule");
    assert_eq!(rule.id().as_str(), "application");
    assert_eq!(rule.key().parts().len(), 2);
    assert_eq!(rule.cost().get(), 2);
    assert_eq!(rule.windows().len(), 1);
    assert_eq!(rule.schema().framing_version(), RATE_KEY_FRAMING_VERSION);
    let plan = RateLimitPlan::try_from_policies([rule]).expect("bounded plan");
    assert_eq!(plan.policies().len(), 1);

    let declaration = RateLimitDeclaration::new("catalog.tenant.read")
        .cost(1)
        .window(120, Duration::from_secs(60))
        .window(5_000, Duration::from_secs(3_600))
        .origin()
        .endpoint()
        .part("tenant", "tenant-a")
        .part_with::<Lowercase, str>("region", "EU");
    declaration.validate().expect("valid declaration");
    assert_eq!(
        declaration.id().map(RateLimitId::as_str),
        Some("catalog.tenant.read")
    );
    let _: fn(&RatePlanSummary) -> &'static str = RatePlanSummary::enforcement_label;
    assert_eq!(DEFAULT_RATE_SCHEMA_REGISTRATIONS, 4_096);

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
}

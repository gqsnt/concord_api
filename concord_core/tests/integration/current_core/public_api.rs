use concord_core::{advanced, prelude};
use std::sync::Arc;

#[test]
fn prelude_surface_contains_normal_user_api() {
    fn assert_endpoint<E: prelude::Endpoint<super::common::TestCx>>() {}
    assert_endpoint::<super::common::TextEndpoint>();

    let _debug = prelude::DebugLevel::default();
    let _secret = prelude::SecretString::new("redacted");
    let _api_key = prelude::ApiKey::new("key");
    let _basic = prelude::BasicCredential::new("user", "pass");
}

#[test]
fn advanced_surface_contains_extension_api() {
    let mut cfg = advanced::RuntimeConfig::default();
    cfg.cache_store(Arc::new(advanced::NoopCacheStore));
    cfg.rate_limiter(Arc::new(advanced::NoopRateLimiter::new()));
    cfg.retry_policy(Arc::new(advanced::ConfiguredRetryPolicy::new(
        advanced::RetryConfig {
            max_attempts: 1,
            methods: Vec::new(),
            statuses: Vec::new(),
            transport_errors: Vec::new(),
            backoff: advanced::RetryBackoff::None,
            respect_retry_after: false,
            idempotency: advanced::RetryIdempotency::SafeMethodsOnly,
        },
    )));
    cfg.runtime_hooks(Arc::new(advanced::NoopRuntimeHooks));
    cfg.max_auth_retries(2);

    let _ctx_ty: Option<advanced::RateLimitResponseContext<'_>> = None;
    let _slot_ty: Option<
        advanced::CredentialSlot<super::common::TestCx, advanced::StaticBearerProvider>,
    > = None;
}

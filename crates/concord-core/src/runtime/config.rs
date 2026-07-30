use std::num::{NonZeroU32, NonZeroUsize};
use std::sync::Arc;
use std::time::Duration;

use crate::hooks::{DebugLevel, RuntimeHooks, default_hooks};
use crate::rate_limit::{MAX_RATE_LIMIT_KEY_BYTES, RateLimiter};
use crate::retry::RetryPolicy;
use crate::runtime::defaults::{
    DEFAULT_AUTH_FAILURE_COOLDOWN_SECONDS, DEFAULT_PAGINATION_IDENTITY_BYTES,
    DEFAULT_PAGINATION_MAX_ITEMS, DEFAULT_PAGINATION_MAX_PAGES, DEFAULT_PROVIDER_REQUEST_BYTES,
    DEFAULT_PROVIDER_RESPONSE_BYTES, DEFAULT_RATE_LIMIT_COOLDOWN_SECONDS,
    DEFAULT_REQUEST_BODY_BYTES, DEFAULT_RESPONSE_BODY_BYTES, DEFAULT_STATUS_BODY_BYTES,
    DEFAULT_STREAM_RESPONSE_BYTES,
};

/// Private immutable finite runtime configuration.
pub(crate) struct RuntimeConfig {
    pub(crate) runtime_endpoint_timeout: Option<Duration>,
    pub(crate) request_body_limit: NonZeroUsize,
    pub(crate) response_body_limit: NonZeroUsize,
    pub(crate) stream_response_limit: NonZeroUsize,
    pub(crate) status_body_limit: Option<NonZeroUsize>,
    pub(crate) provider_request_limit: NonZeroUsize,
    pub(crate) provider_response_limit: NonZeroUsize,
    pub(crate) pagination_max_pages: NonZeroU32,
    pub(crate) pagination_max_items: NonZeroUsize,
    pub(crate) pagination_identity_limit: NonZeroUsize,
    pub(crate) auth_failure_cooldown_cap: Duration,
    pub(crate) rate_limit_cooldown_cap: Duration,
    pub(crate) rate_limit_key_limit: usize,
    pub(crate) rate_limiter: Arc<dyn RateLimiter>,
    pub(crate) retry_policy: Option<RetryPolicy>,
    pub(crate) hooks: Arc<dyn RuntimeHooks>,
    pub(crate) debug: DebugLevel,
}

/// A consuming builder whose ordinary limits remain positive and finite.
pub struct RuntimeConfigBuilder {
    config: RuntimeConfig,
    limiter_selection: LimiterSelection,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum LimiterSelection {
    Fallback,
    GeneratedDefault,
    Explicit,
}

impl RuntimeConfigBuilder {
    pub(crate) fn new() -> Self {
        let rate_limiter = crate::rate_limit::fallback_limiter();
        let rate_limit_key_limit = rate_limiter.preparation_key_limit();
        Self {
            config: RuntimeConfig {
                runtime_endpoint_timeout: None,
                request_body_limit: nonzero_usize(DEFAULT_REQUEST_BODY_BYTES),
                response_body_limit: nonzero_usize(DEFAULT_RESPONSE_BODY_BYTES),
                stream_response_limit: nonzero_usize(DEFAULT_STREAM_RESPONSE_BYTES),
                status_body_limit: Some(nonzero_usize(DEFAULT_STATUS_BODY_BYTES)),
                provider_request_limit: nonzero_usize(DEFAULT_PROVIDER_REQUEST_BYTES),
                provider_response_limit: nonzero_usize(DEFAULT_PROVIDER_RESPONSE_BYTES),
                pagination_max_pages: NonZeroU32::new(DEFAULT_PAGINATION_MAX_PAGES)
                    .expect("the pagination page default is nonzero"),
                pagination_max_items: nonzero_usize(DEFAULT_PAGINATION_MAX_ITEMS),
                pagination_identity_limit: nonzero_usize(DEFAULT_PAGINATION_IDENTITY_BYTES),
                auth_failure_cooldown_cap: Duration::from_secs(
                    DEFAULT_AUTH_FAILURE_COOLDOWN_SECONDS,
                ),
                rate_limit_cooldown_cap: Duration::from_secs(DEFAULT_RATE_LIMIT_COOLDOWN_SECONDS),
                rate_limit_key_limit,
                rate_limiter,
                retry_policy: None,
                hooks: default_hooks(),
                debug: DebugLevel::None,
            },
            limiter_selection: LimiterSelection::Fallback,
        }
    }

    /// Sets the default Concord operation timeout inherited by runtime endpoints.
    ///
    /// Generated endpoints retain their effective declaration timeout; request-local
    /// timeout selection can still replace or disable either default.
    #[must_use]
    pub fn runtime_endpoint_timeout(mut self, duration: Duration) -> Self {
        self.config.runtime_endpoint_timeout = Some(duration);
        self
    }

    /// Disables the default operation timeout inherited by runtime endpoints.
    ///
    /// Generated declaration timeouts and request-local timeout selections are
    /// unchanged.
    #[must_use]
    pub fn disable_runtime_endpoint_timeout(mut self) -> Self {
        self.config.runtime_endpoint_timeout = None;
        self
    }

    /// Sets the positive finite application request-body limit.
    #[must_use]
    pub fn request_body_limit(mut self, bytes: NonZeroUsize) -> Self {
        self.config.request_body_limit = bytes;
        self
    }

    /// Sets the positive finite buffered response-body limit.
    #[must_use]
    pub fn response_body_limit(mut self, bytes: NonZeroUsize) -> Self {
        self.config.response_body_limit = bytes;
        self
    }

    /// Sets the positive finite stream response-body limit.
    #[must_use]
    pub fn stream_response_limit(mut self, bytes: NonZeroUsize) -> Self {
        self.config.stream_response_limit = bytes;
        self
    }

    /// Sets and enables the positive finite terminal status-body limit.
    #[must_use]
    pub fn status_body_limit(mut self, bytes: NonZeroUsize) -> Self {
        self.config.status_body_limit = Some(bytes);
        self
    }

    /// Disables terminal status-body capture without creating an unbounded mode.
    #[must_use]
    pub fn disable_status_body_capture(mut self) -> Self {
        self.config.status_body_limit = None;
        self
    }

    /// Sets the positive finite pagination page ceiling.
    #[must_use]
    pub fn pagination_max_pages(mut self, pages: NonZeroU32) -> Self {
        self.config.pagination_max_pages = pages;
        self
    }

    /// Sets the positive finite pagination item ceiling.
    #[must_use]
    pub fn pagination_max_items(mut self, items: NonZeroUsize) -> Self {
        self.config.pagination_max_items = items;
        self
    }

    /// Sets the positive finite byte ceiling for complete canonical encoded
    /// pagination-identity vectors retained by cycle detection.
    ///
    /// The page ceiling separately bounds the number of retained identities;
    /// allocator and container overhead is not included in this byte total.
    #[must_use]
    pub fn pagination_identity_limit(mut self, bytes: NonZeroUsize) -> Self {
        self.config.pagination_identity_limit = bytes;
        self
    }

    /// Replaces the immutable lifecycle hook object used by the built client.
    #[must_use]
    pub fn hooks(mut self, hooks: Arc<dyn RuntimeHooks>) -> Self {
        self.config.hooks = hooks;
        self
    }

    /// Replaces the built-in body-free debug presentation level.
    #[must_use]
    pub fn debug(mut self, level: DebugLevel) -> Self {
        self.config.debug = level;
        self
    }

    /// Sets the finite authentication failure cooldown cap, including zero.
    #[must_use]
    pub fn auth_failure_cooldown_cap(mut self, duration: Duration) -> Self {
        self.config.auth_failure_cooldown_cap = duration;
        self
    }

    /// Sets the finite server-directed rate-limit cooldown cap, including zero.
    #[must_use]
    pub fn rate_limit_cooldown_cap(mut self, duration: Duration) -> Self {
        self.config.rate_limit_cooldown_cap = duration;
        self
    }

    /// Replaces the immutable limiter shared by the built client and its clones.
    #[must_use]
    pub fn rate_limiter(mut self, limiter: Arc<dyn RateLimiter>) -> Self {
        self.config.rate_limit_key_limit = limiter.preparation_key_limit();
        self.config.rate_limiter = limiter;
        self.limiter_selection = LimiterSelection::Explicit;
        self
    }

    /// Installs the explicit Concord-managed application retry policy.
    #[must_use]
    pub fn retry_policy(mut self, policy: RetryPolicy) -> Self {
        self.config.retry_policy = Some(policy);
        self
    }

    /// Disables Concord-managed application retry.
    #[must_use]
    pub fn disable_retry_policy(mut self) -> Self {
        self.config.retry_policy = None;
        self
    }

    /// Sets the positive finite provider request-body limit.
    #[must_use]
    pub fn provider_request_limit(mut self, bytes: NonZeroUsize) -> Self {
        self.config.provider_request_limit = bytes;
        self
    }

    /// Sets the positive finite provider response-body limit.
    #[must_use]
    pub fn provider_response_limit(mut self, bytes: NonZeroUsize) -> Self {
        self.config.provider_response_limit = bytes;
        self
    }

    pub(crate) fn resolve(self) -> RuntimeConfig {
        self.config
    }

    pub(crate) fn validate_rate_limit_key_limit(
        &self,
    ) -> Result<(), crate::error::ClientBuildError> {
        if self.config.rate_limit_key_limit > MAX_RATE_LIMIT_KEY_BYTES {
            Err(crate::error::ClientBuildError::new(
                crate::error::ClientBuildErrorKind::RuntimeConfiguration,
            ))
        } else {
            Ok(())
        }
    }

    pub(crate) fn install_generated_default_limiter(
        mut self,
        limiter: Arc<dyn RateLimiter>,
    ) -> Self {
        if self.limiter_selection == LimiterSelection::Explicit {
            return self;
        }
        self.config.rate_limit_key_limit = limiter.preparation_key_limit();
        self.config.rate_limiter = limiter;
        self.limiter_selection = LimiterSelection::GeneratedDefault;
        self
    }

    pub(crate) fn prepare_generated_rate_limits(
        self,
        requires_limiter: bool,
    ) -> Result<Self, crate::error::ClientBuildError> {
        self.validate_rate_limit_key_limit()?;
        if requires_limiter && self.limiter_selection == LimiterSelection::Fallback {
            return Err(crate::error::ClientBuildError::new(
                crate::error::ClientBuildErrorKind::LimiterUnavailable,
            ));
        }
        Ok(self)
    }
}

fn nonzero_usize(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).expect("normative runtime defaults are nonzero")
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FirstHooks;
    impl RuntimeHooks for FirstHooks {}

    struct SecondHooks;
    impl RuntimeHooks for SecondHooks {}

    #[test]
    fn defaults_are_exact_positive_finite_values() {
        let config = RuntimeConfigBuilder::new().resolve();
        assert_eq!(config.runtime_endpoint_timeout, None);
        assert_eq!(config.request_body_limit.get(), DEFAULT_REQUEST_BODY_BYTES);
        assert_eq!(
            config.response_body_limit.get(),
            DEFAULT_RESPONSE_BODY_BYTES
        );
        assert_eq!(
            config.stream_response_limit.get(),
            DEFAULT_STREAM_RESPONSE_BYTES
        );
        assert_eq!(
            config.status_body_limit.map(NonZeroUsize::get),
            Some(DEFAULT_STATUS_BODY_BYTES)
        );
        assert_eq!(
            config.provider_request_limit.get(),
            DEFAULT_PROVIDER_REQUEST_BYTES
        );
        assert_eq!(
            config.provider_response_limit.get(),
            DEFAULT_PROVIDER_RESPONSE_BYTES
        );
        assert_eq!(
            config.pagination_max_pages.get(),
            DEFAULT_PAGINATION_MAX_PAGES
        );
        assert_eq!(
            config.pagination_max_items.get(),
            DEFAULT_PAGINATION_MAX_ITEMS
        );
        assert_eq!(
            config.pagination_identity_limit.get(),
            DEFAULT_PAGINATION_IDENTITY_BYTES
        );
        assert_eq!(
            config.auth_failure_cooldown_cap,
            Duration::from_secs(DEFAULT_AUTH_FAILURE_COOLDOWN_SECONDS)
        );
        assert_eq!(
            config.rate_limit_cooldown_cap,
            Duration::from_secs(DEFAULT_RATE_LIMIT_COOLDOWN_SECONDS)
        );
        assert_eq!(
            config.rate_limit_key_limit,
            crate::rate_limit::DEFAULT_RATE_LIMIT_KEY_BYTES
        );
        assert_eq!(config.debug, DebugLevel::None);
        assert!(config.retry_policy.is_none());
    }

    #[test]
    fn selected_limiter_metadata_and_precedence_form_one_runtime_snapshot() {
        struct KeyBoundLimiter(usize);

        impl RateLimiter for KeyBoundLimiter {
            fn preparation_key_limit(&self) -> usize {
                self.0
            }

            fn acquire<'a>(
                &'a self,
                _request: crate::rate_limit::RateLimitRequest<'a>,
            ) -> crate::hooks::BoxFuture<
                'a,
                Result<crate::rate_limit::RateLimitPermit, crate::rate_limit::RateLimitError>,
            > {
                Box::pin(async { Ok(crate::rate_limit::RateLimitPermit::granted()) })
            }
        }

        for limit in [
            0,
            16,
            crate::rate_limit::DEFAULT_RATE_LIMIT_KEY_BYTES,
            2 * crate::rate_limit::DEFAULT_RATE_LIMIT_KEY_BYTES,
        ] {
            let limiter: Arc<dyn RateLimiter> = Arc::new(KeyBoundLimiter(limit));
            let config = RuntimeConfigBuilder::new()
                .rate_limiter(Arc::clone(&limiter))
                .resolve();
            assert_eq!(config.rate_limit_key_limit, limit);
            assert!(Arc::ptr_eq(&config.rate_limiter, &limiter));
        }

        let generated: Arc<dyn RateLimiter> = Arc::new(KeyBoundLimiter(17));
        let explicit: Arc<dyn RateLimiter> = Arc::new(KeyBoundLimiter(23));
        let config = RuntimeConfigBuilder::new()
            .install_generated_default_limiter(Arc::clone(&generated))
            .rate_limiter(Arc::clone(&explicit))
            .install_generated_default_limiter(Arc::clone(&generated))
            .resolve();
        assert_eq!(config.rate_limit_key_limit, 23);
        assert!(Arc::ptr_eq(&config.rate_limiter, &explicit));
    }

    #[test]
    fn runtime_and_generated_limiter_build_failures_have_exact_kinds() {
        struct ExcessiveKeyLimiter;

        impl RateLimiter for ExcessiveKeyLimiter {
            fn preparation_key_limit(&self) -> usize {
                MAX_RATE_LIMIT_KEY_BYTES + 1
            }

            fn acquire<'a>(
                &'a self,
                _request: crate::rate_limit::RateLimitRequest<'a>,
            ) -> crate::hooks::BoxFuture<
                'a,
                Result<crate::rate_limit::RateLimitPermit, crate::rate_limit::RateLimitError>,
            > {
                Box::pin(async { Ok(crate::rate_limit::RateLimitPermit::granted()) })
            }
        }

        let runtime = RuntimeConfigBuilder::new().rate_limiter(Arc::new(ExcessiveKeyLimiter));
        let error = runtime
            .validate_rate_limit_key_limit()
            .expect_err("excessive limiter key bound");
        assert_eq!(
            error.kind(),
            crate::error::ClientBuildErrorKind::RuntimeConfiguration
        );
        assert!(std::error::Error::source(&error).is_none());

        let error = match RuntimeConfigBuilder::new().prepare_generated_rate_limits(true) {
            Ok(_) => panic!("generated rate limits require an installed limiter"),
            Err(error) => error,
        };
        assert_eq!(
            error.kind(),
            crate::error::ClientBuildErrorKind::LimiterUnavailable
        );
        assert!(std::error::Error::source(&error).is_none());
    }

    #[test]
    fn setters_preserve_positive_finite_types_and_zero_duration_caps() {
        let one = NonZeroUsize::MIN;
        let one_page = NonZeroU32::MIN;
        let config = RuntimeConfigBuilder::new()
            .runtime_endpoint_timeout(Duration::from_secs(59))
            .request_body_limit(one)
            .response_body_limit(one)
            .stream_response_limit(one)
            .status_body_limit(one)
            .pagination_max_pages(one_page)
            .pagination_max_items(one)
            .pagination_identity_limit(one)
            .hooks(Arc::new(crate::hooks::NoopHooks))
            .debug(DebugLevel::Headers)
            .auth_failure_cooldown_cap(Duration::ZERO)
            .rate_limit_cooldown_cap(Duration::ZERO)
            .provider_request_limit(one)
            .provider_response_limit(one)
            .resolve();
        assert_eq!(
            config.runtime_endpoint_timeout,
            Some(Duration::from_secs(59))
        );
        assert_eq!(config.request_body_limit, one);
        assert_eq!(config.response_body_limit, one);
        assert_eq!(config.stream_response_limit, one);
        assert_eq!(config.status_body_limit, Some(one));
        assert_eq!(config.pagination_max_pages, one_page);
        assert_eq!(config.pagination_max_items, one);
        assert_eq!(config.pagination_identity_limit, one);
        assert_eq!(config.debug, DebugLevel::Headers);
        assert_eq!(config.auth_failure_cooldown_cap, Duration::ZERO);
        assert_eq!(config.rate_limit_cooldown_cap, Duration::ZERO);
        assert_eq!(config.provider_request_limit, one);
        assert_eq!(config.provider_response_limit, one);
    }

    #[test]
    fn status_capture_disablement_is_none_and_finite_limit_reenables() {
        let disabled = RuntimeConfigBuilder::new()
            .disable_status_body_capture()
            .resolve();
        assert_eq!(disabled.status_body_limit, None);

        let enabled = RuntimeConfigBuilder::new()
            .disable_status_body_capture()
            .status_body_limit(NonZeroUsize::MIN)
            .resolve();
        assert_eq!(enabled.status_body_limit, Some(NonZeroUsize::MIN));
    }

    #[test]
    fn runtime_endpoint_timeout_replacement_and_disablement_are_ordered() {
        let configured = RuntimeConfigBuilder::new()
            .runtime_endpoint_timeout(Duration::from_secs(11))
            .runtime_endpoint_timeout(Duration::from_secs(13))
            .resolve();
        assert_eq!(
            configured.runtime_endpoint_timeout,
            Some(Duration::from_secs(13))
        );

        let disabled = RuntimeConfigBuilder::new()
            .runtime_endpoint_timeout(Duration::from_secs(17))
            .disable_runtime_endpoint_timeout()
            .resolve();
        assert_eq!(disabled.runtime_endpoint_timeout, None);
    }

    #[test]
    fn hook_and_debug_setters_replace_immutable_builder_selection() {
        let first: Arc<dyn RuntimeHooks> = Arc::new(FirstHooks);
        let second: Arc<dyn RuntimeHooks> = Arc::new(SecondHooks);
        let config = RuntimeConfigBuilder::new()
            .hooks(Arc::clone(&first))
            .debug(DebugLevel::Basic)
            .hooks(Arc::clone(&second))
            .debug(DebugLevel::Headers)
            .resolve();

        assert!(!Arc::ptr_eq(&config.hooks, &first));
        assert!(Arc::ptr_eq(&config.hooks, &second));
        assert_eq!(config.debug, DebugLevel::Headers);
    }

    #[test]
    fn retry_policy_is_opt_in_replaceable_and_disableable() {
        let first = RetryPolicy::builder()
            .max_retries_per_operation(1)
            .without_budget()
            .classify_fn(|_| crate::retry::RetryDecision::Stop)
            .build()
            .expect("first retry policy");
        let second = RetryPolicy::builder()
            .max_retries_per_operation(3)
            .without_budget()
            .classify_fn(|_| crate::retry::RetryDecision::Stop)
            .build()
            .expect("second retry policy");

        let configured = RuntimeConfigBuilder::new()
            .retry_policy(first)
            .retry_policy(second)
            .resolve();
        assert_eq!(
            configured
                .retry_policy
                .as_ref()
                .expect("retry policy")
                .max_retries_per_operation(),
            3
        );

        let disabled = RuntimeConfigBuilder::new()
            .retry_policy(
                RetryPolicy::builder()
                    .without_budget()
                    .classify_fn(|_| crate::retry::RetryDecision::Stop)
                    .build()
                    .expect("retry policy"),
            )
            .disable_retry_policy()
            .resolve();
        assert!(disabled.retry_policy.is_none());
    }
}

use std::num::{NonZeroU32, NonZeroUsize};
use std::sync::Arc;
use std::time::Duration;

use crate::hooks::{DebugLevel, RuntimeHooks, default_hooks};

const MIB: usize = 1_048_576;
const KIB: usize = 1_024;

/// Private immutable finite runtime configuration.
#[allow(dead_code, reason = "assembled into the private Phase 2 ClientCore")]
pub(crate) struct RuntimeConfig {
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
    pub(crate) hooks: Arc<dyn RuntimeHooks>,
    pub(crate) debug: DebugLevel,
}

/// A consuming builder whose ordinary limits remain positive and finite.
pub struct RuntimeConfigBuilder {
    config: RuntimeConfig,
}

impl RuntimeConfigBuilder {
    #[allow(dead_code, reason = "initialized by later generated client builders")]
    pub(crate) fn new() -> Self {
        Self {
            config: RuntimeConfig {
                request_body_limit: nonzero_usize(16 * MIB),
                response_body_limit: nonzero_usize(16 * MIB),
                stream_response_limit: nonzero_usize(16 * MIB),
                status_body_limit: Some(nonzero_usize(64 * KIB)),
                provider_request_limit: nonzero_usize(MIB),
                provider_response_limit: nonzero_usize(MIB),
                pagination_max_pages: NonZeroU32::new(1_000)
                    .expect("the pagination page default is nonzero"),
                pagination_max_items: nonzero_usize(100_000),
                pagination_identity_limit: nonzero_usize(4 * MIB),
                auth_failure_cooldown_cap: Duration::from_secs(60),
                rate_limit_cooldown_cap: Duration::from_secs(60),
                hooks: default_hooks(),
                debug: DebugLevel::None,
            },
        }
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

    /// Sets the positive finite pagination identity-storage ceiling.
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

    #[allow(dead_code, reason = "assembled into the private Phase 2 ClientCore")]
    pub(crate) fn resolve(self) -> RuntimeConfig {
        self.config
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
        assert_eq!(config.request_body_limit.get(), 16 * MIB);
        assert_eq!(config.response_body_limit.get(), 16 * MIB);
        assert_eq!(config.stream_response_limit.get(), 16 * MIB);
        assert_eq!(
            config.status_body_limit.map(NonZeroUsize::get),
            Some(64 * KIB)
        );
        assert_eq!(config.provider_request_limit.get(), MIB);
        assert_eq!(config.provider_response_limit.get(), MIB);
        assert_eq!(config.pagination_max_pages.get(), 1_000);
        assert_eq!(config.pagination_max_items.get(), 100_000);
        assert_eq!(config.pagination_identity_limit.get(), 4 * MIB);
        assert_eq!(config.auth_failure_cooldown_cap, Duration::from_secs(60));
        assert_eq!(config.rate_limit_cooldown_cap, Duration::from_secs(60));
        assert_eq!(config.debug, DebugLevel::None);
    }

    #[test]
    fn setters_preserve_positive_finite_types_and_zero_duration_caps() {
        let one = NonZeroUsize::MIN;
        let one_page = NonZeroU32::MIN;
        let config = RuntimeConfigBuilder::new()
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
}

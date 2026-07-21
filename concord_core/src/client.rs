use std::sync::Arc;

use http::Method;
use url::Url;

use crate::error::{ApiError, ClientBuildError, ClientBuildErrorKind};
use crate::hooks::DebugDispatcher;
use crate::hooks::{DebugLevel, RuntimeHooks};
use crate::http::HttpConfigBuilder;
use crate::http::application::ApplicationHttp;
use crate::http::build::ManagedHttp;
use crate::http::provider::ProviderHttp;
use crate::operation::OperationDeadline;
use crate::operation::{OperationId, OperationIdExhausted};
use crate::runtime::{RuntimeConfig, RuntimeConfigBuilder};

/// Shared generated-private owner for one built client's immutable runtime.
#[doc(hidden)]
#[derive(Clone)]
pub struct ClientCore {
    inner: Arc<ClientInner>,
}

struct ClientInner {
    base_url: Url,
    safe_base_url: Arc<str>,
    http: ManagedHttp,
    runtime: RuntimeConfig,
    #[cfg(test)]
    debug_output: Option<Arc<std::sync::Mutex<Vec<String>>>>,
    #[cfg(test)]
    operation_allocations: std::sync::atomic::AtomicUsize,
}

impl ClientCore {
    pub(crate) fn build(
        fixed_base: &str,
        http: HttpConfigBuilder,
        runtime: RuntimeConfigBuilder,
    ) -> Result<Self, ClientBuildError> {
        let base_url = parse_fixed_base(fixed_base)?;
        let runtime = runtime.resolve();
        let http = http.resolve();
        ManagedHttp::validate_fixed_base(&http, &base_url)?;
        let http = ManagedHttp::build(http, &runtime)?;
        http.application.validate_fixed_base(&base_url)?;
        let safe_base_url = Arc::from(base_url.as_str());
        Ok(Self {
            inner: Arc::new(ClientInner {
                base_url,
                safe_base_url,
                http,
                runtime,
                #[cfg(test)]
                debug_output: None,
                #[cfg(test)]
                operation_allocations: std::sync::atomic::AtomicUsize::new(0),
            }),
        })
    }

    #[cfg(test)]
    pub(crate) fn build_with_development(
        fixed_base: &str,
        http: HttpConfigBuilder,
        runtime: RuntimeConfigBuilder,
        executor: &concord_test_support::executor::DeterministicExecutor,
    ) -> Result<Self, ClientBuildError> {
        let base_url = parse_fixed_base(fixed_base)?;
        let runtime = runtime.resolve();
        let http = http.resolve();
        ManagedHttp::validate_fixed_base(&http, &base_url)?;
        let http = ManagedHttp::build_with_development(http, &runtime, executor)?;
        http.application.validate_fixed_base(&base_url)?;
        let safe_base_url = Arc::from(base_url.as_str());
        Ok(Self {
            inner: Arc::new(ClientInner {
                base_url,
                safe_base_url,
                http,
                runtime,
                debug_output: None,
                operation_allocations: std::sync::atomic::AtomicUsize::new(0),
            }),
        })
    }

    pub(crate) fn base_url(&self) -> &Url {
        &self.inner.base_url
    }

    pub(crate) fn safe_base_url(&self) -> Arc<str> {
        Arc::clone(&self.inner.safe_base_url)
    }

    pub(crate) fn application_http(&self) -> &ApplicationHttp {
        &self.inner.http.application
    }

    pub(crate) fn provider_http(&self, deadline: Option<OperationDeadline>) -> ProviderHttp<'_> {
        self.inner.http.provider_http(deadline)
    }

    pub(crate) fn auth_failure_cooldown_cap(&self) -> std::time::Duration {
        self.inner.runtime.auth_failure_cooldown_cap
    }

    pub(crate) fn rate_limiter(&self) -> &Arc<dyn crate::rate_limit::RateLimiter> {
        &self.inner.runtime.rate_limiter
    }

    pub(crate) fn rate_limit_cooldown_cap(&self) -> std::time::Duration {
        self.inner.runtime.rate_limit_cooldown_cap
    }

    pub(crate) fn rate_limit_key_limit(&self) -> usize {
        self.inner.runtime.rate_limit_key_limit
    }

    pub(crate) fn hooks(&self) -> &Arc<dyn RuntimeHooks> {
        &self.inner.runtime.hooks
    }

    pub(crate) fn default_debug(&self) -> DebugLevel {
        self.inner.runtime.debug
    }

    pub(crate) fn debug_dispatcher(&self, level: DebugLevel) -> DebugDispatcher {
        let dispatcher = DebugDispatcher::new(level);
        #[cfg(test)]
        let dispatcher = match &self.inner.debug_output {
            Some(output) => dispatcher.with_output(Arc::clone(output)),
            None => dispatcher,
        };
        dispatcher
    }

    pub(crate) fn response_body_limit(&self) -> std::num::NonZeroUsize {
        self.inner.runtime.response_body_limit
    }

    pub(crate) fn request_body_limit(&self) -> std::num::NonZeroUsize {
        self.inner.runtime.request_body_limit
    }

    pub(crate) fn pagination_max_pages(&self) -> std::num::NonZeroU32 {
        self.inner.runtime.pagination_max_pages
    }

    pub(crate) fn pagination_max_items(&self) -> std::num::NonZeroUsize {
        self.inner.runtime.pagination_max_items
    }

    pub(crate) fn pagination_identity_limit(&self) -> std::num::NonZeroUsize {
        self.inner.runtime.pagination_identity_limit
    }

    pub(crate) fn stream_response_limit(&self) -> std::num::NonZeroUsize {
        self.inner.runtime.stream_response_limit
    }

    pub(crate) fn allocate_operation_id(
        &self,
        endpoint: &'static str,
        method: &Method,
    ) -> Result<OperationId, ApiError> {
        let operation_id = OperationId::allocate().map_err(|OperationIdExhausted| {
            ApiError::classified(
                endpoint,
                method.clone(),
                crate::error::ErrorCategory::Internal,
            )
        })?;
        #[cfg(test)]
        self.inner
            .operation_allocations
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(operation_id)
    }

    pub(crate) fn preflight_url(
        &self,
        endpoint: &'static str,
        method: &Method,
        url: &Url,
    ) -> Result<(), ApiError> {
        self.inner
            .http
            .application
            .preflight_url(endpoint, method, url)
    }

    pub(crate) fn preflight_protected_url(
        &self,
        endpoint: &'static str,
        method: &Method,
        url: &Url,
    ) -> Result<(), ApiError> {
        self.inner
            .http
            .application
            .preflight_protected_url(endpoint, method, url)
    }

    #[cfg(test)]
    pub(crate) fn operation_allocations(&self) -> usize {
        self.inner
            .operation_allocations
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    #[cfg(test)]
    pub(crate) fn shares_inner_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    #[cfg(test)]
    pub(crate) fn with_debug_output(mut self, output: Arc<std::sync::Mutex<Vec<String>>>) -> Self {
        Arc::get_mut(&mut self.inner)
            .expect("debug output is installed before ClientCore cloning")
            .debug_output = Some(output);
        self
    }
}

fn parse_fixed_base(value: &str) -> Result<Url, ClientBuildError> {
    let url = Url::parse(value)
        .map_err(|_| ClientBuildError::new(ClientBuildErrorKind::InvalidBaseUrl))?;
    let raw_authority = value
        .split_once("://")
        .map(|(_, remainder)| remainder.split(['/', '?', '#']).next().unwrap_or_default());
    let valid = matches!(url.scheme(), "http" | "https")
        && url.host().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && raw_authority.is_some_and(|authority| !authority.is_empty() && !authority.contains('@'));
    if !valid {
        return Err(ClientBuildError::new(ClientBuildErrorKind::InvalidBaseUrl));
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::{NonZeroU32, NonZeroUsize};
    use std::time::Duration;

    use crate::advanced::NoopRateLimiter;
    use crate::http::SafeProxy;

    struct FirstHooks;
    impl RuntimeHooks for FirstHooks {}

    struct SecondHooks;
    impl RuntimeHooks for SecondHooks {}

    #[test]
    fn fixed_base_validation_is_structural_and_safe() {
        for invalid in [
            "not a URL",
            "ftp://example.test",
            "https:///missing-host",
            "https://user:password@example.test",
            "https://example.test?query=value",
            "https://example.test#fragment",
        ] {
            let error = match parse_fixed_base(invalid) {
                Ok(_) => panic!("invalid fixed base unexpectedly parsed: {invalid}"),
                Err(error) => error,
            };
            assert_eq!(error.kind(), ClientBuildErrorKind::InvalidBaseUrl);
            assert!(!error.to_string().contains(invalid));
            assert!(!format!("{error:?}").contains(invalid));
        }
    }

    #[test]
    fn client_core_freezes_complete_runtime_selection_at_construction() {
        let first_executor = concord_test_support::executor::DeterministicExecutor::new();
        let second_executor = concord_test_support::executor::DeterministicExecutor::new();
        let first_hooks: Arc<dyn RuntimeHooks> = Arc::new(FirstHooks);
        let second_hooks: Arc<dyn RuntimeHooks> = Arc::new(SecondHooks);
        let first_limiter: Arc<dyn crate::rate_limit::RateLimiter> =
            Arc::new(NoopRateLimiter::new());
        let second_limiter: Arc<dyn crate::rate_limit::RateLimiter> =
            Arc::new(NoopRateLimiter::new());
        let one = NonZeroUsize::new(11).expect("positive test limit");
        let two = NonZeroUsize::new(13).expect("positive test limit");
        let three = NonZeroUsize::new(17).expect("positive test limit");
        let four = NonZeroUsize::new(19).expect("positive test limit");
        let five = NonZeroUsize::new(23).expect("positive test limit");
        let six = NonZeroUsize::new(29).expect("positive test limit");
        let seven = NonZeroUsize::new(31).expect("positive test limit");
        let first = ClientCore::build_with_development(
            "https://first.example",
            HttpConfigBuilder::new()
                .https_only(true)
                .http1_only()
                .proxy(SafeProxy::parse("https://first-proxy.example").expect("safe proxy")),
            RuntimeConfigBuilder::new()
                .request_body_limit(one)
                .response_body_limit(two)
                .stream_response_limit(three)
                .status_body_limit(four)
                .provider_request_limit(five)
                .provider_response_limit(six)
                .pagination_max_pages(NonZeroU32::new(37).expect("positive page limit"))
                .pagination_max_items(seven)
                .pagination_identity_limit(NonZeroUsize::new(41).expect("positive identity limit"))
                .auth_failure_cooldown_cap(Duration::from_secs(43))
                .rate_limit_cooldown_cap(Duration::from_secs(47))
                .hooks(Arc::clone(&first_hooks))
                .rate_limiter(Arc::clone(&first_limiter))
                .debug(DebugLevel::Headers),
            &first_executor,
        )
        .expect("first client builds");

        assert_eq!(
            first.inner.http.test_runtime_selection(),
            (
                true,
                cfg!(feature = "default-tls"),
                Some(four),
                Duration::from_secs(47),
                five,
                six,
            )
        );
        assert_eq!(first.inner.runtime.request_body_limit, one);
        assert_eq!(first.inner.runtime.response_body_limit, two);
        assert_eq!(first.inner.runtime.stream_response_limit, three);
        assert_eq!(first.inner.runtime.status_body_limit, Some(four));
        assert_eq!(first.inner.runtime.provider_request_limit, five);
        assert_eq!(first.inner.runtime.provider_response_limit, six);
        assert_eq!(first.inner.runtime.pagination_max_pages.get(), 37);
        assert_eq!(first.inner.runtime.pagination_max_items, seven);
        assert_eq!(first.inner.runtime.pagination_identity_limit.get(), 41);
        assert_eq!(
            first.inner.runtime.auth_failure_cooldown_cap,
            Duration::from_secs(43)
        );
        assert_eq!(
            first.inner.runtime.rate_limit_cooldown_cap,
            Duration::from_secs(47)
        );
        assert!(Arc::ptr_eq(&first.inner.runtime.hooks, &first_hooks));
        assert!(Arc::ptr_eq(
            &first.inner.runtime.rate_limiter,
            &first_limiter
        ));
        assert_eq!(first.inner.runtime.debug, DebugLevel::Headers);
        assert!(
            first
                .preflight_url(
                    "snapshot",
                    &Method::GET,
                    &Url::parse("http://first.example").expect("URL"),
                )
                .is_err()
        );

        let clone = first.clone();
        assert!(first.shares_inner_with(&clone));
        assert!(Arc::ptr_eq(&clone.inner.runtime.hooks, &first_hooks));
        assert!(Arc::ptr_eq(
            &clone.inner.runtime.rate_limiter,
            &first_limiter
        ));
        assert_eq!(clone.inner.runtime.debug, DebugLevel::Headers);

        let second = ClientCore::build_with_development(
            "http://second.example",
            HttpConfigBuilder::new(),
            RuntimeConfigBuilder::new()
                .request_body_limit(NonZeroUsize::new(53).expect("positive test limit"))
                .hooks(Arc::clone(&second_hooks))
                .rate_limiter(Arc::clone(&second_limiter))
                .debug(DebugLevel::Basic),
            &second_executor,
        )
        .expect("second client builds");
        assert!(!second.shares_inner_with(&first));
        assert!(!Arc::ptr_eq(&second.inner.runtime.hooks, &first_hooks));
        assert!(!Arc::ptr_eq(
            &second.inner.runtime.rate_limiter,
            &first_limiter
        ));
        assert_eq!(second.inner.runtime.debug, DebugLevel::Basic);
        assert!(
            second
                .preflight_url(
                    "snapshot",
                    &Method::GET,
                    &Url::parse("http://second.example").expect("URL"),
                )
                .is_ok()
        );

        assert_eq!(first.inner.runtime.request_body_limit, one);
        assert_eq!(first.inner.runtime.pagination_max_pages.get(), 37);
        assert_eq!(
            first.inner.runtime.rate_limit_cooldown_cap,
            Duration::from_secs(47)
        );
        assert!(Arc::ptr_eq(&first.inner.runtime.hooks, &first_hooks));
        assert!(Arc::ptr_eq(
            &first.inner.runtime.rate_limiter,
            &first_limiter
        ));
    }
}

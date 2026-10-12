use std::sync::Arc;

use http::Method;
use url::Url;

use crate::endpoint::EndpointId;
use crate::error::{ApiError, ClientBuildError, ClientBuildErrorKind};
use crate::hooks::DebugDispatcher;
use crate::hooks::{DebugLevel, OperationObserver};
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
    credential_context: crate::auth::CredentialContext,
    base_url: Url,
    http: ManagedHttp,
    runtime: RuntimeConfig,
    #[cfg(test)]
    debug_output: Option<Arc<std::sync::Mutex<Vec<String>>>>,
    #[cfg(test)]
    operation_allocations: std::sync::atomic::AtomicUsize,
}

impl ClientCore {
    /// Collects independent safe input/configuration failures before construction.
    /// No native client, credential slot, callback, provider acquisition,
    /// operation identity or remote schema registration is created here.
    pub(crate) fn preflight_build(
        fixed_base: &str,
        http: &HttpConfigBuilder,
        runtime: &RuntimeConfigBuilder,
        requires_limiter: bool,
        missing: Vec<(&'static str, crate::error::ClientInputKind)>,
    ) -> Result<(), ClientBuildError> {
        let mut errors = Vec::new();
        if !missing.is_empty() {
            errors.push(ClientBuildError::missing_inputs(missing));
        }
        // Preserve the earlier primary validation ordering.
        for result in [
            runtime.validate_rate_limit_key_limit(),
            runtime.validate_generated_rate_limits(requires_limiter),
            runtime.validate_provider_configuration(),
            runtime.validate_stream_configuration(),
            runtime.validate_limit_configuration(),
        ] {
            if let Err(error) = result {
                errors.push(error);
            }
        }
        let base = match parse_fixed_base(fixed_base) {
            Ok(base) => Some(base),
            Err(error) => {
                errors.push(error);
                None
            }
        };
        if let Err(error) = http.validate_startup_configuration() {
            errors.push(error);
        }
        if let Some(base) = base {
            if let Err(error) = http.validate_fixed_base(&base) {
                errors.push(error);
            }
        }
        if let Err(error) = http.validate_material_configuration() {
            errors.push(error);
        }
        match ClientBuildError::collect(errors) {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    pub(crate) fn build(
        fixed_base: &str,
        http: HttpConfigBuilder,
        runtime: RuntimeConfigBuilder,
    ) -> Result<Self, ClientBuildError> {
        Self::preflight_build(fixed_base, &http, &runtime, false, Vec::new())?;
        let base_url = parse_fixed_base(fixed_base)?;
        let runtime = runtime.resolve();
        let http = http.resolve()?;
        ManagedHttp::validate_fixed_base(&http, &base_url)?;
        let http = ManagedHttp::build(http, &runtime)?;
        http.application.validate_fixed_base(&base_url)?;
        Ok(Self {
            inner: Arc::new(ClientInner {
                credential_context: crate::auth::CredentialContext::new(),
                base_url,
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
        Self::preflight_build(fixed_base, &http, &runtime, false, Vec::new())?;
        let base_url = parse_fixed_base(fixed_base)?;
        let runtime = runtime.resolve();
        let http = http.resolve()?;
        ManagedHttp::validate_fixed_base(&http, &base_url)?;
        let http = ManagedHttp::build_with_development(http, &runtime, executor)?;
        http.application.validate_fixed_base(&base_url)?;
        Ok(Self {
            inner: Arc::new(ClientInner {
                credential_context: crate::auth::CredentialContext::new(),
                base_url,
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

    pub(crate) fn credential_context(&self) -> &crate::auth::CredentialContext {
        &self.inner.credential_context
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

    pub(crate) fn provider_max_origins(&self) -> usize {
        self.inner.runtime.provider_max_origins
    }

    pub(crate) fn rate_limiter(&self) -> &Arc<dyn crate::rate_limit::RateLimiter> {
        &self.inner.runtime.rate_limiter
    }

    /// Whether the application explicitly selected a limiter (a real limiter
    /// or the explicit unenforced `NoopRateLimiter`) at client build. Without
    /// a selection the fail-closed fallback rejects every nonempty plan, so a
    /// runtime binding that adds policies must fail at bind (V3-R11).
    pub(crate) fn rate_limiter_selected(&self) -> bool {
        self.inner.runtime.rate_limiter_selected
    }

    /// Returns the limiter capability snapshotted at client build.
    pub(crate) fn rate_limiter_capability(&self) -> crate::rate_limit::LimiterCapability {
        self.inner.runtime.rate_limiter_capability
    }

    /// Returns the safe structural rate facts of one prepared plan together
    /// with the actual enforcement choice. Performs no limiter call.
    pub(crate) fn rate_plan_summary(
        &self,
        plan: &crate::rate_limit::RateLimitPlan,
    ) -> crate::rate_limit::RatePlanSummary {
        crate::rate_limit::RatePlanSummary::new(
            plan,
            self.inner
                .runtime
                .rate_limiter_selected
                .then_some(self.inner.runtime.rate_limiter_capability),
        )
    }

    pub(crate) fn rate_limit_cooldown_cap(&self) -> std::time::Duration {
        self.inner.runtime.rate_limit_cooldown_cap
    }

    pub(crate) fn rate_limit_key_limit(&self) -> usize {
        self.inner.runtime.rate_limit_key_limit
    }

    pub(crate) fn retry_policy(&self) -> Option<&crate::retry::RetryPolicy> {
        self.inner.runtime.retry_policy.as_ref()
    }

    pub(crate) fn operation_observer(&self) -> &Arc<dyn OperationObserver> {
        &self.inner.runtime.operation_observer
    }

    pub(crate) fn response_wall_time(&self) -> std::time::SystemTime {
        self.inner.runtime.wall_clock.now()
    }

    pub(crate) fn default_debug(&self) -> DebugLevel {
        self.inner.runtime.debug
    }

    pub(crate) fn default_runtime_endpoint_timeout(&self) -> Option<std::time::Duration> {
        self.inner.runtime.runtime_endpoint_timeout
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

    pub(crate) fn pagination_total_response_body_limit(&self) -> std::num::NonZeroUsize {
        self.inner.runtime.pagination_total_response_body_limit
    }

    pub(crate) fn pagination_identity_limit(&self) -> std::num::NonZeroUsize {
        self.inner.runtime.pagination_identity_limit
    }

    /// The trusted aggregate application-handoff ceiling (§20.4
    /// `aggregate_max_sends`), before per-call narrowing.
    pub(crate) fn aggregate_max_sends(&self) -> std::num::NonZeroU32 {
        self.inner.runtime.aggregate_max_sends
    }

    /// Returns the trusted runtime ceilings of a managed stream, before any
    /// per-call narrowing.
    pub(crate) fn stream_limits(&self) -> crate::response::StreamLimits {
        let runtime = &self.inner.runtime;
        crate::response::StreamLimits {
            lifetime: runtime.stream_lifetime,
            idle: runtime.stream_idle_timeout,
            bytes: runtime.stream_response_limit,
            records: runtime.stream_max_records,
            frame: runtime.stream_frame_limit,
        }
    }

    pub(crate) fn allocate_operation_id(
        &self,
        endpoint: &EndpointId,
        method: &Method,
    ) -> Result<OperationId, ApiError> {
        let operation_id = OperationId::allocate().map_err(|OperationIdExhausted| {
            ApiError::classified(
                endpoint.clone(),
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
        endpoint: &EndpointId,
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
        endpoint: &EndpointId,
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
    let mut url = Url::parse(value)
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
    // ORIGIN-01: a missing terminal slash is normalized as a directory
    // prefix, so `/root` and `/root/` own the same fixed base path, with or
    // without generated segments, exactly as the static explanation reports.
    if !url.path().ends_with('/') {
        let path = format!("{}/", url.path());
        url.set_path(&path);
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroUsize;
    use std::time::Duration;

    use crate::contracts::NoopRateLimiter;
    use crate::http::SafeProxy;

    struct FirstObserver;
    impl OperationObserver for FirstObserver {}

    struct SecondObserver;
    impl OperationObserver for SecondObserver {}

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
            assert!(std::error::Error::source(&error).is_none());
        }
    }

    /// ORIGIN-01 regression: the runtime fixed base path uses the same
    /// directory-prefix normalization as the static explanation, so a request
    /// with zero generated segments targets `/root/`, not `/root`.
    #[test]
    fn fixed_base_path_is_normalized_as_a_directory_prefix() {
        for (literal, expected) in [
            ("https://example.test/root", "https://example.test/root/"),
            ("https://example.test/root/", "https://example.test/root/"),
            ("https://example.test/a/b", "https://example.test/a/b/"),
            ("https://example.test", "https://example.test/"),
            ("https://example.test/", "https://example.test/"),
        ] {
            let url = parse_fixed_base(literal).expect("valid fixed base");
            assert_eq!(url.as_str(), expected, "{literal}");
        }
    }

    #[test]
    fn rate_limiter_rejects_preparation_limit_above_hard_cap() {
        struct ExcessiveKeyLimit;

        impl crate::rate_limit::RateLimiter for ExcessiveKeyLimit {
            fn capability(&self) -> crate::rate_limit::LimiterCapability {
                crate::rate_limit::LimiterCapability::new(
                    crate::rate_limit::Enforcement::Local,
                    "test.excessive-key-limit",
                    1,
                )
            }

            fn preparation_key_limit(&self) -> usize {
                crate::rate_limit::MAX_RATE_LIMIT_KEY_BYTES + 1
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

        let executor = concord_test_support::executor::DeterministicExecutor::new();
        let error = ClientCore::build_with_development(
            "http://runtime-limit.example",
            HttpConfigBuilder::new(),
            RuntimeConfigBuilder::new().rate_limiter(Arc::new(ExcessiveKeyLimit)),
            &executor,
        )
        .err()
        .expect("an excessive limiter preparation bound must fail client construction");

        assert_eq!(error.kind(), ClientBuildErrorKind::RuntimeConfiguration);
    }

    #[test]
    fn client_core_freezes_complete_runtime_selection_at_construction() {
        #[cfg(not(feature = "default-tls"))]
        let _ = rustls::crypto::ring::default_provider().install_default();
        let first_executor = concord_test_support::executor::DeterministicExecutor::new();
        let second_executor = concord_test_support::executor::DeterministicExecutor::new();
        let first_observer: Arc<dyn OperationObserver> = Arc::new(FirstObserver);
        let second_observer: Arc<dyn OperationObserver> = Arc::new(SecondObserver);
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
        let eight = NonZeroUsize::new(37).expect("positive test limit");
        let first_https_only = cfg!(feature = "default-tls");
        let first_scheme = if first_https_only { "https" } else { "http" };
        let first_base = format!("{first_scheme}://first.example");
        let first_proxy = format!("{first_scheme}://first-proxy.example");
        let first_http = if first_https_only {
            HttpConfigBuilder::new().require_https()
        } else {
            HttpConfigBuilder::new().allow_http()
        };
        let first = ClientCore::build_with_development(
            &first_base,
            first_http
                .http1_only()
                .proxy(SafeProxy::parse(first_proxy).expect("safe proxy")),
            RuntimeConfigBuilder::new()
                .runtime_endpoint_timeout(Duration::from_secs(7))
                .request_body_limit(one.get())
                .response_body_limit(two.get())
                .stream_response_limit(three.get())
                .status_body_limit(four.get())
                .provider_request_limit(five.get())
                .provider_response_limit(six.get())
                .pagination_max_pages(37)
                .pagination_max_items(seven.get())
                .pagination_total_response_body_limit(eight.get())
                .pagination_identity_limit(41)
                .auth_failure_cooldown_cap(Duration::from_secs(43))
                .rate_limit_cooldown_cap(Duration::from_secs(47))
                .operation_observer(Arc::clone(&first_observer))
                .rate_limiter(Arc::clone(&first_limiter))
                .debug(DebugLevel::Headers),
            &first_executor,
        )
        .expect("first client builds");

        assert_eq!(
            first.inner.http.test_runtime_selection(),
            (
                first_https_only,
                cfg!(feature = "default-tls"),
                Some(four),
                Duration::from_secs(47),
                five,
                six,
            )
        );
        assert_eq!(first.inner.runtime.request_body_limit, one);
        assert_eq!(
            first.inner.runtime.runtime_endpoint_timeout,
            Some(Duration::from_secs(7))
        );
        assert_eq!(first.inner.runtime.response_body_limit, two);
        assert_eq!(first.inner.runtime.stream_response_limit, three);
        assert_eq!(first.inner.runtime.status_body_limit, Some(four));
        assert_eq!(first.inner.runtime.provider_request_limit, five);
        assert_eq!(first.inner.runtime.provider_response_limit, six);
        assert_eq!(first.inner.runtime.pagination_max_pages.get(), 37);
        assert_eq!(first.inner.runtime.pagination_max_items, seven);
        assert_eq!(
            first.inner.runtime.pagination_total_response_body_limit,
            eight
        );
        assert_eq!(first.inner.runtime.pagination_identity_limit.get(), 41);
        assert_eq!(
            first.inner.runtime.auth_failure_cooldown_cap,
            Duration::from_secs(43)
        );
        assert_eq!(
            first.inner.runtime.rate_limit_cooldown_cap,
            Duration::from_secs(47)
        );
        assert!(Arc::ptr_eq(
            &first.inner.runtime.operation_observer,
            &first_observer
        ));
        assert!(Arc::ptr_eq(
            &first.inner.runtime.rate_limiter,
            &first_limiter
        ));
        assert_eq!(first.inner.runtime.debug, DebugLevel::Headers);
        assert_eq!(
            first
                .preflight_url(
                    &EndpointId::from_static("snapshot"),
                    &Method::GET,
                    &Url::parse("http://first.example").expect("URL"),
                )
                .is_err(),
            first_https_only,
        );

        let clone = first.clone();
        assert!(first.shares_inner_with(&clone));
        assert!(Arc::ptr_eq(
            &clone.inner.runtime.operation_observer,
            &first_observer
        ));
        assert!(Arc::ptr_eq(
            &clone.inner.runtime.rate_limiter,
            &first_limiter
        ));
        assert_eq!(clone.inner.runtime.debug, DebugLevel::Headers);
        assert_eq!(
            clone.inner.runtime.runtime_endpoint_timeout,
            Some(Duration::from_secs(7))
        );
        assert_eq!(
            clone.inner.runtime.pagination_total_response_body_limit,
            eight
        );

        let second = ClientCore::build_with_development(
            "http://second.example",
            HttpConfigBuilder::new(),
            RuntimeConfigBuilder::new()
                .request_body_limit(53)
                .operation_observer(Arc::clone(&second_observer))
                .rate_limiter(Arc::clone(&second_limiter))
                .debug(DebugLevel::Basic),
            &second_executor,
        )
        .expect("second client builds");
        assert!(!second.shares_inner_with(&first));
        assert!(!Arc::ptr_eq(
            &second.inner.runtime.operation_observer,
            &first_observer
        ));
        assert!(!Arc::ptr_eq(
            &second.inner.runtime.rate_limiter,
            &first_limiter
        ));
        assert_eq!(second.inner.runtime.debug, DebugLevel::Basic);
        assert!(
            second
                .preflight_url(
                    &EndpointId::from_static("snapshot"),
                    &Method::GET,
                    &Url::parse("http://second.example").expect("URL"),
                )
                .is_ok()
        );

        assert_eq!(first.inner.runtime.request_body_limit, one);
        assert_eq!(
            first.inner.runtime.runtime_endpoint_timeout,
            Some(Duration::from_secs(7))
        );
        assert_eq!(first.inner.runtime.pagination_max_pages.get(), 37);
        assert_eq!(
            first.inner.runtime.pagination_total_response_body_limit,
            eight
        );
        assert_eq!(
            first.inner.runtime.rate_limit_cooldown_cap,
            Duration::from_secs(47)
        );
        assert!(Arc::ptr_eq(
            &first.inner.runtime.operation_observer,
            &first_observer
        ));
        assert!(Arc::ptr_eq(
            &first.inner.runtime.rate_limiter,
            &first_limiter
        ));
    }
}

#[cfg(all(test, feature = "default-tls"))]
mod review_primary_precedence {
    use super::*;
    use std::time::Duration;

    fn checked(
        base: &str,
        http: HttpConfigBuilder,
        runtime: RuntimeConfigBuilder,
        expected: &[ClientBuildErrorKind],
    ) -> ClientBuildError {
        let error = ClientCore::preflight_build(base, &http, &runtime, false, Vec::new())
            .expect_err("expected independent preflight diagnostics");
        assert_eq!(error.kind(), expected[0]);
        assert_eq!(
            error.diagnostics().map(|d| d.kind()).collect::<Vec<_>>(),
            expected
        );
        assert!(std::error::Error::source(&error).is_none());
        error
    }

    #[test]
    fn mixed_failures_preserve_runtime_base_startup_transport_material_order() {
        use ClientBuildErrorKind::*;
        let oversized_root = vec![b'r'; crate::http::MAX_ROOT_CERTIFICATE_BYTES + 1];
        let oversized_identity = vec![b'i'; crate::http::MAX_IDENTITY_PEM_BYTES + 1];

        // Structural startup wins over the later transport restriction and
        // native setting, while each independent diagnostic remains once.
        checked(
            "http://review.invalid/",
            HttpConfigBuilder::new()
                .require_https()
                .add_root_certificate_pem(&oversized_root)
                .connect_timeout(Duration::MAX),
            RuntimeConfigBuilder::new(),
            &[InvalidRootCertificate, HttpsRequired, HttpConfiguration],
        );

        // The first structural setter keeps its primary identity, with the
        // other rejected material domain reported once before transport.
        checked(
            "http://review.invalid/",
            HttpConfigBuilder::new()
                .require_https()
                .identity_pem(&oversized_identity)
                .add_root_certificate_pem(&oversized_root),
            RuntimeConfigBuilder::new(),
            &[InvalidClientIdentity, InvalidRootCertificate, HttpsRequired],
        );

        let error = checked(
            "invalid-base",
            HttpConfigBuilder::new()
                .require_https()
                .add_root_certificate_pem(&oversized_root)
                .connect_timeout(Duration::MAX),
            RuntimeConfigBuilder::new().request_body_limit(0),
            &[
                RuntimeConfiguration,
                InvalidBaseUrl,
                InvalidRootCertificate,
                HttpConfiguration,
            ],
        );
        assert_eq!(error.input_field(), Some("request_body_limit"));

        // Retained, structurally bounded malformed PEM is validated after
        // transport restriction, as it was in the original construction path.
        checked(
            "http://review.invalid/",
            HttpConfigBuilder::new()
                .require_https()
                .add_root_certificate_pem(b"private-root-sentinel")
                .connect_timeout(Duration::MAX),
            RuntimeConfigBuilder::new(),
            &[HttpsRequired, HttpConfiguration, InvalidRootCertificate],
        );
    }

    #[test]
    fn individual_failures_and_valid_zero_controls_discriminate_the_mixed_oracle() {
        use ClientBuildErrorKind::*;
        let oversized_root = vec![b'r'; crate::http::MAX_ROOT_CERTIFICATE_BYTES + 1];
        checked(
            "http://review.invalid/",
            HttpConfigBuilder::new().require_https(),
            RuntimeConfigBuilder::new(),
            &[HttpsRequired],
        );
        checked(
            "https://review.invalid/",
            HttpConfigBuilder::new().add_root_certificate_pem(&oversized_root),
            RuntimeConfigBuilder::new(),
            &[InvalidRootCertificate],
        );
        checked(
            "invalid-base",
            HttpConfigBuilder::new(),
            RuntimeConfigBuilder::new(),
            &[InvalidBaseUrl],
        );
        checked(
            "https://review.invalid/",
            HttpConfigBuilder::new().connect_timeout(Duration::MAX),
            RuntimeConfigBuilder::new(),
            &[HttpConfiguration],
        );
        checked(
            "https://review.invalid/",
            HttpConfigBuilder::new().add_root_certificate_pem(b"private-root-sentinel"),
            RuntimeConfigBuilder::new(),
            &[InvalidRootCertificate],
        );
        ClientCore::preflight_build(
            "https://review.invalid/",
            &HttpConfigBuilder::new()
                .require_https()
                .connect_timeout(Duration::ZERO),
            &RuntimeConfigBuilder::new(),
            false,
            Vec::new(),
        )
        .expect("valid HTTPS and native-zero positive control");
        ClientCore::preflight_build(
            "http://review.invalid/",
            &HttpConfigBuilder::new(),
            &RuntimeConfigBuilder::new(),
            false,
            Vec::new(),
        )
        .expect("valid unrestricted HTTP positive control");
    }
}

#[cfg(all(test, feature = "default-tls"))]
mod review_limiter_snapshot_diagnostic {
    use super::*;
    use crate::rate_limit::{
        Enforcement, LimiterCapability, MAX_RATE_LIMIT_KEY_BYTES, RateLimitError, RateLimitPermit,
        RateLimitRequest, RateLimiter,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct SnapshotLimiter {
        key_limit: usize,
        descriptor: LimiterCapability,
        key_calls: AtomicUsize,
        descriptor_calls: AtomicUsize,
        acquisitions: AtomicUsize,
    }
    impl RateLimiter for SnapshotLimiter {
        fn capability(&self) -> LimiterCapability {
            self.descriptor_calls.fetch_add(1, Ordering::SeqCst);
            self.descriptor
        }
        fn preparation_key_limit(&self) -> usize {
            self.key_calls.fetch_add(1, Ordering::SeqCst);
            self.key_limit
        }
        fn acquire<'a>(
            &'a self,
            _: RateLimitRequest<'a>,
        ) -> crate::hooks::BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
            self.acquisitions.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { panic!("preflight must not acquire") })
        }
    }

    #[test]
    fn selected_limiter_is_one_snapshot_diagnostic_but_independent_setting_is_retained() {
        for (label, bad_key, bad_descriptor, bad_body) in [
            ("valid", false, false, false),
            ("key-only", true, false, false),
            ("descriptor-only", false, true, false),
            ("both-members", true, true, false),
            ("both-plus-independent-setting", true, true, true),
        ] {
            let limiter = Arc::new(SnapshotLimiter {
                key_limit: if bad_key {
                    MAX_RATE_LIMIT_KEY_BYTES + 1
                } else {
                    1024
                },
                descriptor: if bad_descriptor {
                    LimiterCapability::new(Enforcement::Local, "", 0)
                } else {
                    LimiterCapability::new(Enforcement::Local, "review.snapshot", 1)
                },
                key_calls: AtomicUsize::new(0),
                descriptor_calls: AtomicUsize::new(0),
                acquisitions: AtomicUsize::new(0),
            });
            let mut runtime = RuntimeConfigBuilder::new().rate_limiter(limiter.clone());
            if bad_body {
                runtime = runtime.request_body_limit(0);
            }
            assert_eq!(limiter.key_calls.load(Ordering::SeqCst), 1, "{label}");
            assert_eq!(
                limiter.descriptor_calls.load(Ordering::SeqCst),
                1,
                "{label}"
            );
            let result = ClientCore::preflight_build(
                "https://review.invalid/",
                &HttpConfigBuilder::new(),
                &runtime,
                true,
                Vec::new(),
            );
            if bad_key || bad_descriptor {
                let error = result.expect_err(label);
                assert_eq!(error.kind(), ClientBuildErrorKind::RuntimeConfiguration);
                assert_eq!(error.input_field(), None);
                assert_eq!(error.input_kind(), None);
                let diagnostics = error.diagnostics().collect::<Vec<_>>();
                assert_eq!(
                    diagnostics.len(),
                    if bad_body { 2 } else { 1 },
                    "{label}: {error:?}"
                );
                assert_eq!(diagnostics[0].input_field(), None);
                if bad_body {
                    assert_eq!(diagnostics[1].input_field(), Some("request_body_limit"));
                }
                assert!(std::error::Error::source(&error).is_none());
            } else {
                result.expect("valid selected limiter metadata");
            }
            assert_eq!(
                limiter.key_calls.load(Ordering::SeqCst),
                1,
                "preflight {label}"
            );
            assert_eq!(
                limiter.descriptor_calls.load(Ordering::SeqCst),
                1,
                "preflight {label}"
            );
            assert_eq!(
                limiter.acquisitions.load(Ordering::SeqCst),
                0,
                "preflight {label}"
            );
        }
    }
}

use std::time::Duration;

use super::SafeProxy;

/// Reviewed provider native-retry modes.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderRetry {
    /// Permit Reqwest's protocol-recovery behavior.
    ProtocolRecovery,
    /// Disable provider native retries.
    Disabled,
}

#[allow(dead_code, reason = "consumed by managed Reqwest construction")]
pub(crate) enum ApplicationRetry {
    Disabled,
    ReqwestDefault,
    #[cfg(feature = "dangerous-custom-retry")]
    Dangerous(crate::dangerous::ReqwestRetryBuilder),
    #[cfg(feature = "dangerous-custom-retry")]
    DangerousFactory(fn() -> crate::dangerous::ReqwestRetryBuilder),
}

#[allow(dead_code, reason = "consumed by managed Reqwest construction")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProtocolMode {
    Negotiated,
    Http1Only,
    #[cfg(feature = "http2")]
    Http2PriorKnowledge,
}

/// Plain resolved HTTP configuration. It contains no Reqwest client or
/// `reqwest::ClientBuilder`; the dangerous feature may retain only its exact
/// reviewed native retry-builder value.
#[allow(dead_code, reason = "consumed by managed Reqwest construction")]
pub(crate) struct HttpConfig {
    pub(crate) connect_timeout: Option<Duration>,
    pub(crate) read_timeout: Option<Duration>,
    pub(crate) pool_idle_timeout: Option<Duration>,
    pub(crate) pool_max_idle_per_host: Option<usize>,
    pub(crate) tcp_keepalive: Option<Duration>,
    pub(crate) tcp_nodelay: Option<bool>,
    pub(crate) https_only: bool,
    pub(crate) protocol: ProtocolMode,
    pub(crate) proxy: Option<SafeProxy>,
    pub(crate) application_retry: ApplicationRetry,
    pub(crate) provider_retry: ProviderRetry,
    #[cfg(feature = "default-tls")]
    pub(crate) root_certificates_pem: Vec<Vec<u8>>,
    #[cfg(feature = "default-tls")]
    pub(crate) identity_pem: Option<Vec<u8>>,
    #[cfg(feature = "gzip")]
    pub(crate) disable_gzip: bool,
    #[cfg(feature = "brotli")]
    pub(crate) disable_brotli: bool,
    #[cfg(feature = "deflate")]
    pub(crate) disable_deflate: bool,
}

/// A consuming builder for reviewed plain HTTP configuration.
pub struct HttpConfigBuilder {
    config: HttpConfig,
}

impl HttpConfigBuilder {
    #[allow(dead_code, reason = "initialized by later generated client builders")]
    pub(crate) fn new() -> Self {
        Self {
            config: HttpConfig {
                connect_timeout: None,
                read_timeout: None,
                pool_idle_timeout: None,
                pool_max_idle_per_host: None,
                tcp_keepalive: None,
                tcp_nodelay: None,
                https_only: false,
                protocol: ProtocolMode::Negotiated,
                proxy: None,
                application_retry: ApplicationRetry::Disabled,
                provider_retry: ProviderRetry::ProtocolRecovery,
                #[cfg(feature = "default-tls")]
                root_certificates_pem: Vec::new(),
                #[cfg(feature = "default-tls")]
                identity_pem: None,
                #[cfg(feature = "gzip")]
                disable_gzip: false,
                #[cfg(feature = "brotli")]
                disable_brotli: false,
                #[cfg(feature = "deflate")]
                disable_deflate: false,
            },
        }
    }

    /// Sets the native connection timeout.
    #[must_use]
    pub fn connect_timeout(mut self, duration: Duration) -> Self {
        self.config.connect_timeout = Some(duration);
        self
    }

    /// Sets the native per-read timeout.
    #[must_use]
    pub fn read_timeout(mut self, duration: Duration) -> Self {
        self.config.read_timeout = Some(duration);
        self
    }

    /// Sets the idle pooled-connection timeout.
    #[must_use]
    pub fn pool_idle_timeout(mut self, duration: Duration) -> Self {
        self.config.pool_idle_timeout = Some(duration);
        self
    }

    /// Sets the maximum idle pooled connections per host.
    #[must_use]
    pub fn pool_max_idle_per_host(mut self, count: usize) -> Self {
        self.config.pool_max_idle_per_host = Some(count);
        self
    }

    /// Sets native TCP keepalive.
    #[must_use]
    pub fn tcp_keepalive(mut self, duration: Duration) -> Self {
        self.config.tcp_keepalive = Some(duration);
        self
    }

    /// Sets native TCP no-delay behavior.
    #[must_use]
    pub fn tcp_nodelay(mut self, enabled: bool) -> Self {
        self.config.tcp_nodelay = Some(enabled);
        self
    }

    /// Restricts the managed application client to HTTPS when enabled.
    #[must_use]
    pub fn https_only(mut self, enabled: bool) -> Self {
        self.config.https_only = enabled;
        self
    }

    /// Restricts the managed clients to HTTP/1.
    #[must_use]
    pub fn http1_only(mut self) -> Self {
        self.config.protocol = ProtocolMode::Http1Only;
        self
    }

    /// Selects HTTP/2 prior knowledge.
    #[cfg(feature = "http2")]
    #[must_use]
    pub fn http2_prior_knowledge(mut self) -> Self {
        self.config.protocol = ProtocolMode::Http2PriorKnowledge;
        self
    }

    /// Installs one explicitly validated proxy origin.
    #[must_use]
    pub fn proxy(mut self, proxy: SafeProxy) -> Self {
        self.config.proxy = Some(proxy);
        self
    }

    /// Enables Reqwest's native default retry policy for application traffic.
    #[must_use]
    pub fn retry_reqwest_default(mut self) -> Self {
        self.config.application_retry = ApplicationRetry::ReqwestDefault;
        self
    }

    /// Disables native retry for application traffic.
    #[must_use]
    pub fn retry_off(mut self) -> Self {
        self.config.application_retry = ApplicationRetry::Disabled;
        self
    }

    /// Selects the reviewed provider native-retry mode.
    #[must_use]
    pub fn provider_retry(mut self, retry: ProviderRetry) -> Self {
        self.config.provider_retry = retry;
        self
    }

    /// Retains a root-certificate PEM for parsing during client construction.
    #[cfg(feature = "default-tls")]
    #[must_use]
    pub fn add_root_certificate_pem(mut self, pem: impl Into<Vec<u8>>) -> Self {
        self.config.root_certificates_pem.push(pem.into());
        self
    }

    /// Retains an application-client identity PEM for parsing during client
    /// construction. The provider client never receives this identity.
    #[cfg(feature = "default-tls")]
    #[must_use]
    pub fn identity_pem(mut self, pem: impl Into<Vec<u8>>) -> Self {
        self.config.identity_pem = Some(pem.into());
        self
    }

    /// Disables gzip response decompression.
    #[cfg(feature = "gzip")]
    #[must_use]
    pub fn disable_gzip(mut self) -> Self {
        self.config.disable_gzip = true;
        self
    }

    /// Disables Brotli response decompression.
    #[cfg(feature = "brotli")]
    #[must_use]
    pub fn disable_brotli(mut self) -> Self {
        self.config.disable_brotli = true;
        self
    }

    /// Disables Deflate response decompression.
    #[cfg(feature = "deflate")]
    #[must_use]
    pub fn disable_deflate(mut self) -> Self {
        self.config.disable_deflate = true;
        self
    }

    /// Selects an application-owned native Reqwest retry builder.
    #[cfg(feature = "dangerous-custom-retry")]
    #[must_use]
    pub fn dangerous_custom_retry(
        mut self,
        builder: crate::dangerous::ReqwestRetryBuilder,
    ) -> Self {
        self.config.application_retry = ApplicationRetry::Dangerous(builder);
        self
    }

    #[cfg(feature = "dangerous-custom-retry")]
    pub(crate) fn generated_custom_retry_factory(
        mut self,
        factory: fn() -> crate::dangerous::ReqwestRetryBuilder,
    ) -> Self {
        self.config.application_retry = ApplicationRetry::DangerousFactory(factory);
        self
    }

    #[allow(dead_code, reason = "consumed by managed Reqwest construction")]
    pub(crate) fn resolve(self) -> HttpConfig {
        self.config
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "dangerous-custom-retry")]
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[cfg(feature = "dangerous-custom-retry")]
    static OVERRIDDEN_FACTORY_CALLS: AtomicUsize = AtomicUsize::new(0);

    #[cfg(feature = "dangerous-custom-retry")]
    fn overridden_factory() -> crate::dangerous::ReqwestRetryBuilder {
        OVERRIDDEN_FACTORY_CALLS.fetch_add(1, Ordering::SeqCst);
        reqwest::retry::never()
    }

    #[test]
    fn defaults_are_plain_and_application_retry_is_disabled() {
        let config = HttpConfigBuilder::new().resolve();
        assert!(config.connect_timeout.is_none());
        assert!(config.read_timeout.is_none());
        assert!(config.pool_idle_timeout.is_none());
        assert!(config.pool_max_idle_per_host.is_none());
        assert!(config.tcp_keepalive.is_none());
        assert!(config.tcp_nodelay.is_none());
        assert!(!config.https_only);
        assert_eq!(config.protocol, ProtocolMode::Negotiated);
        assert!(config.proxy.is_none());
        assert!(matches!(
            config.application_retry,
            ApplicationRetry::Disabled
        ));
        assert_eq!(config.provider_retry, ProviderRetry::ProtocolRecovery);
    }

    #[test]
    fn always_available_setters_replace_reviewed_values() {
        let config = HttpConfigBuilder::new()
            .connect_timeout(Duration::from_secs(1))
            .read_timeout(Duration::from_secs(2))
            .pool_idle_timeout(Duration::from_secs(3))
            .pool_max_idle_per_host(4)
            .tcp_keepalive(Duration::from_secs(5))
            .tcp_nodelay(false)
            .https_only(true)
            .http1_only()
            .proxy(SafeProxy::parse("https://proxy.example").expect("safe proxy"))
            .retry_reqwest_default()
            .provider_retry(ProviderRetry::Disabled)
            .resolve();
        assert_eq!(config.connect_timeout, Some(Duration::from_secs(1)));
        assert_eq!(config.read_timeout, Some(Duration::from_secs(2)));
        assert_eq!(config.pool_idle_timeout, Some(Duration::from_secs(3)));
        assert_eq!(config.pool_max_idle_per_host, Some(4));
        assert_eq!(config.tcp_keepalive, Some(Duration::from_secs(5)));
        assert_eq!(config.tcp_nodelay, Some(false));
        assert!(config.https_only);
        assert_eq!(config.protocol, ProtocolMode::Http1Only);
        assert_eq!(
            config.proxy.as_ref().map(SafeProxy::as_str),
            Some("https://proxy.example/")
        );
        assert!(matches!(
            config.application_retry,
            ApplicationRetry::ReqwestDefault
        ));
        assert_eq!(config.provider_retry, ProviderRetry::Disabled);
    }

    #[test]
    fn retry_off_overrides_an_explicit_opt_in() {
        let config = HttpConfigBuilder::new()
            .retry_reqwest_default()
            .retry_off()
            .resolve();
        assert!(matches!(
            config.application_retry,
            ApplicationRetry::Disabled
        ));
    }

    #[cfg(feature = "http2")]
    #[test]
    fn http2_prior_knowledge_replaces_http1_selection() {
        let config = HttpConfigBuilder::new()
            .http1_only()
            .http2_prior_knowledge()
            .resolve();
        assert_eq!(config.protocol, ProtocolMode::Http2PriorKnowledge);
    }

    #[cfg(feature = "default-tls")]
    #[test]
    fn pem_inputs_are_retained_without_parsing() {
        let config = HttpConfigBuilder::new()
            .add_root_certificate_pem(b"root-one".to_vec())
            .add_root_certificate_pem(b"root-two".to_vec())
            .identity_pem(b"identity".to_vec())
            .resolve();
        assert_eq!(
            config.root_certificates_pem,
            [b"root-one".to_vec(), b"root-two".to_vec()]
        );
        assert_eq!(config.identity_pem.as_deref(), Some(&b"identity"[..]));
    }

    #[cfg(all(feature = "gzip", feature = "brotli", feature = "deflate"))]
    #[test]
    fn decompression_disablement_is_independent_plain_state() {
        let config = HttpConfigBuilder::new()
            .disable_gzip()
            .disable_brotli()
            .disable_deflate()
            .resolve();
        assert!(config.disable_gzip);
        assert!(config.disable_brotli);
        assert!(config.disable_deflate);
    }

    #[cfg(feature = "dangerous-custom-retry")]
    #[test]
    fn dangerous_retry_selection_retains_only_the_reviewed_native_builder_type() {
        let config = HttpConfigBuilder::new()
            .dangerous_custom_retry(reqwest::retry::never())
            .resolve();
        assert!(matches!(
            config.application_retry,
            ApplicationRetry::Dangerous(_)
        ));
    }

    #[cfg(feature = "dangerous-custom-retry")]
    #[test]
    fn overriding_a_generated_custom_retry_selection_never_invokes_its_factory() {
        OVERRIDDEN_FACTORY_CALLS.store(0, Ordering::SeqCst);
        let config = HttpConfigBuilder::new()
            .generated_custom_retry_factory(overridden_factory)
            .retry_off()
            .resolve();
        assert!(matches!(
            config.application_retry,
            ApplicationRetry::Disabled
        ));
        assert_eq!(OVERRIDDEN_FACTORY_CALLS.load(Ordering::SeqCst), 0);
    }
}

use std::time::Duration;

use crate::error::{ClientBuildError, ClientBuildErrorKind};

use super::SafeProxy;
#[cfg(feature = "default-tls")]
use super::{
    MAX_IDENTITY_PEM_BYTES, MAX_ROOT_CERTIFICATE_BYTES, MAX_ROOT_CERTIFICATES,
    MAX_ROOT_CERTIFICATES_TOTAL_BYTES,
};

/// Reviewed provider native-retry modes.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderRetry {
    /// Permit Reqwest's protocol-recovery behavior.
    ProtocolRecovery,
    /// Disable provider native retries.
    Disabled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProtocolMode {
    Negotiated,
    Http1Only,
    #[cfg(feature = "http2")]
    Http2PriorKnowledge,
}

/// Plain resolved HTTP configuration. It contains no Reqwest client or
/// `reqwest::ClientBuilder`.
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
    pub(crate) protocol_recovery: bool,
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
///
/// An explicit proxy and custom root certificates apply to both the
/// application and isolated provider clients. An application identity applies
/// only to the application client and is never installed on provider HTTP.
pub struct HttpConfigBuilder {
    config: HttpConfig,
    #[cfg(feature = "default-tls")]
    root_certificates_total_bytes: usize,
    startup_error: Option<ClientBuildErrorKind>,
}

impl HttpConfigBuilder {
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
                protocol_recovery: true,
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
            #[cfg(feature = "default-tls")]
            root_certificates_total_bytes: 0,
            startup_error: None,
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

    /// Requires HTTPS for the managed application client.
    #[must_use]
    pub fn require_https(mut self) -> Self {
        self.config.https_only = true;
        self
    }

    /// Explicitly permits either HTTP or HTTPS for the managed application
    /// client.
    #[must_use]
    pub fn allow_http(mut self) -> Self {
        self.config.https_only = false;
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

    /// Installs one explicitly validated proxy origin for both application and
    /// provider HTTP.
    #[must_use]
    pub fn proxy(mut self, proxy: SafeProxy) -> Self {
        self.config.proxy = Some(proxy);
        self
    }

    /// Enables or disables private known-safe protocol recovery.
    ///
    /// Protocol recovery is enabled by default. It never enables status retry,
    /// exposes no native policy type, and remains inside one visible Concord
    /// attempt with at most two physical tries in total, and only for an
    /// HTTP/2 stream the protocol proves unprocessed (`REFUSED_STREAM` or a
    /// graceful `GOAWAY`). Without HTTP/2 support it has no effect.
    #[must_use]
    pub fn protocol_recovery(mut self, enabled: bool) -> Self {
        self.config.protocol_recovery = enabled;
        self
    }

    /// Selects the reviewed provider native-retry mode.
    #[must_use]
    pub fn provider_retry(mut self, retry: ProviderRetry) -> Self {
        self.config.provider_retry = retry;
        self
    }

    /// Retains a structurally bounded root-certificate PEM for both application
    /// and provider client construction.
    ///
    /// Structural excess is latched without retaining the rejected bytes and
    /// makes the eventual client build fail. TLS parsing may reject a
    /// structurally valid input later.
    #[cfg(feature = "default-tls")]
    #[must_use]
    pub fn add_root_certificate_pem(mut self, pem: impl AsRef<[u8]>) -> Self {
        if self.startup_error.is_some() {
            return self;
        }
        let pem = pem.as_ref();
        let next_total = self.root_certificates_total_bytes.checked_add(pem.len());
        let valid = self.config.root_certificates_pem.len() < MAX_ROOT_CERTIFICATES
            && pem.len() <= MAX_ROOT_CERTIFICATE_BYTES
            && next_total.is_some_and(|total| total <= MAX_ROOT_CERTIFICATES_TOTAL_BYTES);
        if !valid {
            self.startup_error = Some(ClientBuildErrorKind::InvalidRootCertificate);
            return self;
        }
        self.config.root_certificates_pem.push(pem.to_vec());
        self.root_certificates_total_bytes =
            next_total.expect("validated root-certificate aggregate bytes");
        self
    }

    /// Retains a structurally bounded application-client identity PEM for
    /// parsing during client construction. The provider client never receives
    /// this identity.
    ///
    /// Structural excess is latched without retaining the rejected bytes and
    /// makes the eventual client build fail. TLS parsing may reject a
    /// structurally valid input later.
    #[cfg(feature = "default-tls")]
    #[must_use]
    pub fn identity_pem(mut self, pem: impl AsRef<[u8]>) -> Self {
        if self.startup_error.is_some() {
            return self;
        }
        let pem = pem.as_ref();
        if pem.len() > MAX_IDENTITY_PEM_BYTES {
            self.startup_error = Some(ClientBuildErrorKind::InvalidClientIdentity);
            return self;
        }
        self.config.identity_pem = Some(pem.to_vec());
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

    pub(crate) fn resolve(self) -> Result<HttpConfig, ClientBuildError> {
        match self.startup_error {
            Some(kind) => Err(ClientBuildError::new(kind)),
            None => Ok(self.config),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "default-tls")]
    fn startup_material_error(builder: HttpConfigBuilder) -> ClientBuildError {
        match builder.resolve() {
            Ok(_) => panic!("structurally excessive startup material must fail"),
            Err(error) => error,
        }
    }

    #[cfg(feature = "default-tls")]
    fn assert_redacted_startup_material_error(
        builder: HttpConfigBuilder,
        expected: ClientBuildErrorKind,
        sentinel: &str,
    ) {
        let error = startup_material_error(builder);
        assert_eq!(error.kind(), expected);
        assert_eq!(error.input_field(), None);
        assert_eq!(error.input_kind(), None);
        for rendered in [error.to_string(), format!("{error:?}")] {
            assert!(!rendered.contains(sentinel));
        }
        assert!(std::error::Error::source(&error).is_none());
    }

    #[cfg(feature = "default-tls")]
    fn root_builder_with_bytes(mut bytes: usize) -> HttpConfigBuilder {
        let chunk = vec![b'a'; MAX_ROOT_CERTIFICATE_BYTES];
        let mut builder = HttpConfigBuilder::new();
        while bytes >= chunk.len() {
            builder = builder.add_root_certificate_pem(&chunk);
            bytes -= chunk.len();
        }
        if bytes != 0 {
            builder = builder.add_root_certificate_pem(&chunk[..bytes]);
        }
        builder
    }

    #[test]
    fn defaults_are_plain_and_protocol_recovery_is_enabled() {
        let config = HttpConfigBuilder::new()
            .resolve()
            .expect("default HTTP configuration");
        assert!(config.connect_timeout.is_none());
        assert!(config.read_timeout.is_none());
        assert!(config.pool_idle_timeout.is_none());
        assert!(config.pool_max_idle_per_host.is_none());
        assert!(config.tcp_keepalive.is_none());
        assert!(config.tcp_nodelay.is_none());
        assert!(!config.https_only);
        assert_eq!(config.protocol, ProtocolMode::Negotiated);
        assert!(config.proxy.is_none());
        assert!(config.protocol_recovery);
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
            .require_https()
            .http1_only()
            .proxy(SafeProxy::parse("https://proxy.example").expect("safe proxy"))
            .protocol_recovery(false)
            .provider_retry(ProviderRetry::Disabled)
            .resolve()
            .expect("configured HTTP configuration");
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
        assert!(!config.protocol_recovery);
        assert_eq!(config.provider_retry, ProviderRetry::Disabled);
    }

    #[test]
    fn protocol_recovery_selection_replaces_in_call_order() {
        let config = HttpConfigBuilder::new()
            .protocol_recovery(false)
            .protocol_recovery(true)
            .resolve()
            .expect("re-enabled protocol recovery");
        assert!(config.protocol_recovery);
    }

    #[cfg(feature = "http2")]
    #[test]
    fn http2_prior_knowledge_replaces_http1_selection() {
        let config = HttpConfigBuilder::new()
            .http1_only()
            .http2_prior_knowledge()
            .resolve()
            .expect("HTTP/2 prior knowledge");
        assert_eq!(config.protocol, ProtocolMode::Http2PriorKnowledge);
    }

    #[cfg(feature = "default-tls")]
    #[test]
    fn pem_inputs_are_retained_without_parsing() {
        let config = HttpConfigBuilder::new()
            .add_root_certificate_pem(b"root-one")
            .add_root_certificate_pem(b"root-two")
            .identity_pem(b"identity")
            .resolve()
            .expect("bounded TLS material");
        assert_eq!(
            config.root_certificates_pem,
            [b"root-one".to_vec(), b"root-two".to_vec()]
        );
        assert_eq!(config.identity_pem.as_deref(), Some(&b"identity"[..]));
    }

    #[cfg(feature = "default-tls")]
    #[test]
    fn root_certificate_limits_accept_exact_and_withhold_first_excess() {
        const SENTINEL: &[u8] = b"root-certificate-limit-sentinel-64be";
        let exact_certificate = vec![b'r'; MAX_ROOT_CERTIFICATE_BYTES];
        let exact = HttpConfigBuilder::new().add_root_certificate_pem(&exact_certificate);
        assert_eq!(exact.config.root_certificates_pem.len(), 1);
        assert_eq!(
            exact.config.root_certificates_pem[0].len(),
            MAX_ROOT_CERTIFICATE_BYTES
        );
        assert_eq!(
            exact.root_certificates_total_bytes,
            MAX_ROOT_CERTIFICATE_BYTES
        );
        exact.resolve().expect("exact root-certificate byte limit");

        let mut oversized_certificate = vec![b'x'; MAX_ROOT_CERTIFICATE_BYTES + 1];
        oversized_certificate[..SENTINEL.len()].copy_from_slice(SENTINEL);
        let oversized = HttpConfigBuilder::new().add_root_certificate_pem(&oversized_certificate);
        assert!(oversized.config.root_certificates_pem.is_empty());
        assert_eq!(oversized.root_certificates_total_bytes, 0);
        assert_redacted_startup_material_error(
            oversized,
            ClientBuildErrorKind::InvalidRootCertificate,
            std::str::from_utf8(SENTINEL).expect("ASCII sentinel"),
        );
    }

    #[cfg(feature = "default-tls")]
    #[test]
    fn root_certificate_count_accepts_exact_and_withholds_first_excess() {
        const SENTINEL: &[u8] = b"root-certificate-count-sentinel-b620";
        let mut builder = HttpConfigBuilder::new();
        for _ in 0..MAX_ROOT_CERTIFICATES {
            builder = builder.add_root_certificate_pem(b"x");
        }
        assert_eq!(
            builder.config.root_certificates_pem.len(),
            MAX_ROOT_CERTIFICATES
        );
        assert_eq!(builder.root_certificates_total_bytes, MAX_ROOT_CERTIFICATES);
        assert!(builder.startup_error.is_none());

        builder = builder.add_root_certificate_pem(SENTINEL);
        assert_eq!(
            builder.config.root_certificates_pem.len(),
            MAX_ROOT_CERTIFICATES
        );
        assert_eq!(builder.root_certificates_total_bytes, MAX_ROOT_CERTIFICATES);
        assert!(
            builder
                .config
                .root_certificates_pem
                .iter()
                .all(|pem| pem.as_slice() != SENTINEL)
        );
        assert_redacted_startup_material_error(
            builder,
            ClientBuildErrorKind::InvalidRootCertificate,
            std::str::from_utf8(SENTINEL).expect("ASCII sentinel"),
        );
    }

    #[cfg(feature = "default-tls")]
    #[test]
    fn root_certificate_aggregate_accepts_exact_and_withholds_first_excess() {
        const SENTINEL: &[u8] = b"root-certificate-aggregate-sentinel-aa4d";
        let certificate_count = MAX_ROOT_CERTIFICATES_TOTAL_BYTES / MAX_ROOT_CERTIFICATE_BYTES;
        let exact = root_builder_with_bytes(MAX_ROOT_CERTIFICATES_TOTAL_BYTES);
        assert_eq!(
            exact.root_certificates_total_bytes,
            MAX_ROOT_CERTIFICATES_TOTAL_BYTES
        );
        assert_eq!(exact.config.root_certificates_pem.len(), certificate_count);
        exact
            .resolve()
            .expect("exact aggregate root-certificate byte limit");

        let retained_before_excess = MAX_ROOT_CERTIFICATES_TOTAL_BYTES - SENTINEL.len() + 1;
        assert_eq!(
            retained_before_excess + SENTINEL.len(),
            MAX_ROOT_CERTIFICATES_TOTAL_BYTES + 1
        );
        let mut builder = root_builder_with_bytes(retained_before_excess);
        let certificate_count = builder.config.root_certificates_pem.len();
        builder = builder.add_root_certificate_pem(SENTINEL);
        assert_eq!(
            builder.root_certificates_total_bytes,
            retained_before_excess
        );
        assert_eq!(
            builder.config.root_certificates_pem.len(),
            certificate_count
        );
        assert!(
            builder
                .config
                .root_certificates_pem
                .iter()
                .all(|pem| pem.as_slice() != SENTINEL)
        );
        assert_redacted_startup_material_error(
            builder,
            ClientBuildErrorKind::InvalidRootCertificate,
            std::str::from_utf8(SENTINEL).expect("ASCII sentinel"),
        );
    }

    #[cfg(feature = "default-tls")]
    #[test]
    fn identity_limit_accepts_exact_and_withholds_first_excess() {
        const SENTINEL: &[u8] = b"identity-limit-sentinel-567f";
        let exact_identity = vec![b'i'; MAX_IDENTITY_PEM_BYTES];
        let exact = HttpConfigBuilder::new().identity_pem(&exact_identity);
        assert_eq!(
            exact.config.identity_pem.as_ref().map(Vec::len),
            Some(MAX_IDENTITY_PEM_BYTES)
        );
        exact.resolve().expect("exact identity byte limit");

        let mut oversized_identity = vec![b'x'; MAX_IDENTITY_PEM_BYTES + 1];
        oversized_identity[..SENTINEL.len()].copy_from_slice(SENTINEL);
        let oversized = HttpConfigBuilder::new().identity_pem(&oversized_identity);
        assert!(oversized.config.identity_pem.is_none());
        assert_redacted_startup_material_error(
            oversized,
            ClientBuildErrorKind::InvalidClientIdentity,
            std::str::from_utf8(SENTINEL).expect("ASCII sentinel"),
        );
    }

    #[cfg(feature = "default-tls")]
    #[test]
    fn first_structural_startup_failure_kind_wins_without_retaining_later_material() {
        let oversized_root = vec![b'r'; MAX_ROOT_CERTIFICATE_BYTES + 1];
        let oversized_identity = vec![b'i'; MAX_IDENTITY_PEM_BYTES + 1];

        let root_first = HttpConfigBuilder::new()
            .add_root_certificate_pem(&oversized_root)
            .identity_pem(&oversized_identity);
        assert_eq!(
            startup_material_error(root_first).kind(),
            ClientBuildErrorKind::InvalidRootCertificate
        );

        let identity_first = HttpConfigBuilder::new()
            .identity_pem(&oversized_identity)
            .add_root_certificate_pem(&oversized_root);
        assert_eq!(
            startup_material_error(identity_first).kind(),
            ClientBuildErrorKind::InvalidClientIdentity
        );
    }

    #[cfg(all(feature = "gzip", feature = "brotli", feature = "deflate"))]
    #[test]
    fn decompression_disablement_is_independent_plain_state() {
        let config = HttpConfigBuilder::new()
            .disable_gzip()
            .disable_brotli()
            .disable_deflate()
            .resolve()
            .expect("disabled content decoding");
        assert!(config.disable_gzip);
        assert!(config.disable_brotli);
        assert!(config.disable_deflate);
    }
}

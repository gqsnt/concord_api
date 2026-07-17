use crate::error::{ClientBuildError, ClientBuildErrorKind};
use crate::runtime::RuntimeConfig;

use super::application::ApplicationHttp;
use super::config::{ApplicationRetry, HttpConfig, ProtocolMode, ProviderRetry};
use super::provider::ProviderHttpInner;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TlsCapability {
    Available,
    Unavailable,
}

pub(crate) struct ManagedHttp {
    pub(crate) application: ApplicationHttp,
    pub(super) provider: ProviderHttpInner,
}

impl ManagedHttp {
    pub(crate) fn validate_fixed_base(
        http: &HttpConfig,
        base_url: &url::Url,
    ) -> Result<(), ClientBuildError> {
        if http.https_only && base_url.scheme() != "https" {
            return Err(ClientBuildError::new(
                ClientBuildErrorKind::HttpConfiguration,
            ));
        }
        #[cfg(not(feature = "default-tls"))]
        if base_url.scheme() == "https" {
            return Err(ClientBuildError::new(ClientBuildErrorKind::TlsUnavailable));
        }
        Ok(())
    }

    pub(crate) fn build(
        http: HttpConfig,
        runtime: &RuntimeConfig,
    ) -> Result<Self, ClientBuildError> {
        #[cfg(feature = "default-tls")]
        let tls = TlsCapability::Available;
        #[cfg(not(feature = "default-tls"))]
        let tls = TlsCapability::Unavailable;

        #[cfg(feature = "default-tls")]
        let tls_material = parse_tls_material(&http)?;
        let application_https_only = http.https_only;

        let mut application = base_builder(&http, false)?;
        let mut provider = base_builder(&http, true)?;
        application = match http.application_retry {
            ApplicationRetry::Disabled => application.retry(reqwest::retry::never()),
            ApplicationRetry::ReqwestDefault => application,
            #[cfg(feature = "dangerous-custom-retry")]
            ApplicationRetry::Dangerous(retry) => application.retry(retry),
        };
        provider = match http.provider_retry {
            ProviderRetry::ProtocolRecovery => provider,
            ProviderRetry::Disabled => provider.retry(reqwest::retry::never()),
        };

        #[cfg(feature = "default-tls")]
        {
            for certificate in &tls_material.certificates {
                application = application.add_root_certificate(certificate.clone());
                provider = provider.add_root_certificate(certificate.clone());
            }
            if let Some(identity) = tls_material.identity {
                application = application.identity(identity);
            }
        }

        let application = application.build().map_err(safe_client_build_error)?;
        let provider = provider.build().map_err(safe_client_build_error)?;
        Ok(Self {
            application: ApplicationHttp::new(
                application,
                tls,
                application_https_only,
                runtime.status_body_limit,
                runtime.rate_limit_cooldown_cap,
            ),
            provider: ProviderHttpInner::new(
                provider,
                tls,
                runtime.provider_request_limit,
                runtime.provider_response_limit,
            ),
        })
    }

    #[cfg(any(test, feature = "dangerous-dev-tools"))]
    pub(crate) fn build_with_development(
        http: HttpConfig,
        runtime: &RuntimeConfig,
        executor: &concord_test_support::executor::DeterministicExecutor,
    ) -> Result<Self, ClientBuildError> {
        let managed = Self::build(http, runtime)?;
        Ok(Self {
            application: managed.application.with_development(executor.application()),
            provider: managed.provider.with_development(executor.provider()),
        })
    }
}

fn base_builder(
    config: &HttpConfig,
    provider: bool,
) -> Result<reqwest::ClientBuilder, ClientBuildError> {
    let mut builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy();
    if provider || config.https_only {
        builder = builder.https_only(true);
    }
    if let Some(timeout) = config.connect_timeout {
        builder = builder.connect_timeout(timeout);
    }
    if let Some(timeout) = config.read_timeout {
        builder = builder.read_timeout(timeout);
    }
    if let Some(timeout) = config.pool_idle_timeout {
        builder = builder.pool_idle_timeout(timeout);
    }
    if let Some(count) = config.pool_max_idle_per_host {
        builder = builder.pool_max_idle_per_host(count);
    }
    if let Some(keepalive) = config.tcp_keepalive {
        builder = builder.tcp_keepalive(keepalive);
    }
    if let Some(enabled) = config.tcp_nodelay {
        builder = builder.tcp_nodelay(enabled);
    }
    builder = match config.protocol {
        ProtocolMode::Negotiated => builder,
        ProtocolMode::Http1Only => builder.http1_only(),
        #[cfg(feature = "http2")]
        ProtocolMode::Http2PriorKnowledge => builder.http2_prior_knowledge(),
    };
    if let Some(proxy) = &config.proxy {
        let proxy = reqwest::Proxy::all(proxy.as_str()).map_err(|error| {
            drop(error);
            ClientBuildError::new(ClientBuildErrorKind::HttpConfiguration)
        })?;
        builder = builder.proxy(proxy);
    }
    #[cfg(feature = "gzip")]
    if config.disable_gzip {
        builder = builder.no_gzip();
    }
    #[cfg(feature = "brotli")]
    if config.disable_brotli {
        builder = builder.no_brotli();
    }
    #[cfg(feature = "deflate")]
    if config.disable_deflate {
        builder = builder.no_deflate();
    }
    Ok(builder)
}

fn safe_client_build_error(error: reqwest::Error) -> ClientBuildError {
    drop(error);
    ClientBuildError::new(ClientBuildErrorKind::HttpConfiguration)
}

#[cfg(feature = "default-tls")]
struct TlsMaterial {
    certificates: Vec<reqwest::Certificate>,
    identity: Option<reqwest::Identity>,
}

#[cfg(feature = "default-tls")]
fn parse_tls_material(config: &HttpConfig) -> Result<TlsMaterial, ClientBuildError> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut certificates = Vec::new();
    for pem in &config.root_certificates_pem {
        let parsed = reqwest::Certificate::from_pem_bundle(pem).map_err(|error| {
            drop(error);
            ClientBuildError::new(ClientBuildErrorKind::HttpConfiguration)
        })?;
        if parsed.is_empty() {
            return Err(ClientBuildError::new(
                ClientBuildErrorKind::HttpConfiguration,
            ));
        }
        certificates.extend(parsed);
    }
    let identity = config
        .identity_pem
        .as_ref()
        .map(|pem| {
            reqwest::Identity::from_pem(pem).map_err(|error| {
                drop(error);
                ClientBuildError::new(ClientBuildErrorKind::HttpConfiguration)
            })
        })
        .transpose()?;
    Ok(TlsMaterial {
        certificates,
        identity,
    })
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;
    use std::net::SocketAddr;
    use std::process::Command;
    #[cfg(feature = "http2")]
    use std::sync::Arc;
    #[cfg(feature = "http2")]
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use concord_test_support::proxy::ProxyTrap;
    use concord_test_support::server::{ResponseScript, ScriptedServer};
    #[cfg(feature = "default-tls")]
    use concord_test_support::tls::TlsServer;
    #[cfg(feature = "http2")]
    use h2::Reason;
    use http::Method;
    #[cfg(feature = "http2")]
    use tokio::net::TcpListener as TokioTcpListener;

    use super::*;
    #[cfg(feature = "default-tls")]
    use crate::error::ClientBuildErrorKind;
    use crate::error::{ConfigurationErrorKind, ErrorCategory};
    use crate::http::{HttpConfigBuilder, SafeProxy};
    use crate::runtime::RuntimeConfigBuilder;

    const WAIT: Duration = Duration::from_secs(3);

    fn managed(builder: HttpConfigBuilder) -> ManagedHttp {
        let runtime = RuntimeConfigBuilder::new().resolve();
        ManagedHttp::build(builder.resolve(), &runtime).expect("managed clients should build")
    }

    fn assert_send_sync_static<T: Send + Sync + 'static>() {}

    #[test]
    fn managed_owners_are_send_sync_static_and_private_by_construction() {
        assert_send_sync_static::<ApplicationHttp>();
        assert_send_sync_static::<ProviderHttpInner>();
        assert_send_sync_static::<ManagedHttp>();
    }

    #[tokio::test]
    async fn redirects_are_not_followed_by_application_client() {
        let server = ScriptedServer::start([
            ResponseScript::status(302, Vec::new()).with_header("Location", "/target"),
            ResponseScript::ok(b"unexpected".to_vec()),
        ])
        .expect("redirect server should start");
        let managed = managed(HttpConfigBuilder::new().http1_only());
        let response = managed
            .application
            .get("managed.redirect", &server.url("/redirect"))
            .await
            .expect("redirect response should remain terminal");
        assert_eq!(response.status(), reqwest::StatusCode::FOUND);
        server
            .wait_for_requests(1, WAIT)
            .expect("one application request should arrive");
        assert_eq!(server.request_count(), 1);
    }

    #[tokio::test]
    async fn cookies_are_not_stored_or_replayed() {
        let server = ScriptedServer::start([
            ResponseScript::ok(b"set".to_vec())
                .with_header("Set-Cookie", "managed_cookie=sentinel; Path=/"),
            ResponseScript::ok(b"check".to_vec()),
        ])
        .expect("cookie server should start");
        let managed = managed(HttpConfigBuilder::new().http1_only());
        for path in ["/set", "/check"] {
            managed
                .application
                .get("managed.cookie", &server.url(path))
                .await
                .expect("cookie probe response should arrive")
                .bytes()
                .await
                .expect("cookie probe body should complete");
        }
        let requests = server.requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[1].header("cookie"), None);
    }

    #[tokio::test]
    async fn explicit_safe_proxy_is_used() {
        let proxy = ProxyTrap::start([ResponseScript::ok(b"proxied".to_vec())])
            .expect("proxy should start");
        let proxy_origin = SafeProxy::parse(proxy.url()).expect("proxy origin should be safe");
        let managed = managed(HttpConfigBuilder::new().proxy(proxy_origin).http1_only());
        let response = managed
            .application
            .get("managed.proxy", "http://example.invalid/resource")
            .await
            .expect("explicit proxy should answer");
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        proxy
            .wait_for_requests(1, WAIT)
            .expect("explicit proxy should receive one request");
        assert_eq!(
            proxy.requests()[0].target(),
            "http://example.invalid/resource"
        );
    }

    #[tokio::test]
    async fn system_proxy_environment_is_ignored_by_managed_client() {
        const CHILD: &str = "CONCORD_MANAGED_PROXY_CHILD";
        const ORIGIN: &str = "CONCORD_MANAGED_ORIGIN";
        if std::env::var_os(CHILD).is_some() {
            let origin: SocketAddr = std::env::var(ORIGIN)
                .expect("child origin should exist")
                .parse()
                .expect("child origin should parse");
            let managed = managed(HttpConfigBuilder::new().http1_only());
            let response = managed
                .application
                .get("managed.proxy", &format!("http://{origin}/direct"))
                .await
                .expect("managed client should bypass environment proxy");
            assert_eq!(response.status(), reqwest::StatusCode::OK);
            return;
        }

        let origin = ScriptedServer::start([ResponseScript::ok(b"direct".to_vec())])
            .expect("origin should start");
        let trap = ProxyTrap::start([ResponseScript::ok(b"proxy".to_vec())])
            .expect("proxy trap should start");
        let output = Command::new(std::env::current_exe().expect("test executable should exist"))
            .args([
                "--exact",
                "http::build::tests::system_proxy_environment_is_ignored_by_managed_client",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env(ORIGIN, origin.address().to_string())
            .env("HTTP_PROXY", trap.url())
            .env("HTTPS_PROXY", trap.url())
            .env("ALL_PROXY", trap.url())
            .env("NO_PROXY", "")
            .env("http_proxy", trap.url())
            .env("https_proxy", trap.url())
            .env("all_proxy", trap.url())
            .env("no_proxy", "")
            .output()
            .expect("managed proxy child should run");
        assert!(
            output.status.success(),
            "child failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        origin
            .wait_for_requests(1, WAIT)
            .expect("direct origin should be reached");
        assert_eq!(trap.request_count(), 0);
    }

    #[test]
    fn cleartext_preflights_run_before_protected_or_provider_body_work() {
        const URL_SENTINEL: &str = "protected-cleartext-sentinel-8b71d4";
        let server = ScriptedServer::start([ResponseScript::ok(b"must-not-arrive".to_vec())])
            .expect("trap origin should start");
        let managed = managed(HttpConfigBuilder::new());

        let protected = managed.application.prepare(
            "managed.protected",
            Method::GET,
            &server.url(&format!("/protected/{URL_SENTINEL}")),
            true,
        );
        let error = protected.expect_err("protected loopback HTTP must be rejected");
        assert_eq!(error.category(), ErrorCategory::Configuration);
        assert_eq!(
            error.configuration_kind(),
            Some(ConfigurationErrorKind::InvalidAuthentication)
        );
        assert!(!error.to_string().contains(URL_SENTINEL));
        assert!(!format!("{error:?}").contains(URL_SENTINEL));

        let provider =
            managed
                .provider
                .prepare(Method::POST, &server.url("/token"), Some(usize::MAX));
        let error = provider.expect_err("provider scheme must precede body-limit work");
        assert_eq!(error, ErrorCategory::Configuration);
        assert_eq!(server.request_count(), 0);
    }

    #[test]
    fn application_owner_retains_effective_https_only_preflight_metadata() {
        let ordinary = managed(HttpConfigBuilder::new());
        assert!(!ordinary.application.https_only());

        let restricted = managed(HttpConfigBuilder::new().https_only(true));
        assert!(restricted.application.https_only());
    }

    #[cfg(not(feature = "default-tls"))]
    #[tokio::test]
    async fn no_default_http_works_and_https_preflight_is_typed() {
        // Core's TLS-enabled Reqwest dev dependency is feature-unified into
        // unit tests. A real no-default downstream dependency has no Rustls
        // backend at all; installing this fixed test provider only satisfies
        // the dev-only unified Reqwest build.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let server = ScriptedServer::start([ResponseScript::ok(b"plain".to_vec())])
            .expect("plain server should start");
        let managed = managed(HttpConfigBuilder::new().http1_only());
        let response = managed
            .application
            .get("managed.plain", &server.url("/plain"))
            .await
            .expect("no-default application HTTP should work");
        assert_eq!(response.status(), reqwest::StatusCode::OK);

        let error = managed
            .application
            .prepare(
                "managed.https",
                Method::GET,
                "https://localhost.invalid/resource",
                false,
            )
            .expect_err("unavailable TLS should fail before native work");
        assert_eq!(error.category(), ErrorCategory::Configuration);

        let provider_error = managed
            .provider
            .prepare(Method::GET, "https://localhost.invalid/provider", None)
            .expect_err("provider TLS must be unavailable before native work");
        assert_eq!(provider_error, ErrorCategory::Configuration);
    }

    #[cfg(feature = "default-tls")]
    #[tokio::test]
    async fn additional_roots_apply_to_both_but_identity_is_application_only() {
        const ROOT_PEM_SENTINEL: &str = "MIIDJTCCAg2gAwIBAgIUVVfN";
        const IDENTITY_PEM_SENTINEL: &str = "MIIDOTCCAiGgAwIBAgIUJwMj";

        let trust_server = TlsServer::start().expect("TLS server should start");
        let mtls_server = TlsServer::start_mtls().expect("mTLS server should start");
        let root_pem = trust_server.certificate_pem();
        let identity_pem = trust_server.identity_pem();
        assert!(
            String::from_utf8_lossy(&root_pem).contains(ROOT_PEM_SENTINEL),
            "root sentinel must be present in configured input"
        );
        assert!(
            String::from_utf8_lossy(&identity_pem).contains(IDENTITY_PEM_SENTINEL),
            "identity sentinel must be present in configured input"
        );
        let managed = managed(
            HttpConfigBuilder::new()
                .http1_only()
                .add_root_certificate_pem(root_pem)
                .identity_pem(identity_pem),
        );

        let application = managed
            .application
            .get(
                "managed.tls",
                &trust_server.url("localhost", "/application-root"),
            )
            .await
            .expect("application root should validate");
        assert_eq!(application.status(), reqwest::StatusCode::OK);
        application
            .bytes()
            .await
            .expect("application TLS body should complete");

        let request = managed
            .provider
            .prepare(
                Method::GET,
                &trust_server.url("localhost", "/provider-root"),
                None,
            )
            .expect("provider HTTPS should prepare");
        let provider = managed
            .provider
            .execute_native("managed.provider", Method::GET, request)
            .await
            .expect("provider root should validate");
        assert_eq!(provider.status(), reqwest::StatusCode::OK);
        provider
            .bytes()
            .await
            .expect("provider TLS body should complete");
        trust_server
            .wait_for_requests(2, WAIT)
            .expect("both managed clients should reach TLS server");
        let requests = trust_server.requests();
        let targets: Vec<_> = requests
            .iter()
            .map(|request| request.target().to_owned())
            .collect();
        assert_eq!(targets, ["/application-root", "/provider-root"]);
        assert_ne!(
            requests[0].connection_id(),
            requests[1].connection_id(),
            "application and provider clients must not share a connection pool"
        );

        let application = managed
            .application
            .get(
                "managed.mtls",
                &mtls_server.url("localhost", "/application-identity"),
            )
            .await
            .expect("application client should present its configured identity");
        assert_eq!(application.status(), reqwest::StatusCode::OK);
        application
            .bytes()
            .await
            .expect("application mTLS body should complete");

        let request = managed
            .provider
            .prepare(
                Method::GET,
                &mtls_server.url("localhost", "/provider-without-identity"),
                None,
            )
            .expect("provider mTLS URL should prepare");
        let provider_error = managed
            .provider
            .execute_native("managed.provider", Method::GET, request)
            .await
            .expect_err("provider client must not present the application identity");
        assert!(matches!(
            provider_error,
            ErrorCategory::Connect | ErrorCategory::Transport
        ));
        let diagnostics = format!("{provider_error:?}");
        for sentinel in [ROOT_PEM_SENTINEL, IDENTITY_PEM_SENTINEL] {
            assert!(!diagnostics.contains(sentinel));
        }
        mtls_server
            .wait_for_requests(1, WAIT)
            .expect("only the authenticated application client should reach HTTP");
        let targets: Vec<_> = mtls_server
            .requests()
            .into_iter()
            .map(|request| request.target().to_owned())
            .collect();
        assert_eq!(targets, ["/application-identity"]);
    }

    #[cfg(feature = "default-tls")]
    #[tokio::test]
    async fn https_failure_never_falls_back_to_plain_http() {
        let plain = ScriptedServer::start([ResponseScript::ok(b"downgraded".to_vec())])
            .expect("plain downgrade trap should start");
        let managed = managed(
            HttpConfigBuilder::new()
                .http1_only()
                .connect_timeout(Duration::from_millis(300))
                .read_timeout(Duration::from_millis(300)),
        );
        let url = format!("https://localhost:{}/secure", plain.address().port());
        let result = managed.application.get("managed.https", &url).await;
        assert!(result.is_err());
        assert_eq!(plain.request_count(), 0);
    }

    #[tokio::test]
    async fn managed_native_error_reduction_drops_authenticated_url() {
        const SENTINEL: &str = "managed-native-url-secret-8db7a2";
        let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .expect("closed-port reservation should bind");
        let address = listener
            .local_addr()
            .expect("reserved address should exist");
        drop(listener);
        let managed = managed(HttpConfigBuilder::new().http1_only());
        let url = format!(
            "http://user-{SENTINEL}:password-{SENTINEL}@{address}/private?token={SENTINEL}"
        );
        let error = managed
            .application
            .get("managed.native", &url)
            .await
            .expect_err("closed port should fail safely");
        for formatted in [error.to_string(), format!("{error:?}")] {
            assert!(!formatted.contains(SENTINEL));
        }
        assert!(std::error::Error::source(&error).is_none());
    }

    #[cfg(feature = "default-tls")]
    #[test]
    fn invalid_root_and_identity_pem_are_reduced_without_reproducing_input() {
        const ROOT_SENTINEL: &str = "private-root-pem-sentinel-3af128";
        const IDENTITY_SENTINEL: &str = "private-identity-pem-sentinel-f90e31";
        let invalid_root = ManagedHttp::build(
            HttpConfigBuilder::new()
                .add_root_certificate_pem(ROOT_SENTINEL.as_bytes().to_vec())
                .resolve(),
            &RuntimeConfigBuilder::new().resolve(),
        );
        assert_safe_pem_error(invalid_root, ROOT_SENTINEL);

        let invalid_identity = ManagedHttp::build(
            HttpConfigBuilder::new()
                .identity_pem(IDENTITY_SENTINEL.as_bytes().to_vec())
                .resolve(),
            &RuntimeConfigBuilder::new().resolve(),
        );
        assert_safe_pem_error(invalid_identity, IDENTITY_SENTINEL);
    }

    #[cfg(feature = "default-tls")]
    fn assert_safe_pem_error(result: Result<ManagedHttp, ClientBuildError>, sentinel: &str) {
        let error = match result {
            Ok(_) => panic!("invalid PEM must fail construction"),
            Err(error) => error,
        };
        assert_eq!(error.kind(), ClientBuildErrorKind::HttpConfiguration);
        for formatted in [error.to_string(), format!("{error:?}")] {
            assert!(!formatted.contains(sentinel));
        }
        assert!(std::error::Error::source(&error).is_none());
    }

    #[cfg(feature = "http2")]
    struct H2RefusedServer {
        address: SocketAddr,
        physical_attempts: Arc<AtomicUsize>,
        worker: tokio::task::JoinHandle<()>,
    }

    #[cfg(feature = "http2")]
    impl H2RefusedServer {
        async fn start(refusals: usize, expected_attempts: usize) -> Self {
            let listener = TokioTcpListener::bind((Ipv4Addr::LOCALHOST, 0))
                .await
                .expect("HTTP/2 retry listener should bind");
            let address = listener.local_addr().expect("HTTP/2 address should exist");
            let physical_attempts = Arc::new(AtomicUsize::new(0));
            let counter = Arc::clone(&physical_attempts);
            let worker = tokio::spawn(async move {
                let (stream, _) = tokio::time::timeout(WAIT, listener.accept())
                    .await
                    .expect("HTTP/2 client should connect")
                    .expect("HTTP/2 accept should succeed");
                let mut connection = h2::server::handshake(stream)
                    .await
                    .expect("HTTP/2 handshake should succeed");
                while let Some(request) = tokio::time::timeout(WAIT, connection.accept())
                    .await
                    .expect("HTTP/2 request should arrive")
                {
                    let (_, mut respond) = request.expect("HTTP/2 request should parse");
                    let attempt = counter.fetch_add(1, Ordering::SeqCst) + 1;
                    if attempt <= refusals {
                        respond.send_reset(Reason::REFUSED_STREAM);
                    } else {
                        let response = http::Response::builder()
                            .status(200)
                            .body(())
                            .expect("HTTP/2 response should build");
                        respond
                            .send_response(response, true)
                            .expect("HTTP/2 response should send");
                    }
                    if attempt >= expected_attempts {
                        let _ =
                            tokio::time::timeout(Duration::from_millis(500), connection.accept())
                                .await;
                        break;
                    }
                }
            });
            Self {
                address,
                physical_attempts,
                worker,
            }
        }

        fn url(&self) -> String {
            format!("http://{}/retry", self.address)
        }

        async fn finish(self) {
            tokio::time::timeout(WAIT, self.worker)
                .await
                .expect("HTTP/2 worker should finish")
                .expect("HTTP/2 worker should not panic");
        }
    }

    #[cfg(feature = "http2")]
    #[tokio::test]
    async fn application_retry_is_off_by_default_at_one_boundary_call() {
        let server = H2RefusedServer::start(1, 1).await;
        let attempts = Arc::clone(&server.physical_attempts);
        let managed = managed(HttpConfigBuilder::new().http2_prior_knowledge());
        let boundary_calls = AtomicUsize::new(0);
        boundary_calls.fetch_add(1, Ordering::SeqCst);
        let result = managed
            .application
            .get("managed.retry", &server.url())
            .await;
        assert!(result.is_err());
        server.finish().await;
        assert_eq!(boundary_calls.load(Ordering::SeqCst), 1);
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[cfg(feature = "http2")]
    #[tokio::test]
    async fn reqwest_default_retry_requires_opt_in_and_stays_inside_boundary() {
        let server = H2RefusedServer::start(1, 2).await;
        let attempts = Arc::clone(&server.physical_attempts);
        let managed = managed(
            HttpConfigBuilder::new()
                .http2_prior_knowledge()
                .retry_reqwest_default(),
        );
        let boundary_calls = AtomicUsize::new(0);
        boundary_calls.fetch_add(1, Ordering::SeqCst);
        let response = managed
            .application
            .get("managed.retry", &server.url())
            .await
            .expect("native default retry should recover REFUSED_STREAM");
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        server.finish().await;
        assert_eq!(boundary_calls.load(Ordering::SeqCst), 1);
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }
}
